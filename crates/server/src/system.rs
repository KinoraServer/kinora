//! Server info, general settings, hardware report, activity.

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::auth::{user_count, AdminUser};
use crate::db::{get_setting, set_setting};
use crate::error::{AppError, AppResult};
use crate::hw::{self, HardwareReport};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/system/info", get(info))
        .route("/api/system/settings", get(get_settings).put(put_settings))
        .route("/api/system/hardware", get(hardware))
        .route("/api/system/hardware/detect", post(redetect))
        .route("/api/system/activity", get(activity))
}

#[derive(Serialize)]
struct Info {
    name: &'static str,
    version: &'static str,
    server_name: String,
    setup_required: bool,
    /// App skin contract versions (docs/app-skins.md).
    skin_api: SkinApi,
}

#[derive(Serialize)]
struct SkinApi {
    current: u32,
    supported: Vec<u32>,
}

/// Public: the web app calls this first to decide between setup, login and home.
async fn info(State(state): State<AppState>) -> AppResult<Json<Info>> {
    Ok(Json(Info {
        name: "Kinora",
        version: env!("CARGO_PKG_VERSION"),
        server_name: get_setting(&state.db, "server_name").await?.unwrap_or_else(|| "Kinora".into()),
        setup_required: user_count(&state).await? == 0,
        skin_api: {
            let r = kinora_plugin_api::app_skin::rules();
            SkinApi { current: r.skin_api, supported: r.supported.clone() }
        },
    }))
}

#[derive(Serialize, Deserialize)]
struct GeneralSettings {
    server_name: String,
    default_theme: String,
    /// "auto", "none", "nvenc", "qsv", "vaapi", "videotoolbox".
    hw_accel: String,
    /// Minutes between automatic library scans; 0 disables.
    scan_interval_minutes: u32,
}

pub async fn load_general(state: &AppState) -> AppResult<(String, String, String, u32)> {
    Ok((
        get_setting(&state.db, "server_name").await?.unwrap_or_else(|| "Kinora".into()),
        get_setting(&state.db, "default_theme").await?.unwrap_or_else(|| "kinora".into()),
        get_setting(&state.db, "hw_accel").await?.unwrap_or_else(|| "auto".into()),
        get_setting(&state.db, "scan_interval_minutes").await?.and_then(|v| v.parse().ok()).unwrap_or(60),
    ))
}

async fn get_settings(State(state): State<AppState>, _admin: AdminUser) -> AppResult<Json<GeneralSettings>> {
    let (server_name, default_theme, hw_accel, scan_interval_minutes) = load_general(&state).await?;
    Ok(Json(GeneralSettings { server_name, default_theme, hw_accel, scan_interval_minutes }))
}

async fn put_settings(
    State(state): State<AppState>,
    _admin: AdminUser,
    Json(s): Json<GeneralSettings>,
) -> AppResult<Json<GeneralSettings>> {
    const ACCELS: &[&str] = &["auto", "none", "nvenc", "qsv", "vaapi", "videotoolbox"];
    if !ACCELS.contains(&s.hw_accel.as_str()) {
        return Err(AppError::BadRequest("unknown hardware acceleration option".into()));
    }
    if s.server_name.trim().is_empty() || s.server_name.len() > 80 {
        return Err(AppError::BadRequest("server name must be 1-80 characters".into()));
    }
    let old_accel = get_setting(&state.db, "hw_accel").await?.unwrap_or_else(|| "auto".into());
    set_setting(&state.db, "server_name", s.server_name.trim()).await?;
    set_setting(&state.db, "default_theme", &s.default_theme).await?;
    set_setting(&state.db, "hw_accel", &s.hw_accel).await?;
    set_setting(&state.db, "scan_interval_minutes", &s.scan_interval_minutes.to_string()).await?;
    if old_accel != s.hw_accel {
        spawn_detection(&state);
    }
    Ok(Json(s))
}

async fn hardware(State(state): State<AppState>, _admin: AdminUser) -> Json<HardwareReport> {
    Json(state.hardware.read().await.clone())
}

pub fn spawn_detection(state: &AppState) {
    let state = state.clone();
    tokio::spawn(async move {
        let pref = get_setting(&state.db, "hw_accel").await.ok().flatten().unwrap_or_else(|| "auto".into());
        let report = hw::detect(&state.config.ffmpeg, &pref).await;
        *state.hardware.write().await = report;
    });
}

async fn redetect(State(state): State<AppState>, _admin: AdminUser) -> Json<serde_json::Value> {
    spawn_detection(&state);
    Json(serde_json::json!({ "started": true }))
}

#[derive(Serialize, sqlx::FromRow)]
struct ActivityRow {
    id: i64,
    time: i64,
    username: Option<String>,
    event: String,
    payload: String,
}

async fn activity(State(state): State<AppState>, _admin: AdminUser) -> AppResult<Json<Vec<ActivityRow>>> {
    let rows = sqlx::query_as(
        "SELECT a.id, a.time, u.username, a.event, a.payload FROM activity_log a
         LEFT JOIN users u ON u.id = a.user_id ORDER BY a.id DESC LIMIT 100",
    )
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}
