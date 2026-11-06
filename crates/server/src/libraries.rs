//! Library management (admin) and a folder picker for choosing paths.

use std::path::PathBuf;

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::auth::{AdminUser, AuthUser};
use crate::db::{new_id, now_ms};
use crate::error::{AppError, AppResult};
use crate::scanner::{self, ScanStatus};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/libraries", get(list).post(create))
        .route("/api/libraries/{id}", axum::routing::put(update).delete(remove))
        .route("/api/libraries/{id}/scan", post(scan))
        .route("/api/libraries/scan-all", post(scan_all))
        .route("/api/fs/folders", get(folders))
}

#[derive(Serialize)]
struct LibraryView {
    id: String,
    name: String,
    kind: String,
    paths: Vec<String>,
    created_at: i64,
    last_scan_at: Option<i64>,
    item_count: i64,
    scan: Option<ScanStatus>,
}

async fn list(State(state): State<AppState>, _user: AuthUser) -> AppResult<Json<Vec<LibraryView>>> {
    let rows: Vec<(String, String, String, String, i64, Option<i64>, i64)> = sqlx::query_as(
        "SELECT l.id, l.name, l.kind, l.paths, l.created_at, l.last_scan_at,
                (SELECT COUNT(*) FROM items i WHERE i.library_id = l.id AND i.kind IN ('movie','series'))
         FROM libraries l ORDER BY l.name",
    )
    .fetch_all(&state.db)
    .await?;
    let status = state.scans.status().await;
    Ok(Json(
        rows.into_iter()
            .map(|(id, name, kind, paths, created_at, last_scan_at, item_count)| LibraryView {
                scan: status.get(&id).cloned(),
                id,
                name,
                kind,
                paths: serde_json::from_str(&paths).unwrap_or_default(),
                created_at,
                last_scan_at,
                item_count,
            })
            .collect(),
    ))
}

#[derive(Deserialize)]
struct LibraryInput {
    name: String,
    kind: String,
    paths: Vec<String>,
}

fn validate(input: &LibraryInput) -> AppResult<()> {
    if input.name.trim().is_empty() {
        return Err(AppError::BadRequest("give the library a name".into()));
    }
    if input.kind != "movies" && input.kind != "tv" {
        return Err(AppError::BadRequest("kind must be movies or tv".into()));
    }
    if input.paths.is_empty() {
        return Err(AppError::BadRequest("add at least one folder".into()));
    }
    for p in &input.paths {
        if !PathBuf::from(p).is_dir() {
            return Err(AppError::BadRequest(format!("folder not found: {p}")));
        }
    }
    Ok(())
}

async fn create(
    State(state): State<AppState>,
    _admin: AdminUser,
    Json(input): Json<LibraryInput>,
) -> AppResult<Json<serde_json::Value>> {
    validate(&input)?;
    let id = new_id();
    sqlx::query("INSERT INTO libraries (id, name, kind, paths, created_at) VALUES (?, ?, ?, ?, ?)")
        .bind(&id)
        .bind(input.name.trim())
        .bind(&input.kind)
        .bind(serde_json::to_string(&input.paths).map_err(anyhow::Error::from)?)
        .bind(now_ms())
        .execute(&state.db)
        .await?;
    scanner::start_scan(&state, &id).await;
    Ok(Json(serde_json::json!({ "id": id })))
}

async fn update(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(id): Path<String>,
    Json(input): Json<LibraryInput>,
) -> AppResult<Json<serde_json::Value>> {
    validate(&input)?;
    let res = sqlx::query("UPDATE libraries SET name = ?, kind = ?, paths = ? WHERE id = ?")
        .bind(input.name.trim())
        .bind(&input.kind)
        .bind(serde_json::to_string(&input.paths).map_err(anyhow::Error::from)?)
        .bind(&id)
        .execute(&state.db)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    scanner::start_scan(&state, &id).await;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn remove(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(id): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    // Media files on disk are never touched; only our records are removed.
    sqlx::query("DELETE FROM libraries WHERE id = ?").bind(&id).execute(&state.db).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn scan(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(id): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    let started = scanner::start_scan(&state, &id).await;
    Ok(Json(serde_json::json!({ "started": started })))
}

async fn scan_all(State(state): State<AppState>, _admin: AdminUser) -> AppResult<Json<serde_json::Value>> {
    scanner::scan_all(&state).await;
    Ok(Json(serde_json::json!({ "ok": true })))
}

#[derive(Deserialize)]
struct FolderQuery {
    path: Option<String>,
}

#[derive(Serialize)]
struct FolderList {
    path: String,
    parent: Option<String>,
    folders: Vec<String>,
}

/// List sub-folders so admins can pick library paths in the UI.
async fn folders(_admin: AdminUser, Query(q): Query<FolderQuery>) -> AppResult<Json<FolderList>> {
    let default_root = if PathBuf::from("/media").is_dir() { "/media" } else { "/" };
    let path = PathBuf::from(q.path.filter(|p| !p.is_empty()).unwrap_or_else(|| default_root.into()));
    let mut folders: Vec<String> = std::fs::read_dir(&path)
        .map_err(|_| AppError::BadRequest(format!("can't open {}", path.display())))?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| !n.starts_with('.'))
        .collect();
    folders.sort_by_key(|a| a.to_lowercase());
    Ok(Json(FolderList {
        parent: path.parent().map(|p| p.display().to_string()),
        path: path.display().to_string(),
        folders,
    }))
}
