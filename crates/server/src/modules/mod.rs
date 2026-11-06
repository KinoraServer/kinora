//! Module registry: every optional feature is a module that can be switched
//! off in the admin UI. Built-in modules are listed here; community plugins
//! will be registered the same way once the Wasm host exists (plan §4).

pub mod activity;
pub mod skins;
pub mod themes;
pub mod tmdb;

use axum::extract::{Path, State};
use axum::routing::{get, put};
use axum::{Json, Router};
use kinora_plugin_api::{ModuleInfo, SettingKind, Settings};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::auth::AdminUser;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// All built-in modules.
pub fn builtin() -> Vec<ModuleInfo> {
    vec![tmdb::info(), themes::info(), skins::info(), activity::info()]
}

pub fn find(id: &str) -> Option<ModuleInfo> {
    builtin().into_iter().find(|m| m.id == id)
}

pub async fn is_enabled(db: &SqlitePool, id: &str) -> bool {
    let default = find(id).map(|m| m.enabled_by_default).unwrap_or(false);
    sqlx::query_scalar::<_, i64>("SELECT enabled FROM modules WHERE id = ?")
        .bind(id)
        .fetch_optional(db)
        .await
        .ok()
        .flatten()
        .map(|e| e != 0)
        .unwrap_or(default)
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

/// First start: copy TMDB_API_KEY from the environment into the module settings.
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
}

#[derive(Serialize)]
struct ModuleView {
    #[serde(flatten)]
    info: ModuleInfo,
    enabled: bool,
    /// Current values; secrets are replaced by `secrets_set`.
    values: Settings,
    secrets_set: Vec<String>,
}

async fn view(db: &SqlitePool, info: ModuleInfo) -> ModuleView {
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
    ModuleView { info, enabled, values, secrets_set }
}

async fn list(State(state): State<AppState>, _admin: AdminUser) -> Json<Vec<ModuleView>> {
    let mut out = Vec::new();
    for m in builtin() {
        out.push(view(&state.db, m).await);
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
    let info = find(&id).ok_or(AppError::NotFound)?;
    let mut current = settings(&state.db, &id).await;
    for (k, v) in body.settings {
        if !info.settings.iter().any(|f| f.key == k) {
            return Err(AppError::BadRequest(format!("unknown setting {k}")));
        }
        if v.is_empty() {
            current.remove(&k);
        } else {
            current.insert(k, v);
        }
    }
    let enabled = body.enabled.unwrap_or(is_enabled(&state.db, &id).await);
    save(&state.db, &id, enabled, &current).await?;
    Ok(Json(view(&state.db, info).await))
}
