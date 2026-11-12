//! Modules: every optional feature can be switched off in the admin UI.
//!
//! Three engines are part of the server (themes, player skins, app skins):
//! they render in the web app and take plain files. Everything else is
//! installed from a registry at runtime (`registry`) and runs in a sandbox
//! (`wasm`); the files next to this one are the server's side of each
//! extension point. See docs/modules.md.

mod app_skin_samples;
pub mod app_skins;
pub mod events;
pub mod metadata;
pub mod registry;
pub mod skins;
#[cfg(test)]
pub mod test_support;
pub mod themes;
pub mod tracks;
pub mod wasm;

use axum::extract::{Path, State};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use kinora_plugin_api::{ModuleInfo, SettingKind, Settings};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::auth::AdminUser;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// The engines that are part of the server.
pub fn builtin() -> Vec<ModuleInfo> {
    vec![themes::info(), skins::info(), app_skins::info()]
}

/// How an installed module looks in the module list.
fn installed_info(m: &wasm::InstalledModule) -> ModuleInfo {
    ModuleInfo {
        id: m.id.clone(),
        name: m.name.clone(),
        description: m.description.clone(),
        version: m.version.clone(),
        provides: m.spec.provides.clone(),
        enabled_by_default: true,
        settings: m.spec.settings.clone(),
    }
}

/// A built-in engine or an installed module.
async fn find(state: &AppState, id: &str) -> Option<(ModuleInfo, Source)> {
    if let Some(m) = builtin().into_iter().find(|m| m.id == id) {
        return Some((m, Source::Builtin));
    }
    state.modules.find(state, id).await.map(|m| (installed_info(&m), Source::Registry))
}

/// Is a module switched on? An installed module is on until it is switched off.
pub async fn is_enabled(db: &SqlitePool, id: &str) -> bool {
    let default = builtin().into_iter().find(|m| m.id == id).map(|m| m.enabled_by_default).unwrap_or(true);
    sqlx::query_scalar::<_, i64>("SELECT enabled FROM modules WHERE id = ?")
        .bind(id)
        .fetch_optional(db)
        .await
        .ok()
        .flatten()
        .map(|e| e != 0)
        .unwrap_or(default)
}

/// A bool module setting ("true" / "false"); `default` when it was never saved.
pub fn flag(settings: &Settings, key: &str, default: bool) -> bool {
    settings.get(key).map(|v| v == "true").unwrap_or(default)
}

pub async fn settings(db: &SqlitePool, id: &str) -> Settings {
    sqlx::query_scalar::<_, String>("SELECT settings FROM modules WHERE id = ?")
        .bind(id)
        .fetch_optional(db)
        .await
        .ok()
        .flatten()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

async fn save(db: &SqlitePool, id: &str, enabled: bool, settings: &Settings) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO modules (id, enabled, settings) VALUES (?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET enabled = excluded.enabled, settings = excluded.settings",
    )
    .bind(id)
    .bind(enabled as i64)
    .bind(serde_json::to_string(settings).unwrap_or_else(|_| "{}".into()))
    .execute(db)
    .await?;
    Ok(())
}

