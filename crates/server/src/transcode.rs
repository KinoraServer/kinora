//! Transcode sessions: one ffmpeg process per active HLS playback.
//!
//! The player gets a full VOD playlist up front (see `kinora_core::hls`).
//! When it asks for segment N:
//! - if the file exists, serve it;
//! - if ffmpeg is already working towards N, wait for it;
//! - otherwise (a seek), restart ffmpeg at N.
//!
//! Idle sessions are killed and their folders deleted.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};

use kinora_core::ffmpeg::{hls_args, HlsJob, HwProfile};
use kinora_core::hls::{segment_count, segment_name, SEGMENT_SECONDS};
use kinora_core::playback::PlaybackPlan;
use serde::Serialize;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

use crate::db::new_id;

/// How far ahead of ffmpeg's progress a request may be before we restart
/// instead of waiting.
const MAX_WAIT_AHEAD: u32 = 4;
const IDLE_TIMEOUT: Duration = Duration::from_secs(90);
const SEGMENT_TIMEOUT: Duration = Duration::from_secs(60);

pub struct SessionInput {
    pub user_id: String,
    pub username: String,
    pub item_id: String,
    pub title: String,
    pub file_path: String,
    pub duration_ms: u64,
    pub video_index: Option<u32>,
    pub frame_rate: Option<f64>,
    pub source_is_dolby_vision: bool,
    pub plan: PlaybackPlan,
    pub hw: HwProfile,
}

