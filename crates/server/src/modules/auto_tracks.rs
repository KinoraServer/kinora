//! Auto audio & subtitles (module "auto-tracks").
//!
//! When a viewer changes the audio or subtitle track, the player sends the
//! new pair to `PUT /api/playback/{file_id}/tracks`. We save a description of
//! it (see `kinora_core::tracks`) for that viewer, keyed by the season, the
//! show and the library. When playback starts, `pick` looks the choice up
//! (season if the "per season" setting is on, then show, then library if the
//! "library fallback" setting is on) and finds the closest tracks in the new
//! file. Because this runs on the server, every client gets the same picks.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use kinora_core::media::MediaInfo;
use kinora_core::tracks::{self, AudioRef, SubtitleRef, TrackChoice, TrackPick};
use kinora_plugin_api::{ExtensionPoint, ModuleInfo, SettingField, SettingKind};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::auth::AuthUser;
use crate::db::now_ms;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

pub const ID: &str = "auto-tracks";
const PER_SEASON: &str = "per_season";
const LIBRARY_FALLBACK: &str = "library_fallback";

pub fn info() -> ModuleInfo {
    ModuleInfo {
        id: ID.into(),
        name: "Auto audio & subtitles".into(),
        description: "Remembers the audio and subtitle tracks each viewer picks for a show and picks the closest \
                      match on every other episode, on every device."
            .into(),
        version: env!("CARGO_PKG_VERSION").into(),
        provides: vec![ExtensionPoint::TrackSelection],
        enabled_by_default: true,
        settings: vec![
            SettingField {
                key: LIBRARY_FALLBACK.into(),
                label: "Use the library's last choice for new shows".into(),
                kind: SettingKind::Bool,
                help: Some(
                    "A show (or movie) you haven't picked tracks for yet starts with what you last picked in the same \
                     library, e.g. Japanese audio with English subtitles in an anime library. On by default."
                        .into(),
                ),
                default: Some("true".into()),
            },
            SettingField {
                key: PER_SEASON.into(),
                label: "Remember per season".into(),
                kind: SettingKind::Bool,
                help: Some(
                    "Keep a separate choice for each season instead of one per show. A season without its own \
                     choice still uses the show's. Off by default."
                        .into(),
                ),
                default: Some("false".into()),
            },
        ],
    }
}

/// Where a remembered choice was found, from most to least specific.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    Season,
    Series,
    Library,
}

impl Scope {
    fn as_str(self) -> &'static str {
        match self {
            Scope::Season => "season",
            Scope::Series => "series",
            Scope::Library => "library",
        }
    }
}

/// Sent to the client with the playback start response.
#[derive(Debug, Serialize)]
pub struct AutoTracks {
    /// Which remembered choice was used.
    pub from: Scope,
    pub audio_index: Option<u32>,
    /// Missing: no opinion, the client picks. `null`: subtitles off.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtitle_index: Option<Option<u32>>,
}

/// The season, show and library an item belongs to, most specific first.
async fn scopes(db: &SqlitePool, item_id: &str) -> sqlx::Result<Vec<(Scope, String)>> {
    let (kind, library_id, parent_id, series_id): (String, String, Option<String>, Option<String>) =
        sqlx::query_as("SELECT kind, library_id, parent_id, series_id FROM items WHERE id = ?")
            .bind(item_id)
            .fetch_one(db)
            .await?;
    let mut out = Vec::new();
    if kind == "episode" {
        if let Some(season) = parent_id.filter(|p| Some(p) != series_id.as_ref()) {
            out.push((Scope::Season, season));
        }
        if let Some(series) = series_id {
            out.push((Scope::Series, series));
        }
    }
    out.push((Scope::Library, library_id));
    Ok(out)
}

