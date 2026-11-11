//! Playback routes: decide how to play, direct streams, HLS, subtitles.

use axum::body::Body;
use std::net::SocketAddr;

use axum::extract::{ConnectInfo, Path, Request, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Extension, Json, Router};
use kinora_core::ffmpeg::Tonemapper;
use kinora_core::hls::{parse_segment_name, INIT_NAME};
use kinora_core::media::{HdrFormat, MediaInfo};
use kinora_core::playback::{decide, ClientCaps, Limits, Method, PlaybackPlan, SegmentFormat};
use kinora_core::roles::Role;
use kinora_plugin_api::ServerEvent;
use serde::{Deserialize, Serialize};
use tower::ServiceExt;
use tower_http::services::ServeFile;

use crate::auth::AuthUser;
use crate::error::{AppError, AppResult};
use crate::now_playing::{ClientInfo, Heartbeat, ItemSummary, PlayInput, StreamInfo};
use crate::state::AppState;
use crate::transcode::{SegmentError, SessionInput};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/playback/{file_id}", post(start))
        .route("/api/stream/{file_id}", get(direct))
        .route("/api/hls/{session}/main.m3u8", get(playlist))
        .route("/api/hls/{session}/keepalive", post(keepalive))
        .route("/api/hls/{session}/{segment}", get(segment))
        .route("/api/hls/{session}", delete(stop))
        .route("/api/subtitles/{file_id}/{index}", get(subtitle))
        .route("/api/sessions", get(active_sessions))
        .route("/api/now-playing/{id}/heartbeat", post(heartbeat))
        .route("/api/now-playing/{id}", delete(end_playback))
}

struct FileRow {
    item_id: String,
    path: String,
    info: MediaInfo,
}

async fn load_file(state: &AppState, file_id: &str) -> AppResult<FileRow> {
    let (item_id, path, info): (String, String, Option<String>) =
        sqlx::query_as("SELECT item_id, path, info FROM media_files WHERE id = ?")
            .bind(file_id)
            .fetch_one(&state.db)
            .await?;
    let mut info: MediaInfo = info
        .and_then(|i| serde_json::from_str(&i).ok())
        .ok_or_else(|| AppError::BadRequest("file has not been scanned yet".into()))?;
    // ffprobe reports WebM as "matroska"; the extension tells them apart.
    if path.to_lowercase().ends_with(".webm") {
        info.container = "webm".into();
    }
    Ok(FileRow { item_id, path, info })
}

#[derive(Deserialize)]
struct StartRequest {
    caps: ClientCaps,
    #[serde(default)]
    audio_index: Option<u32>,
    #[serde(default)]
    max_bitrate: Option<u64>,
    /// Force a transcode even if direct play would work (for testing / bad files).
    #[serde(default)]
    force_transcode: bool,
    /// Who is playing, for the dashboard. Filled in from the User-Agent when missing.
    #[serde(default)]
    client: Option<ClientInfo>,
    /// Where playback starts, so the dashboard has a position before the first heartbeat.
    #[serde(default)]
    start_ms: Option<u64>,
}

#[derive(Serialize)]
struct SubtitleTrack {
    index: u32,
    label: String,
    language: Option<String>,
    url: String,
    codec: String,
    is_default: bool,
    is_forced: bool,
    is_hearing_impaired: bool,
    /// Text subtitles can be shown in the browser. Image subtitles (PGS, VobSub)
    /// need to be burned into the video, which isn't built yet, so they're listed but disabled.
    supported: bool,
}

#[derive(Serialize)]
struct StartResponse {
    method: Method,
    /// Relative URL (works behind reverse proxies with a sub-path).
    url: String,
    session_id: Option<String>,
    /// This playback on the dashboard: send heartbeats to `api/now-playing/{play_id}/heartbeat`.
    /// The same as `session_id` for HLS.
    play_id: String,
    plan: PlaybackPlan,
    duration_ms: u64,
    subtitles: Vec<SubtitleTrack>,
    hardware: String,
    /// Tracks picked from what this viewer chose on other episodes (module
    /// "auto-tracks"). Missing when there's nothing to go on.
    #[serde(skip_serializing_if = "Option::is_none")]
    auto_tracks: Option<crate::modules::auto_tracks::AutoTracks>,
    /// Seconds of video to buffer ahead (admin setting "stream_buffer_seconds").
    buffer_seconds: u32,
}