struct Session {
    input: SessionInput,
    dir: PathBuf,
    child: Option<Child>,
    /// Segment the current ffmpeg process started at.
    started_at_segment: u32,
    last_access: Instant,
    created: Instant,
    last_segment: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionInfo {
    pub id: String,
    pub username: String,
    pub item_id: String,
    pub title: String,
    pub method: kinora_core::playback::Method,
    pub plan: PlaybackPlan,
    pub hw: String,
    pub position_ms: u64,
    pub running_seconds: u64,
}

#[derive(Clone)]
pub struct TranscodeManager {
    root: PathBuf,
    ffmpeg: String,
    sessions: Arc<Mutex<HashMap<String, Arc<Mutex<Session>>>>>,
}

impl TranscodeManager {
    pub fn new(base: PathBuf, ffmpeg: String) -> Self {
        // Our own sub-folder, so clearing leftovers from a previous run can
        // never delete anything else in a user-chosen directory.
        let root = base.join("kinora-sessions");
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::create_dir_all(&root);
        let mgr = TranscodeManager { root, ffmpeg, sessions: Arc::default() };
        let reaper = mgr.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(15)).await;
                reaper.reap_idle().await;
            }
        });
        mgr
    }

    pub async fn create(&self, input: SessionInput) -> std::io::Result<String> {
        let id = new_id();
        let dir = self.root.join(&id);
        tokio::fs::create_dir_all(&dir).await?;
        let session = Session {
            input,
            dir,
            child: None,
            started_at_segment: 0,
            last_access: Instant::now(),
            created: Instant::now(),
            last_segment: 0,
        };
        self.sessions.lock().await.insert(id.clone(), Arc::new(Mutex::new(session)));
        Ok(id)
    }

    async fn get(&self, id: &str) -> Option<Arc<Mutex<Session>>> {
        self.sessions.lock().await.get(id).cloned()
    }

    pub async fn owner(&self, id: &str) -> Option<String> {
        let s = self.get(id).await?;
        let s = s.lock().await;
        Some(s.input.user_id.clone())
    }

    pub async fn duration_ms(&self, id: &str) -> Option<u64> {
        let s = self.get(id).await?;
        let s = s.lock().await;
        Some(s.input.duration_ms)
    }

    pub async fn stop(&self, id: &str) {
        let removed = self.sessions.lock().await.remove(id);
        if let Some(s) = removed {
            let mut s = s.lock().await;
            kill(&mut s).await;
            let _ = tokio::fs::remove_dir_all(&s.dir).await;
        }
    }

    pub async fn list(&self) -> Vec<SessionInfo> {
        let sessions: Vec<(String, Arc<Mutex<Session>>)> =
            self.sessions.lock().await.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        let mut out = Vec::new();
        for (id, s) in sessions {
            let s = s.lock().await;
            out.push(SessionInfo {
                id,
                username: s.input.username.clone(),
                item_id: s.input.item_id.clone(),
                title: s.input.title.clone(),
                method: s.input.plan.method,
                plan: s.input.plan.clone(),
                hw: s.input.hw.accel.label().to_string(),
                position_ms: u64::from(s.last_segment) * u64::from(SEGMENT_SECONDS) * 1000,
                running_seconds: s.created.elapsed().as_secs(),
            });
        }
        out
    }

    /// Path of a finished segment, starting or restarting ffmpeg as needed.
    pub async fn segment(&self, id: &str, index: u32) -> anyhow::Result<PathBuf> {
        let session = self.get(id).await.ok_or_else(|| anyhow::anyhow!("no such session"))?;
        let deadline = Instant::now() + SEGMENT_TIMEOUT;
        {
            let s = session.lock().await;
            let count = segment_count(s.input.duration_ms, SEGMENT_SECONDS);
            if index >= count {
                anyhow::bail!("segment out of range");
            }
        }
        loop {
            {
                let mut s = session.lock().await;
                s.last_access = Instant::now();
                s.last_segment = index;
                let path = s.dir.join(segment_name(index));
                // ffmpeg writes "seg_N.ts.tmp" and renames when done (hls_flags temp_file),
                // so if the file exists it's complete.
                if path.exists() {
                    return Ok(path);
                }

                let running = match s.child.as_mut() {
                    Some(c) => matches!(c.try_wait(), Ok(None)),
                    None => false,
                };
                let progress = latest_segment(&s.dir, s.started_at_segment).unwrap_or(s.started_at_segment);
                let in_reach = running && index >= s.started_at_segment && index <= progress + MAX_WAIT_AHEAD;
                if !in_reach {
                    if !running && s.child.is_some() && index >= s.started_at_segment && index <= progress {
                        anyhow::bail!("transcoder stopped before producing segment {index}");
                    }
                    self.start_ffmpeg(&mut s, index).await?;
                }
            }
            if Instant::now() > deadline {
                anyhow::bail!("timed out waiting for segment {index}");
            }
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
    }

    async fn start_ffmpeg(&self, s: &mut Session, start_segment: u32) -> anyhow::Result<()> {
        kill(s).await;
        let output_dir = s.dir.display().to_string();
        let args = hls_args(&HlsJob {
            input: &s.input.file_path,
            plan: &s.input.plan,
            hw: &s.input.hw,
            video_index: s.input.video_index,
            source_is_dolby_vision: s.input.source_is_dolby_vision,
            frame_rate: s.input.frame_rate,
            segment_seconds: SEGMENT_SECONDS,
            start_segment,
            output_dir: &output_dir,
        });
        tracing::info!(session = %s.dir.display(), start_segment, "starting ffmpeg: {}", args.join(" "));
        let mut child = Command::new(&self.ffmpeg)
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;
        if let Some(stderr) = child.stderr.take() {
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    tracing::warn!(target: "ffmpeg", "{line}");
                }
            });
        }
        s.child = Some(child);
        s.started_at_segment = start_segment;
        Ok(())
    }

    async fn reap_idle(&self) {
        let idle: Vec<String> = {
            let sessions = self.sessions.lock().await;
            let mut out = Vec::new();
            for (id, s) in sessions.iter() {
                if let Ok(s) = s.try_lock() {
                    if s.last_access.elapsed() > IDLE_TIMEOUT {
                        out.push(id.clone());
                    }
                }
            }
            out
        };
        for id in idle {
            tracing::info!(session = %id, "stopping idle transcode");
            self.stop(&id).await;
        }
    }
}

async fn kill(s: &mut Session) {
    if let Some(mut child) = s.child.take() {
        let _ = child.kill().await;
    }
}

/// Highest finished segment index >= `from` in the session folder.
fn latest_segment(dir: &Path, from: u32) -> Option<u32> {
    std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter_map(|e| kinora_core::hls::parse_segment_name(&e.file_name().to_string_lossy()))
        .filter(|n| *n >= from)
        .max()
}