/// First start: copy TMDB_API_KEY from the environment into the TMDB module's
/// settings, where it waits until the module is installed.
pub async fn seed_from_env(db: &SqlitePool, tmdb_key: Option<&str>) -> sqlx::Result<()> {
    if let Some(key) = tmdb_key {
        let mut s = settings(db, "tmdb").await;
        if s.get("api_key").map(|k| k.is_empty()).unwrap_or(true) {
            s.insert("api_key".into(), key.to_string());
            let enabled = is_enabled(db, "tmdb").await;
            save(db, "tmdb", enabled, &s).await?;
        }
    }
    Ok(())
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/modules", get(list))
        .route("/api/modules/{id}", put(update))
        .route("/api/themes", get(themes::list))
        .route("/api/player-skins", get(skins::list))
        .route("/api/player/style", get(skins::get_style).put(skins::put_style))
        .route("/api/playback/{file_id}/tracks", put(tracks::remember))
        .route("/api/track-choices", delete(tracks::forget_all))
        .merge(registry::routes())
        .route("/api/app-skins", get(app_skins::list))
        .route("/api/app-skins/errors", post(app_skins::report_error))
        .route("/api/app-skins/{id}", delete(app_skins::remove))
        .route("/api/app-skins/{id}/bundle", get(app_skins::preview_bundle))
        .route("/api/app-skins/{id}/options", put(app_skins::put_options))
        .route("/api/app-skins/starters/{name}", post(app_skins::add_starter))
        .route("/api/app-skins/{id}/preview", get(app_skins::preview_image))
        .route("/api/app-skins/{id}/assets/{file}", get(app_skins::asset))
        .route(
            "/api/app-skins/upload/{token}/{*path}",
            put(app_skins::upload_file).layer(axum::extract::DefaultBodyLimit::max(4 * 1024 * 1024)),
        )
        .route("/api/app-skins/finish/{token}", post(app_skins::upload_finish))
        .route("/api/app-skin", put(app_skins::put_active))
        .route("/api/app-skin/active", get(app_skins::active))
        .route("/api/me/app-skin", get(app_skins::get_mine).put(app_skins::put_mine))
}

/// Where a module comes from.
#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum Source {
    /// Part of the server.
    Builtin,
    /// Installed from a registry; can be uninstalled.
    Registry,
}

#[derive(Serialize)]
struct ModuleView {
    #[serde(flatten)]
    info: ModuleInfo,
    source: Source,
    enabled: bool,
    /// Current values; secrets are replaced by `secrets_set`.
    values: Settings,
    secrets_set: Vec<String>,
}

async fn view(db: &SqlitePool, info: ModuleInfo, source: Source) -> ModuleView {
    let enabled = is_enabled(db, &info.id).await;
    let mut values = settings(db, &info.id).await;
    let mut secrets_set = Vec::new();
    for field in &info.settings {
        if field.kind == SettingKind::Secret {
            if let Some(v) = values.remove(&field.key) {
                if !v.is_empty() {
                    secrets_set.push(field.key.clone());
                }
            }
        }
    }
    ModuleView { info, source, enabled, values, secrets_set }
}

async fn list(State(state): State<AppState>, _admin: AdminUser) -> Json<Vec<ModuleView>> {
    let mut out = Vec::new();
    for m in builtin() {
        out.push(view(&state.db, m, Source::Builtin).await);
    }
    for m in state.modules.installed(&state).await.iter() {
        out.push(view(&state.db, installed_info(m), Source::Registry).await);
    }
    Json(out)
}

#[derive(Deserialize)]
struct ModuleUpdate {
    enabled: Option<bool>,
    /// Only the keys present are changed. An empty string clears a value.
    #[serde(default)]
    settings: Settings,
}

async fn update(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(id): Path<String>,
    Json(body): Json<ModuleUpdate>,
) -> AppResult<Json<ModuleView>> {
    let (info, source) = find(&state, &id).await.ok_or(AppError::NotFound)?;
    let mut current = settings(&state.db, &id).await;
    for (k, v) in body.settings {
        let Some(field) = info.settings.iter().find(|f| f.key == k) else {
            return Err(AppError::BadRequest(format!("unknown setting {k}")));
        };
        if field.kind == SettingKind::Bool && !matches!(v.as_str(), "" | "true" | "false") {
            return Err(AppError::BadRequest(format!("{k} must be true or false")));
        }
        if v.is_empty() {
            current.remove(&k);
        } else {
            current.insert(k, v);
        }
    }
    let enabled = body.enabled.unwrap_or(is_enabled(&state.db, &id).await);
    save(&state.db, &id, enabled, &current).await?;
    Ok(Json(view(&state.db, info, source).await))
}