/// The tracks to start `item_id` with for this viewer, if they picked any before.
pub async fn pick(db: &SqlitePool, user_id: &str, item_id: &str, info: &MediaInfo) -> Option<AutoTracks> {
    if !super::is_enabled(db, ID).await {
        return None;
    }
    let settings = super::settings(db, ID).await;
    let per_season = super::flag(&settings, PER_SEASON, false);
    let library_fallback = super::flag(&settings, LIBRARY_FALLBACK, true);

    let scopes = match scopes(db, item_id).await {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(item_id, error = %e, "auto tracks: item lookup failed");
            return None;
        }
    };
    for (scope, scope_id) in scopes {
        if (scope == Scope::Season && !per_season) || (scope == Scope::Library && !library_fallback) {
            continue;
        }
        let saved: Option<String> =
            sqlx::query_scalar("SELECT choice FROM track_choices WHERE user_id = ? AND scope = ? AND scope_id = ?")
                .bind(user_id)
                .bind(scope.as_str())
                .bind(&scope_id)
                .fetch_optional(db)
                .await
                .ok()
                .flatten();
        let Some(choice) = saved.and_then(|c| serde_json::from_str::<TrackChoice>(&c).ok()) else { continue };
        let TrackPick { audio_index, subtitle_index } = tracks::apply(&choice, &info.audio, &info.subtitles);
        if audio_index.is_none() && subtitle_index.is_none() {
            // Nothing in this file matches (e.g. a show without the library's usual language). Keep looking.
            continue;
        }
        return Some(AutoTracks { from: scope, audio_index, subtitle_index });
    }
    None
}

#[derive(Deserialize)]
pub struct Remember {
    /// The audio stream playing now (ffprobe index).
    audio_index: Option<u32>,
    /// The subtitle stream showing now, or null for off.
    subtitle_index: Option<u32>,
}

/// The player calls this when the viewer changes the audio or subtitle track.
/// 204 either way; nothing is saved while the module is off.
pub async fn remember(
    State(state): State<AppState>,
    user: AuthUser,
    Path(file_id): Path<String>,
    Json(body): Json<Remember>,
) -> AppResult<StatusCode> {
    if !super::is_enabled(&state.db, ID).await {
        return Ok(StatusCode::NO_CONTENT);
    }
    let (item_id, info): (String, Option<String>) =
        sqlx::query_as("SELECT item_id, info FROM media_files WHERE id = ?")
            .bind(&file_id)
            .fetch_one(&state.db)
            .await?;
    let info: MediaInfo = info
        .and_then(|i| serde_json::from_str(&i).ok())
        .ok_or_else(|| AppError::BadRequest("file has not been scanned yet".into()))?;

    let audio = match body.audio_index {
        Some(i) => Some(AudioRef::from(
            info.audio
                .iter()
                .find(|a| a.index == i)
                .ok_or_else(|| AppError::BadRequest("unknown audio track".into()))?,
        )),
        None => None,
    };
    let subtitle = match body.subtitle_index {
        Some(i) => SubtitleRef::from(Some(
            info.subtitles
                .iter()
                .find(|s| s.index == i)
                .ok_or_else(|| AppError::BadRequest("unknown subtitle track".into()))?,
        )),
        None => SubtitleRef::Off,
    };
    let choice = serde_json::to_string(&TrackChoice { audio, subtitle: Some(subtitle) })
        .map_err(|e| AppError::Internal(e.into()))?;

    let now = now_ms();
    for (scope, scope_id) in scopes(&state.db, &item_id).await? {
        sqlx::query(
            "INSERT INTO track_choices (user_id, scope, scope_id, choice, item_id, updated_at) VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT(user_id, scope, scope_id) DO UPDATE SET
                choice = excluded.choice, item_id = excluded.item_id, updated_at = excluded.updated_at",
        )
        .bind(&user.id)
        .bind(scope.as_str())
        .bind(&scope_id)
        .bind(&choice)
        .bind(&item_id)
        .bind(now)
        .execute(&state.db)
        .await?;
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Forget every track choice the signed-in viewer made.
pub async fn forget_all(State(state): State<AppState>, user: AuthUser) -> AppResult<StatusCode> {
    sqlx::query("DELETE FROM track_choices WHERE user_id = ?").bind(&user.id).execute(&state.db).await?;
    Ok(StatusCode::NO_CONTENT)
}
