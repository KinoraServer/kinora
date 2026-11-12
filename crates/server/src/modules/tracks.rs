//! Audio and subtitle selection (extension point `track_selection`).
//!
//! The server only does the plumbing: when a viewer changes the audio or
//! subtitle track, the player sends the new pair to
//! `PUT /api/playback/{file_id}/tracks` and the installed module is told
//! (`tracks.remember`). When playback starts the module is asked which tracks
//! to start with (`tracks.pick`). What is remembered and how tracks are
//! matched is up to the module; without one, players use the file's defaults.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use kinora_core::media::MediaInfo;
use kinora_plugin_api::ExtensionPoint;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::json;
use sqlx::SqlitePool;

use crate::auth::AuthUser;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// Where a remembered choice was found, from most to least specific.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    Season,
    Series,
    Library,
}

/// `null` and "not there" mean different things for subtitles.
fn present<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Option<u32>>, D::Error> {
    Option::<u32>::deserialize(d).map(Some)
}

/// Sent to the client with the playback start response.
#[derive(Debug, Serialize, Deserialize)]
pub struct AutoTracks {
    /// Which remembered choice was used.
    pub from: Scope,
    #[serde(default)]
    pub audio_index: Option<u32>,
    /// Missing: no opinion, the client picks. `null`: subtitles off.
    #[serde(default, deserialize_with = "present", skip_serializing_if = "Option::is_none")]
    pub subtitle_index: Option<Option<u32>>,
}

/// The season, show and library an item belongs to, most specific first.
async fn scopes(db: &SqlitePool, item_id: &str) -> sqlx::Result<Vec<serde_json::Value>> {
    let (kind, library_id, parent_id, series_id): (String, String, Option<String>, Option<String>) =
        sqlx::query_as("SELECT kind, library_id, parent_id, series_id FROM items WHERE id = ?")
            .bind(item_id)
            .fetch_one(db)
            .await?;
    let mut out = Vec::new();
    if kind == "episode" {
        if let Some(season) = parent_id.filter(|p| Some(p) != series_id.as_ref()) {
            out.push(json!({ "scope": "season", "id": season }));
        }
        if let Some(series) = series_id {
            out.push(json!({ "scope": "series", "id": series }));
        }
    }
    out.push(json!({ "scope": "library", "id": library_id }));
    Ok(out)
}

/// The tracks to start `item_id` with for this viewer, if a module has an opinion.
pub async fn pick(state: &AppState, user_id: &str, item_id: &str, info: &MediaInfo) -> Option<AutoTracks> {
    let module = state.modules.providers(state, ExtensionPoint::TrackSelection).await.into_iter().next()?;
    let scopes = match scopes(&state.db, item_id).await {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(item_id, error = %e, "track selection: item lookup failed");
            return None;
        }
    };
    let args = json!({ "user_id": user_id, "scopes": scopes, "audio": info.audio, "subtitles": info.subtitles });
    let picked = match state.modules.call(state, &module, "tracks.pick", args).await {
        Ok(v) if v.is_null() => return None,
        Ok(v) => serde_json::from_value::<AutoTracks>(v).map_err(|e| anyhow::anyhow!("module {}: {e}", module.id)),
        Err(e) => Err(e),
    };
    let mut picked = match picked {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(error = %e, "track selection failed; using the file's default tracks");
            return None;
        }
    };
    // Only tracks this file really has.
    picked.audio_index = picked.audio_index.filter(|i| info.audio.iter().any(|a| a.index == *i));
    if let Some(Some(i)) = picked.subtitle_index {
        if !info.subtitles.iter().any(|s| s.index == i) {
            picked.subtitle_index = None;
        }
    }
    (picked.audio_index.is_some() || picked.subtitle_index.is_some()).then_some(picked)
}

#[derive(Deserialize)]
pub struct Remember {
    /// The audio stream playing now (ffprobe index).
    audio_index: Option<u32>,
    /// The subtitle stream showing now, or null for off.
    subtitle_index: Option<u32>,
}

/// The player calls this when the viewer changes the audio or subtitle track.
/// 204 either way; nothing is saved without a track selection module.
pub async fn remember(
    State(state): State<AppState>,
    user: AuthUser,
    Path(file_id): Path<String>,
    Json(body): Json<Remember>,
) -> AppResult<StatusCode> {
    let Some(module) = state.modules.providers(&state, ExtensionPoint::TrackSelection).await.into_iter().next() else {
        return Ok(StatusCode::NO_CONTENT);
    };
    let (item_id, info): (String, Option<String>) =
        sqlx::query_as("SELECT item_id, info FROM media_files WHERE id = ?")
            .bind(&file_id)
            .fetch_one(&state.db)
            .await?;
    let info: MediaInfo = info
        .and_then(|i| serde_json::from_str(&i).ok())
        .ok_or_else(|| AppError::BadRequest("file has not been scanned yet".into()))?;
    if body.audio_index.is_some_and(|i| !info.audio.iter().any(|a| a.index == i)) {
        return Err(AppError::BadRequest("unknown audio track".into()));
    }
    if body.subtitle_index.is_some_and(|i| !info.subtitles.iter().any(|s| s.index == i)) {
        return Err(AppError::BadRequest("unknown subtitle track".into()));
    }
    let args = json!({
        "user_id": user.id,
        "item_id": item_id,
        "scopes": scopes(&state.db, &item_id).await?,
        "audio": info.audio,
        "subtitles": info.subtitles,
        "audio_index": body.audio_index,
        "subtitle_index": body.subtitle_index,
    });
    state.modules.call(&state, &module, "tracks.remember", args).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Forget every track choice the signed-in viewer made.
pub async fn forget_all(State(state): State<AppState>, user: AuthUser) -> AppResult<StatusCode> {
    for module in state.modules.providers(&state, ExtensionPoint::TrackSelection).await {
        state.modules.call(&state, &module, "tracks.forget", json!({ "user_id": user.id })).await?;
    }
    Ok(StatusCode::NO_CONTENT)
}