async fn start(
    State(state): State<AppState>,
    user: AuthUser,
    Path(file_id): Path<String>,
    headers: HeaderMap,
    connect: Option<Extension<ConnectInfo<SocketAddr>>>,
    Json(req): Json<StartRequest>,
) -> AppResult<Json<StartResponse>> {
    let file = load_file(&state, &file_id).await?;
    // The viewer's remembered tracks for this show, unless the player asked for a specific audio track.
    let auto_tracks = if req.audio_index.is_none() {
        crate::modules::auto_tracks::pick(&state.db, &user.id, &file.item_id, &file.info).await
    } else {
        None
    };
    let audio_index = req.audio_index.or(auto_tracks.as_ref().and_then(|a| a.audio_index));
    let hw = state.hardware.read().await.profile.clone();
    // CPU tone mapping can't keep up with 4K, so those transcodes go out at 1080p.
    let tonemap_max_height = (hw.tonemapper == Tonemapper::Zscale).then_some(1080);
    let limits = Limits { max_bitrate: req.max_bitrate, audio_index, tonemap_max_height, ..Default::default() };
    let mut caps = req.caps;
    if req.force_transcode {
        caps.containers.clear();
        caps.video_codecs.retain(|_| false);
    }
    let plan = decide(&file.info, &caps, &limits);

    let subtitles = file
        .info
        .subtitles
        .iter()
        .map(|s| SubtitleTrack {
            index: s.index,
            label: s.title.clone().or_else(|| s.language.clone()).unwrap_or_else(|| format!("Track {}", s.index)),
            language: s.language.clone(),
            url: format!("api/subtitles/{file_id}/{}", s.index),
            codec: s.codec.clone(),
            is_default: s.is_default,
            is_forced: s.is_forced,
            is_hearing_impaired: kinora_core::tracks::is_hearing_impaired(s),
            supported: s.is_text,
        })
        .collect();

    let item = item_summary(&state, &file.item_id).await?;
    let title = item.title.clone();

    state.emit(ServerEvent::PlaybackStarted {
        user_id: user.id.clone(),
        item_id: file.item_id.clone(),
        method: format!("{:?}", plan.method),
    });

    let (url, session_id) = if plan.method == Method::DirectPlay {
        (format!("api/stream/{file_id}"), None)
    } else {
        let v = file.info.video.as_ref();
        // Copied video can only be cut at keyframes: read their times from
        // the file's index so the playlist matches what ffmpeg writes.
        let keyframes_ms = if plan.segments == SegmentFormat::Fmp4 && file.info.container == "matroska" {
            let path = file.path.clone();
            tokio::task::spawn_blocking(move || {
                std::fs::File::open(&path).ok().and_then(|mut f| kinora_core::mkv::video_keyframes_ms(&mut f))
            })
            .await
            .unwrap_or(None)
        } else {
            None
        };
        if plan.segments == SegmentFormat::Fmp4 && keyframes_ms.is_none() {
            tracing::info!(path = %file.path, "no keyframe index; copied video uses fixed-length segments");
        }
        let id = state
            .transcodes
            .create(SessionInput {
                user_id: user.id.clone(),
                username: user.username.clone(),
                item_id: file.item_id.clone(),
                title,
                file_path: file.path.clone(),
                duration_ms: file.info.duration_ms,
                video_index: v.map(|v| v.index),
                frame_rate: v.and_then(|v| v.frame_rate),
                source_is_dolby_vision: v.map(|v| v.hdr == HdrFormat::DolbyVision).unwrap_or(false),
                keyframes_ms,
                plan: plan.clone(),
                hw: hw.clone(),
            })
            .await?;
        (format!("api/hls/{id}/main.m3u8"), Some(id))
    };

    let play_id = session_id.clone().unwrap_or_else(crate::db::new_id);
    let user_agent = headers.get(header::USER_AGENT).and_then(|v| v.to_str().ok());
    let address = client_address(&headers, connect.map(|Extension(ConnectInfo(a))| a));
    state.now_playing.insert(
        play_id.clone(),
        PlayInput {
            user_id: user.id.clone(),
            username: user.username.clone(),
            file_id: file_id.clone(),
            item,
            method: plan.method,
            plan: plan.clone(),
            hardware: hw.accel.label().to_string(),
            client: ClientInfo::resolve(req.client, user_agent, address),
            media: file.info.clone(),
            url: url.clone(),
        },
        req.start_ms,
    );

    Ok(Json(StartResponse {
        method: plan.method,
        url,
        session_id,
        play_id,
        duration_ms: file.info.duration_ms,
        plan,
        subtitles,
        hardware: hw.accel.label().to_string(),
        auto_tracks,
        buffer_seconds: crate::system::stream_buffer_seconds(&state).await?,
    }))
}

