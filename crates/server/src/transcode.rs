//! Transcode sessions: one ffmpeg process per active HLS playback.
//!
//! The player gets a full VOD playlist up front (see `kinora_core::hls`).
//! When it asks for segment N:
//! - if the file exists, serve it;
//! - if ffmpeg is already working towards N, wait for it;
//! - otherwise (a seek), restart ffmpeg at N.
//!
//! If ffmpeg dies before producing the segment (a GPU driver crash, say), it
//! is restarted on a more conservative pipeline (`fallback_profile`) until
//! there's nothing left to fall back to.
//!
//! Idle sessions are killed and their folders deleted. A paused player stops
//! asking for segments, so it pings `keepalive` to keep its session.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};

use kinora_core::ffmpeg::{fallback_profile, hls_args, HlsJob, HwProfile};
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
    /// Last line ffmpeg wrote to stderr, for error reports.
    last_stderr: Arc<StdMutex<Option<String>>>,
}

/// Why a segment couldn't be served.
#[derive(Debug, thiserror::Error)]
pub enum SegmentError {
    #[error("no such session")]
    NoSession,
    #[error("segment out of range")]
    OutOfRange,
    #[error(transparent)]
    Failed(#[from] anyhow::Error),
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

/// Live state of one session, for the player's trace view.
#[derive(Debug, Clone, Serialize)]
pub struct SessionStatus {
    /// Is an ffmpeg process running right now?
    pub running: bool,
    /// Segment the current ffmpeg process started at (it restarts on a seek).
    pub started_at_segment: u32,
    /// Last segment the player asked for.
    pub last_requested_segment: u32,
    /// Highest finished segment since the last (re)start.
    pub transcoded_through: Option<u32>,
    pub segment_seconds: u32,
    pub hw: String,
    pub running_seconds: u64,
    /// Last line ffmpeg printed, if any.
    pub last_ffmpeg_line: Option<String>,
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
            last_stderr: Arc::default(),
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

    /// Mark the session as in use (the player is open, maybe paused) and report its state.
    pub async fn keepalive(&self, id: &str) -> Option<SessionStatus> {
        let s = self.get(id).await?;
        let mut s = s.lock().await;
        s.last_access = Instant::now();
        let running = match s.child.as_mut() {
            Some(c) => matches!(c.try_wait(), Ok(None)),
            None => false,
        };
        Some(SessionStatus {
            running,
            started_at_segment: s.started_at_segment,
            last_requested_segment: s.last_segment,
            transcoded_through: latest_segment(&s.dir, s.started_at_segment),
            segment_seconds: SEGMENT_SECONDS,
            hw: s.input.hw.accel.label().to_string(),
            running_seconds: s.created.elapsed().as_secs(),
            last_ffmpeg_line: s.last_stderr.lock().ok().and_then(|l| l.clone()),
        })
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
    pub async fn segment(&self, id: &str, index: u32) -> Result<PathBuf, SegmentError> {
        let session = self.get(id).await.ok_or(SegmentError::NoSession)?;
        let deadline = Instant::now() + SEGMENT_TIMEOUT;
        {
            let s = session.lock().await;
            let count = segment_count(s.input.duration_ms, SEGMENT_SECONDS);
            if index >= count {
                return Err(SegmentError::OutOfRange);
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

                let exit = match s.child.as_mut() {
                    Some(c) => c.try_wait().map_err(anyhow::Error::from)?,
                    None => None,
                };
                let running = s.child.is_some() && exit.is_none();
                let progress = latest_segment(&s.dir, s.started_at_segment).unwrap_or(s.started_at_segment);
                let in_reach = running && index >= s.started_at_segment && index <= progress + MAX_WAIT_AHEAD;
                if !in_reach {
                    let died_before_it = index >= s.started_at_segment && index <= progress;
                    match exit {
                        Some(status) if died_before_it => {
                            let stderr = s.last_stderr.lock().ok().and_then(|l| l.clone()).unwrap_or_default();
                            tracing::warn!(
                                session = %s.dir.display(),
                                %status,
                                stderr,
                                "ffmpeg exited before producing segment {index}"
                            );
                            s.child = None;
                            let fallback = if status.success() { None } else { fallback_profile(&s.input.hw) };
                            let Some(next) = fallback else {
                                return Err(anyhow::anyhow!(
                                    "transcoder stopped before producing segment {index} ({status}) {stderr}"
                                )
                                .into());
                            };
                            tracing::warn!(
                                session = %s.dir.display(),
                                accel = next.accel.label(),
                                tonemapper = ?next.tonemapper,
                                "retrying with a more conservative pipeline"
                            );
                            s.input.hw = next;
                        }
                        _ => {}
                    }
                    self.start_ffmpeg(&mut s, index).await?;
                }
            }
            if Instant::now() > deadline {
                return Err(anyhow::anyhow!("timed out waiting for segment {index}").into());
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
        if let Ok(mut last) = s.last_stderr.lock() {
            *last = None;
        }
        if let Some(stderr) = child.stderr.take() {
            let last_stderr = s.last_stderr.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    tracing::warn!(target: "ffmpeg", "{line}");
                    if let Ok(mut last) = last_stderr.lock() {
                        *last = Some(line);
                    }
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
