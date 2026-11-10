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
//! Copied HEVC goes out as fragmented MP4, which also needs an init segment.
//! Every ffmpeg run writes its own (`init_N.mp4`); the first complete one is
//! copied to `init.mp4` and served for the whole session. They are all the
//! same, since the streams and their timestamps don't depend on the start.
//!
//! ffmpeg is paused (stopped) once it is about two minutes ahead of the
//! player and started again before the player runs out, and segments well
//! behind the player are deleted, so a session's folder stays small. That
//! matters because the transcode folder is often in RAM (/tmp on Unraid).
//!
//! Idle sessions are killed and their folders deleted. A paused player stops
//! asking for segments, so it pings `keepalive` to keep its session. When a
//! session ends, the source file's page cache is let go (see `page_cache`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant, SystemTime};

use kinora_core::ffmpeg::{fallback_profile, hls_args, HlsJob, HwProfile};
use kinora_core::hls::{
    run_init_name, segment_count, segment_name, segments_to_delete, throttle, Throttle, INIT_NAME, SEGMENT_SECONDS,
};
use kinora_core::playback::{PlaybackPlan, SegmentFormat};
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
/// How often sessions are checked for throttling and cleanup.
const THROTTLE_INTERVAL: Duration = Duration::from_secs(2);

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
    /// When the current ffmpeg process started.
    run_started: SystemTime,
    /// `init.mp4` is in place (fMP4 sessions only).
    init_ready: bool,
    /// ffmpeg was stopped because it got far enough ahead of the player.
    throttled: bool,
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