/// Serve the original file with HTTP range support (seeking).
async fn direct(
    State(state): State<AppState>,
    user: AuthUser,
    Path(file_id): Path<String>,
    request: Request,
) -> AppResult<Response> {
    state.now_playing.touch_file(&user.id, &file_id);
    let path: String =
        sqlx::query_scalar("SELECT path FROM media_files WHERE id = ?").bind(&file_id).fetch_one(&state.db).await?;
    let resp = ServeFile::new(&path).oneshot(request).await.map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
    Ok(resp.map(Body::new).into_response())
}

async fn check_session(state: &AppState, user: &AuthUser, session: &str) -> AppResult<()> {
    match state.transcodes.owner(session).await {
        Some(owner) if owner == user.id || user.role.at_least(Role::Admin) => Ok(()),
        Some(_) => Err(AppError::Forbidden),
        None => Err(AppError::NotFound),
    }
}

async fn playlist(State(state): State<AppState>, user: AuthUser, Path(session): Path<String>) -> AppResult<Response> {
    check_session(&state, &user, &session).await?;
    let playlist = state.transcodes.playlist(&session).await.ok_or(AppError::NotFound)?;
    Ok(([(header::CONTENT_TYPE, "application/vnd.apple.mpegurl"), (header::CACHE_CONTROL, "no-cache")], playlist)
        .into_response())
}

async fn segment(
    State(state): State<AppState>,
    user: AuthUser,
    Path((session, segment)): Path<(String, String)>,
) -> AppResult<Response> {
    check_session(&state, &user, &session).await?;
    state.now_playing.touch(&session);
    let result = if segment == INIT_NAME {
        state.transcodes.init_segment(&session).await
    } else {
        let index = parse_segment_name(&segment).ok_or(AppError::NotFound)?;
        state.transcodes.segment(&session, index).await
    };
    // Only a missing session or segment is a 404. A transcoder failure is a
    // 500, so the player reports a server problem instead of a missing file.
    let path = result.map_err(|e| {
        tracing::warn!(session, segment, error = %e, "segment failed");
        match e {
            SegmentError::NoSession | SegmentError::OutOfRange => AppError::NotFound,
            SegmentError::Failed(e) => AppError::Internal(e),
        }
    })?;
    let content_type = if segment.ends_with(".ts") { "video/mp2t" } else { "video/mp4" };
    let bytes = tokio::fs::read(&path).await?;
    Ok(([(header::CONTENT_TYPE, content_type)], bytes).into_response())
}

/// The player calls this every so often (also while paused) so the session
/// isn't cleaned up under it. 404 means it already was: start a new one.
async fn keepalive(
    State(state): State<AppState>,
    user: AuthUser,
    Path(session): Path<String>,
) -> AppResult<Json<crate::transcode::SessionStatus>> {
    check_session(&state, &user, &session).await?;
    state.now_playing.touch(&session);
    state.transcodes.keepalive(&session).await.map(Json).ok_or(AppError::NotFound)
}

#[derive(Deserialize, Default)]
struct StopBody {
    #[serde(default)]
    position_ms: Option<u64>,
    #[serde(default)]
    item_id: Option<String>,
}

