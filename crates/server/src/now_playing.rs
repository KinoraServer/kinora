//! Everything that is playing right now, whatever the method (direct play,
//! direct stream, transcode), for the admin dashboard.
//!
//! An entry is added when a player starts a playback and kept alive by the
//! player's heartbeats (position, paused, a few stats) and, for clients that
//! don't send heartbeats yet, by progress saves and stream requests. It goes
//! away when the player says it stopped or after `TIMEOUT` without news.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use kinora_core::media::MediaInfo;
use kinora_core::playback::{AudioAction, Method, PlaybackPlan, VideoAction};
use serde::{Deserialize, Serialize};

use crate::db::now_ms;
use crate::transcode::SessionStatus;

/// How long an entry lives without a heartbeat. The web player sends one
/// every 5 s, also while paused.
pub const TIMEOUT: Duration = Duration::from_secs(45);
const SWEEP_INTERVAL: Duration = Duration::from_secs(10);
const MAX_TEXT: usize = 160;
const MAX_ERRORS: usize = 8;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlayState {
    #[default]
    Playing,
    Paused,
    Buffering,
}

/// What the player says about itself (all optional: send what you have).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ClientStats {
    pub render_fps: Option<f64>,
    pub dropped_frames: Option<u64>,
    pub total_frames: Option<u64>,
    pub stalls: Option<u32>,
    pub stalled_ms: Option<u64>,
    pub buffered_ahead_s: Option<f64>,
    pub download_bps: Option<u64>,
    pub stream_bitrate_bps: Option<u64>,
    pub decoded_size: Option<String>,
    pub player: Option<String>,
    pub recoveries: Option<u32>,
    pub recent_errors: Vec<String>,
}

impl ClientStats {
    /// Keep what a client sends small: this is kept in memory and shown to admins.
    fn trimmed(mut self) -> Self {
        self.decoded_size = self.decoded_size.map(|s| clip(&s));
        self.player = self.player.map(|s| clip(&s));
        let skip = self.recent_errors.len().saturating_sub(MAX_ERRORS);
        self.recent_errors = self.recent_errors.iter().skip(skip).map(|s| clip(s)).collect();
        self
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClientInfo {
    /// The app, e.g. "Kinora web".
    pub name: Option<String>,
    /// Browser and system, e.g. "Chrome on macOS".
    pub device: Option<String>,
    /// Network address the playback was started from.
    #[serde(default)]
    pub address: Option<String>,
}

impl ClientInfo {
    /// What the client said about itself, filled in from its User-Agent where it said nothing.
    pub fn resolve(given: Option<ClientInfo>, user_agent: Option<&str>, address: Option<String>) -> ClientInfo {
        let given = given.unwrap_or_default();
        let (ua_name, ua_device) = describe_user_agent(user_agent.unwrap_or(""));
        ClientInfo {
            name: given.name.filter(|s| !s.trim().is_empty()).map(|s| clip(&s)).or(ua_name),
            device: given.device.filter(|s| !s.trim().is_empty()).map(|s| clip(&s)).or(ua_device),
            address,
        }
    }
}

/// The title being played, for the card.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ItemSummary {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub series_title: Option<String>,
    pub season: Option<i64>,
    pub episode: Option<i64>,
    pub year: Option<i64>,
}

pub struct PlayInput {
    pub user_id: String,
    pub username: String,
    pub file_id: String,
    pub item: ItemSummary,
    pub method: Method,
    pub plan: PlaybackPlan,
    pub hardware: String,
    pub client: ClientInfo,
    pub media: MediaInfo,
    pub url: String,
}

struct Entry {
    input: Arc<PlayInput>,
    started_at: i64,
    last_seen: Instant,
    last_seen_at: i64,
    position_ms: Option<u64>,
    /// When `position_ms` was reported.
    position_seen: Instant,
    state: PlayState,
    stats: Option<ClientStats>,
}

/// One heartbeat from a player.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Heartbeat {
    pub position_ms: Option<u64>,
    pub state: Option<PlayState>,
    pub stats: Option<ClientStats>,
}

/// One row of `GET /api/sessions`.
#[derive(Debug, Serialize)]
pub struct StreamInfo {
    pub id: String,
    pub method: Method,
    pub state: PlayState,
    pub user: StreamUser,
    pub client: ClientInfo,
    pub item: ItemSummary,
    pub file_id: String,
    /// Where the viewer is now (moved on from the last report while playing).
    pub position_ms: Option<u64>,
    pub duration_ms: u64,
    pub started_at: i64,
    pub last_seen_at: i64,
    /// Bits per second going out to this viewer (0 while paused).
    pub bitrate_bps: Option<u64>,
    pub url: String,
    pub hardware: String,
    pub media: MediaInfo,
    pub plan: PlaybackPlan,
    pub transcoder: Option<SessionStatus>,
    pub client_stats: Option<ClientStats>,
}

#[derive(Debug, Serialize)]
pub struct StreamUser {
    pub id: String,
    pub username: String,
}