/// Live state of one session, for the player's trace view.
#[derive(Debug, Clone, Serialize)]
pub struct SessionStatus {
    /// Is an ffmpeg process running right now?
    pub running: bool,
    /// ffmpeg is paused because it is far enough ahead of the player.
    pub throttled: bool,
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
    /// How much faster than realtime the current ffmpeg run is going
    /// (2.0 = two seconds of video per second). Missing until it has made a segment.
    pub speed: Option<f64>,
    /// Frames per second ffmpeg is producing (speed × source frame rate).
    pub fps: Option<f64>,
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
            let mut ticks: u32 = 0;
            loop {
                tokio::time::sleep(THROTTLE_INTERVAL).await;
                reaper.throttle_all().await;
                ticks = ticks.wrapping_add(1);
                if ticks.is_multiple_of(8) {
                    reaper.reap_idle().await;
                }
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
            run_started: SystemTime::now(),
            init_ready: false,
            throttled: false,
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

    /// Duration and segment packaging, for the playlist.
    pub async fn playlist_info(&self, id: &str) -> Option<(u64, SegmentFormat)> {
        let s = self.get(id).await?;
        let s = s.lock().await;
        Some((s.input.duration_ms, s.input.plan.segments))
    }

    /// Path of the fMP4 init segment, starting ffmpeg if nothing has written one yet.
    pub async fn init_segment(&self, id: &str) -> Result<PathBuf, SegmentError> {
        let session = self.get(id).await.ok_or(SegmentError::NoSession)?;
        let deadline = Instant::now() + SEGMENT_TIMEOUT;
        loop {
            {
                let mut s = session.lock().await;
                if s.input.plan.segments != SegmentFormat::Fmp4 {
                    return Err(SegmentError::OutOfRange);
                }
                s.last_access = Instant::now();
                promote_init(&mut s).await;
                if s.init_ready {
                    return Ok(s.dir.join(INIT_NAME));
                }
                let running = match s.child.as_mut() {
                    Some(c) => matches!(c.try_wait(), Ok(None)),
                    None => false,
                };
                if !running {
                    // Usually the first request of a session: start where the
                    // player will most likely ask for its first segment.
                    let start = s.last_segment;
                    self.start_ffmpeg(&mut s, start).await?;
                }
            }
            if Instant::now() > deadline {
                return Err(anyhow::anyhow!("timed out waiting for the init segment").into());
            }
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
    }

    /// Mark the session as in use (the player is open, maybe paused) and report its state.
    pub async fn keepalive(&self, id: &str) -> Option<SessionStatus> {
        let s = self.get(id).await?;
        let mut s = s.lock().await;
        s.last_access = Instant::now();
        Some(status(&mut s))
    }

    /// The state of every session, for the admin dashboard (doesn't keep them alive).
    pub async fn all_status(&self) -> HashMap<String, SessionStatus> {
        let sessions: Vec<(String, Arc<Mutex<Session>>)> =
            self.sessions.lock().await.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        let mut out = HashMap::new();
        for (id, s) in sessions {
            let mut s = s.lock().await;
            out.insert(id, status(&mut s));
        }
        out
    }

    pub async fn stop(&self, id: &str) {
        let removed = self.sessions.lock().await.remove(id);
        if let Some(s) = removed {
            let mut s = s.lock().await;
            kill(&mut s).await;
            let _ = tokio::fs::remove_dir_all(&s.dir).await;
            if !self.is_playing(&s.input.file_path).await {
                crate::page_cache::release(&s.input.file_path);
            }
        }
    }

    /// Is any session reading this file?
    pub async fn is_playing(&self, file_path: &str) -> bool {
        let sessions: Vec<Arc<Mutex<Session>>> = self.sessions.lock().await.values().cloned().collect();
        for s in sessions {
            if s.lock().await.input.file_path == file_path {
                return true;
            }
        }
        false
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
                promote_init(&mut s).await;
                let path = s.dir.join(segment_name(index, s.input.plan.segments));
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
        let run_started = SystemTime::now();
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
        s.run_started = run_started;
        s.throttled = false;
        Ok(())
    }

    /// Pause, resume and clean up every session (see `kinora_core::hls::throttle`).
    async fn throttle_all(&self) {
        let sessions: Vec<Arc<Mutex<Session>>> = self.sessions.lock().await.values().cloned().collect();
        for session in sessions {
            // A session busy serving a segment is checked next time.
            let Ok(mut s) = session.try_lock() else { continue };
            if let Err(e) = self.throttle_session(&mut s).await {
                tracing::warn!(session = %s.dir.display(), error = %e, "couldn't resume the transcoder");
            }
        }
    }

    async fn throttle_session(&self, s: &mut Session) -> anyhow::Result<()> {
        let running = match s.child.as_mut() {
            Some(c) => matches!(c.try_wait(), Ok(None)),
            None => false,
        };
        let produced = latest_segment(&s.dir, s.started_at_segment);
        let count = segment_count(s.input.duration_ms, SEGMENT_SECONDS);
        match throttle(running, s.throttled, s.last_segment, produced, count) {
            Throttle::Keep => {}
            Throttle::Pause => {
                tracing::info!(session = %s.dir.display(), produced, requested = s.last_segment, "pausing transcoder, far enough ahead");
                kill(s).await;
                s.throttled = true;
            }
            Throttle::Resume(from) => {
                tracing::info!(session = %s.dir.display(), from, requested = s.last_segment, "resuming transcoder");
                self.start_ffmpeg(s, from).await?;
            }
        }
        let existing = finished_segments(&s.dir);
        for index in segments_to_delete(&existing, s.last_segment) {
            // The file may be open on Windows; it goes next time.
            let _ = std::fs::remove_file(s.dir.join(segment_name(index, s.input.plan.segments)));
        }
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

fn status(s: &mut Session) -> SessionStatus {
    let running = match s.child.as_mut() {
        Some(c) => matches!(c.try_wait(), Ok(None)),
        None => false,
    };
    let transcoded_through = latest_segment(&s.dir, s.started_at_segment);
    let speed = if running {
        let made = transcoded_through.map(|n| n + 1 - s.started_at_segment).unwrap_or(0);
        let elapsed = s.run_started.elapsed().map(|d| d.as_secs_f64()).unwrap_or(0.0);
        transcode_speed(made, SEGMENT_SECONDS, elapsed)
    } else {
        None
    };
    SessionStatus {
        running,
        throttled: s.throttled,
        started_at_segment: s.started_at_segment,
        last_requested_segment: s.last_segment,
        transcoded_through,
        segment_seconds: SEGMENT_SECONDS,
        hw: s.input.hw.accel.label().to_string(),
        running_seconds: s.created.elapsed().as_secs(),
        last_ffmpeg_line: s.last_stderr.lock().ok().and_then(|l| l.clone()),
        speed,
        fps: speed.zip(s.input.frame_rate).map(|(sp, fr)| sp * fr),
    }
}

/// Seconds of video made per second of wall time, once there's something to go on.
fn transcode_speed(segments_made: u32, segment_seconds: u32, elapsed_secs: f64) -> Option<f64> {
    if segments_made == 0 || elapsed_secs < 1.0 {
        return None;
    }
    Some(f64::from(segments_made * segment_seconds) / elapsed_secs)
}

async fn kill(s: &mut Session) {
    if let Some(mut child) = s.child.take() {
        let _ = child.kill().await;
    }
}

/// Once the current ffmpeg run has finished its first segment, its init
/// segment is complete: copy it to `init.mp4` (via a temp file, so a reader
/// never sees half of it).
async fn promote_init(s: &mut Session) {
    if s.init_ready || s.input.plan.segments != SegmentFormat::Fmp4 {
        return;
    }
    let first = s.dir.join(segment_name(s.started_at_segment, SegmentFormat::Fmp4));
    let written_by_this_run =
        std::fs::metadata(&first).and_then(|m| m.modified()).map(|t| t >= s.run_started).unwrap_or(false);
    if !written_by_this_run {
        return;
    }
    let src = s.dir.join(run_init_name(s.started_at_segment));
    let tmp = s.dir.join(format!("{INIT_NAME}.tmp"));
    let copied = async {
        tokio::fs::copy(&src, &tmp).await?;
        tokio::fs::rename(&tmp, s.dir.join(INIT_NAME)).await
    };
    match copied.await {
        Ok(()) => s.init_ready = true,
        Err(e) => tracing::warn!(session = %s.dir.display(), error = %e, "couldn't set up the init segment"),
    }
}

/// Indexes of the finished segments in the session folder.
fn finished_segments(dir: &Path) -> Vec<u32> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    entries
        .filter_map(|e| e.ok())
        .filter_map(|e| kinora_core::hls::parse_segment_name(&e.file_name().to_string_lossy()))
        .collect()
}

/// Highest finished segment index >= `from` in the session folder.
fn latest_segment(dir: &Path, from: u32) -> Option<u32> {
    finished_segments(dir).into_iter().filter(|n| *n >= from).max()
}

#[cfg(test)]
mod tests {
    use super::transcode_speed;

    #[test]
    fn speed_needs_a_segment_and_some_time() {
        assert_eq!(transcode_speed(0, 6, 10.0), None);
        assert_eq!(transcode_speed(3, 6, 0.5), None);
        assert_eq!(transcode_speed(4, 6, 12.0), Some(2.0));
    }
}