async fn stop(
    State(state): State<AppState>,
    user: AuthUser,
    Path(session): Path<String>,
    body: Option<Json<StopBody>>,
) -> AppResult<StatusCode> {
    check_session(&state, &user, &session).await?;
    state.transcodes.stop(&session).await;
    state.now_playing.remove(&session);
    if let Some(Json(b)) = body {
        if let (Some(item_id), Some(position_ms)) = (b.item_id, b.position_ms) {
            state.emit(ServerEvent::PlaybackStopped { user_id: user.id, item_id, position_ms });
        }
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Convert an embedded text subtitle to WebVTT for the browser.
async fn subtitle(
    State(state): State<AppState>,
    _user: AuthUser,
    Path((file_id, index)): Path<(String, String)>,
) -> AppResult<Response> {
    let index: u32 = index.trim_end_matches(".vtt").parse().map_err(|_| AppError::NotFound)?;
    let file = load_file(&state, &file_id).await?;
    if !file.info.subtitles.iter().any(|s| s.index == index && s.is_text) {
        return Err(AppError::NotFound);
    }
    let args = kinora_core::ffmpeg::subtitle_vtt_args(&file.path, index);
    let out = tokio::process::Command::new(&state.config.ffmpeg).args(&args).kill_on_drop(true).output().await?;
    if !out.status.success() {
        return Err(AppError::Internal(anyhow::anyhow!(
            "subtitle extraction failed: {}",
            String::from_utf8_lossy(&out.stderr)
        )));
    }
    Ok(([(header::CONTENT_TYPE, "text/vtt; charset=utf-8")], out.stdout).into_response())
}

/// Everything playing right now, every method (admin dashboard).
async fn active_sessions(State(state): State<AppState>, user: AuthUser) -> AppResult<Json<Vec<StreamInfo>>> {
    user.require(Role::Admin)?;
    let statuses = state.transcodes.all_status().await;
    Ok(Json(state.now_playing.list(|id| statuses.get(id).cloned())))
}

fn check_play(state: &AppState, user: &AuthUser, id: &str) -> AppResult<()> {
    match state.now_playing.owner(id) {
        Some(owner) if owner == user.id || user.role.at_least(Role::Admin) => Ok(()),
        Some(_) => Err(AppError::Forbidden),
        None => Err(AppError::NotFound),
    }
}

/// The player is still open: where it is, playing or paused, and a few stats.
/// 404 means the playback was forgotten (start a new one to show up again).
async fn heartbeat(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    Json(hb): Json<Heartbeat>,
) -> AppResult<StatusCode> {
    check_play(&state, &user, &id)?;
    state.now_playing.touch(&id);
    if state.now_playing.heartbeat(&id, hb) {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::NotFound)
    }
}

/// The player closed (direct play; HLS players stop their session instead).
async fn end_playback(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    body: Option<Json<StopBody>>,
) -> AppResult<StatusCode> {
    check_play(&state, &user, &id)?;
    let Some(removed) = state.now_playing.remove(&id) else { return Ok(StatusCode::NO_CONTENT) };
    // An HLS session ends with its transcode (which also lets go of the file's page cache).
    state.transcodes.stop(&id).await;
    if !state.now_playing.is_playing_file(&removed.file_id) {
        let path: Option<String> = sqlx::query_scalar("SELECT path FROM media_files WHERE id = ?")
            .bind(&removed.file_id)
            .fetch_optional(&state.db)
            .await?;
        if let Some(path) = path {
            if !state.transcodes.is_playing(&path).await {
                crate::page_cache::release(path);
            }
        }
    }
    let position_ms = body.and_then(|Json(b)| b.position_ms).or(removed.position_ms).unwrap_or(0);
    state.emit(ServerEvent::PlaybackStopped { user_id: user.id, item_id: removed.item_id, position_ms });
    Ok(StatusCode::NO_CONTENT)
}

async fn item_summary(state: &AppState, item_id: &str) -> AppResult<ItemSummary> {
    let (kind, title, series_title, season, episode, year): (
        String,
        String,
        Option<String>,
        Option<i64>,
        Option<i64>,
        Option<i64>,
    ) = sqlx::query_as(
        "SELECT i.kind, i.title, s.title, i.season_number, i.episode_number, COALESCE(i.year, s.year)
         FROM items i LEFT JOIN items s ON s.id = i.series_id WHERE i.id = ?",
    )
    .bind(item_id)
    .fetch_one(&state.db)
    .await?;
    Ok(ItemSummary { id: item_id.to_string(), kind, title, series_title, season, episode, year })
}

/// The viewer's address: the first hop a reverse proxy reports, else the connection's.
fn client_address(headers: &HeaderMap, peer: Option<SocketAddr>) -> Option<String> {
    let forwarded = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .or_else(|| headers.get("x-real-ip").and_then(|v| v.to_str().ok()))
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty() && v.len() <= 64);
    forwarded.or_else(|| peer.map(|p| p.ip().to_string()))
}