/// What `NowPlaying::remove` took off the list.
pub struct Removed {
    pub item_id: String,
    pub file_id: String,
    pub position_ms: Option<u64>,
}

#[derive(Clone, Default)]
pub struct NowPlaying {
    entries: Arc<Mutex<HashMap<String, Entry>>>,
}

impl NowPlaying {
    /// A registry with a background task that drops entries nobody has heard from.
    pub fn new() -> Self {
        let np = NowPlaying::default();
        let sweeper = np.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(SWEEP_INTERVAL).await;
                sweeper.prune();
            }
        });
        np
    }

    pub fn insert(&self, id: String, input: PlayInput, start_ms: Option<u64>) {
        let now = Instant::now();
        let entry = Entry {
            input: Arc::new(input),
            started_at: now_ms(),
            last_seen: now,
            last_seen_at: now_ms(),
            position_ms: start_ms,
            position_seen: now,
            state: PlayState::Buffering,
            stats: None,
        };
        self.lock().insert(id, entry);
    }

    pub fn owner(&self, id: &str) -> Option<String> {
        self.lock().get(id).map(|e| e.input.user_id.clone())
    }

    /// Record a heartbeat. False when the entry is gone (the player should start again).
    pub fn heartbeat(&self, id: &str, hb: Heartbeat) -> bool {
        let mut entries = self.lock();
        let Some(e) = entries.get_mut(id) else { return false };
        touch(e);
        if let Some(p) = hb.position_ms {
            e.position_ms = Some(p);
            e.position_seen = Instant::now();
        }
        if let Some(s) = hb.state {
            e.state = s;
        }
        if let Some(stats) = hb.stats {
            e.stats = Some(stats.trimmed());
        }
        true
    }

    /// A progress save from a client that doesn't send heartbeats.
    pub fn progress(&self, user_id: &str, item_id: &str, position_ms: u64) {
        let mut entries = self.lock();
        for e in entries.values_mut().filter(|e| e.input.user_id == user_id && e.input.item.id == item_id) {
            touch(e);
            e.position_ms = Some(position_ms);
            e.position_seen = Instant::now();
            if e.stats.is_none() {
                e.state = PlayState::Playing;
            }
        }
    }

    /// The viewer fetched part of a file (direct play range requests).
    pub fn touch_file(&self, user_id: &str, file_id: &str) {
        let mut entries = self.lock();
        for e in entries.values_mut().filter(|e| e.input.user_id == user_id && e.input.file_id == file_id) {
            touch(e);
        }
    }

    /// The viewer fetched a segment of this HLS session.
    pub fn touch(&self, id: &str) {
        if let Some(e) = self.lock().get_mut(id) {
            touch(e);
        }
    }

    /// Remove an entry, returning its item, file and last known position.
    pub fn remove(&self, id: &str) -> Option<Removed> {
        let e = self.lock().remove(id)?;
        let position_ms = current_position(&e);
        Some(Removed { item_id: e.input.item.id.clone(), file_id: e.input.file_id.clone(), position_ms })
    }

    /// Is anyone still playing this file?
    pub fn is_playing_file(&self, file_id: &str) -> bool {
        self.lock().values().any(|e| e.input.file_id == file_id)
    }

    fn prune(&self) {
        self.lock().retain(|_, e| e.last_seen.elapsed() < TIMEOUT);
    }

    /// Everything playing, oldest first. `transcoder` looks up an HLS session's status.
    pub fn list(&self, transcoder: impl Fn(&str) -> Option<SessionStatus>) -> Vec<StreamInfo> {
        self.prune();
        let entries = self.lock();
        let mut out: Vec<StreamInfo> = entries
            .iter()
            .map(|(id, e)| {
                let i = &e.input;
                StreamInfo {
                    id: id.clone(),
                    method: i.method,
                    state: e.state,
                    user: StreamUser { id: i.user_id.clone(), username: i.username.clone() },
                    client: i.client.clone(),
                    item: i.item.clone(),
                    file_id: i.file_id.clone(),
                    position_ms: current_position(e),
                    duration_ms: i.media.duration_ms,
                    started_at: e.started_at,
                    last_seen_at: e.last_seen_at,
                    bitrate_bps: bitrate(i, e),
                    url: i.url.clone(),
                    hardware: i.hardware.clone(),
                    media: i.media.clone(),
                    plan: i.plan.clone(),
                    transcoder: if i.method == Method::DirectPlay { None } else { transcoder(id) },
                    client_stats: e.stats.clone(),
                }
            })
            .collect();
        out.sort_by_key(|s| s.started_at);
        out
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Entry>> {
        self.entries.lock().unwrap_or_else(|p| p.into_inner())
    }
}

fn touch(e: &mut Entry) {
    e.last_seen = Instant::now();
    e.last_seen_at = now_ms();
}

/// The last reported position, moved on by the time since while playing.
fn current_position(e: &Entry) -> Option<u64> {
    let p = e.position_ms?;
    let moved = if e.state == PlayState::Playing { e.position_seen.elapsed().as_millis() as u64 } else { 0 };
    let p = p + moved;
    let d = e.input.media.duration_ms;
    Some(if d > 0 { p.min(d) } else { p })
}

/// Bits per second going out: what the player measured, or what the plan says.
fn bitrate(i: &PlayInput, e: &Entry) -> Option<u64> {
    if e.state == PlayState::Paused {
        return Some(0);
    }
    if let Some(b) = e.stats.as_ref().and_then(|s| s.stream_bitrate_bps).filter(|b| *b > 0) {
        return Some(b);
    }
    planned_bitrate(&i.plan, &i.media)
}

/// Expected bitrate of a stream from its plan and source.
pub fn planned_bitrate(plan: &PlaybackPlan, media: &MediaInfo) -> Option<u64> {
    if plan.method != Method::Transcode {
        return media.bitrate;
    }
    let source_audio = plan
        .audio_index
        .and_then(|idx| media.audio.iter().find(|a| a.index == idx))
        .or(media.audio.first())
        .and_then(|a| a.bitrate);
    let video = match &plan.video {
        Some(VideoAction::Encode { bitrate, .. }) => Some(*bitrate),
        Some(VideoAction::Copy) => media.video.as_ref().and_then(|v| v.bitrate),
        None => Some(0),
    };
    let audio = match &plan.audio {
        Some(AudioAction::Encode { bitrate, .. }) => Some(*bitrate),
        Some(AudioAction::Copy) => source_audio,
        None => Some(0),
    };
    match (video, audio) {
        (Some(v), Some(a)) => Some(v + a),
        (Some(v), None) => Some(v),
        _ => None,
    }
}

fn clip(s: &str) -> String {
    let s = s.trim();
    match s.char_indices().nth(MAX_TEXT) {
        Some((i, _)) => format!("{}…", &s[..i]),
        None => s.to_string(),
    }
}

/// A rough "app" and "browser on system" from a User-Agent, for clients
/// that don't describe themselves.
pub fn describe_user_agent(ua: &str) -> (Option<String>, Option<String>) {
    let l = ua.to_lowercase();
    if l.is_empty() {
        return (None, None);
    }
    if l.contains("okhttp") || l.contains("exoplayer") || l.contains("androidx.media3") {
        return (Some("Kinora Android".into()), None);
    }
    let os = if l.contains("android") {
        Some("Android")
    } else if l.contains("iphone") {
        Some("iPhone")
    } else if l.contains("ipad") {
        Some("iPad")
    } else if l.contains("cros") {
        Some("ChromeOS")
    } else if l.contains("mac os x") || l.contains("macintosh") {
        Some("macOS")
    } else if l.contains("windows") {
        Some("Windows")
    } else if l.contains("linux") {
        Some("Linux")
    } else {
        None
    };
    let browser = if l.contains("edg/") {
        Some("Edge")
    } else if l.contains("opr/") || l.contains("opera") {
        Some("Opera")
    } else if l.contains("firefox/") || l.contains("fxios/") {
        Some("Firefox")
    } else if l.contains("chrome/") || l.contains("crios/") {
        Some("Chrome")
    } else if l.contains("safari/") {
        Some("Safari")
    } else {
        None
    };
    let device = match (browser, os) {
        (Some(b), Some(o)) => Some(format!("{b} on {o}")),
        (Some(b), None) => Some(b.to_string()),
        (None, Some(o)) => Some(o.to_string()),
        (None, None) => None,
    };
    (browser.map(|_| "Kinora web".to_string()), device)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_agents() {
        let mac_chrome = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36";
        assert_eq!(describe_user_agent(mac_chrome), (Some("Kinora web".into()), Some("Chrome on macOS".into())));
        let iphone = "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1";
        assert_eq!(describe_user_agent(iphone).1.as_deref(), Some("Safari on iPhone"));
        let win_ff = "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:132.0) Gecko/20100101 Firefox/132.0";
        assert_eq!(describe_user_agent(win_ff).1.as_deref(), Some("Firefox on Windows"));
        assert_eq!(describe_user_agent("okhttp/4.12.0").0.as_deref(), Some("Kinora Android"));
        assert_eq!(describe_user_agent(""), (None, None));
    }

    #[test]
    fn client_info_prefers_what_the_client_said() {
        let given = ClientInfo { name: Some("Kinora TV".into()), device: Some("  ".into()), address: None };
        let c = ClientInfo::resolve(Some(given), Some("okhttp/4"), Some("10.0.0.2".into()));
        assert_eq!(c.name.as_deref(), Some("Kinora TV"));
        assert_eq!(c.device, None);
        assert_eq!(c.address.as_deref(), Some("10.0.0.2"));
    }

    #[test]
    fn stats_are_trimmed() {
        let s = ClientStats {
            player: Some("x".repeat(500)),
            recent_errors: (0..20).map(|i| i.to_string()).collect(),
            ..Default::default()
        }
        .trimmed();
        assert_eq!(s.player.unwrap().chars().count(), MAX_TEXT + 1);
        assert_eq!(s.recent_errors, (12..20).map(|i| i.to_string()).collect::<Vec<_>>());
    }
}
