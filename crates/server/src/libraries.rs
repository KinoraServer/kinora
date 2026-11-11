//! Library management (admin) and a folder picker for choosing paths.

use std::path::{Path as FsPath, PathBuf};

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

/// An edit. Every field is optional; send only what changes. `kind` may be
/// sent (the admin UI sends the whole library) but can't change: the items
/// of a movie library and a TV library are built differently.
#[derive(Deserialize)]
struct LibraryEdit {
    name: Option<String>,
    kind: Option<String>,
    paths: Option<Vec<String>>,
}

/// Trim a folder path and drop trailing slashes, so `/media/tv/` and
/// `/media/tv` are the same folder.
fn normalize_path(p: &str) -> String {
    let p = p.trim();
    let trimmed = p.trim_end_matches(['/', '\\']);
    if trimmed.is_empty() || trimmed.ends_with(':') {
        // "/" or a Windows drive root like `C:\`.
        p.to_string()
    } else {
        trimmed.to_string()
    }
}

/// True when one folder is the other or sits inside it.
fn overlaps(a: &str, b: &str) -> bool {
    let (a, b) = (FsPath::new(a), FsPath::new(b));
    a.starts_with(b) || b.starts_with(a)
}

async fn validate_name(state: &AppState, name: &str, except_id: Option<&str>) -> AppResult<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::BadRequest("give the library a name".into()));
    }
    let taken: Option<String> =
        sqlx::query_scalar("SELECT id FROM libraries WHERE name = ? COLLATE NOCASE AND id IS NOT ?")
            .bind(name)
            .bind(except_id)
            .fetch_optional(&state.db)
            .await?;
    if taken.is_some() {
        return Err(AppError::Conflict(format!("there is already a library called {name}")));
    }
    Ok(name.to_string())
}

fn validate_kind(kind: &str) -> AppResult<()> {
    if kind != "movies" && kind != "tv" {
        return Err(AppError::BadRequest("kind must be movies or tv".into()));
    }
    Ok(())
}

/// Check a library's folders and return them cleaned up. Each must exist and
/// be readable, and no folder may be inside another, in this library or any
/// other one: a file can only belong to one library.
async fn validate_paths(state: &AppState, paths: &[String], except_id: Option<&str>) -> AppResult<Vec<String>> {
    let mut clean: Vec<String> = Vec::new();
    for p in paths.iter().map(|p| normalize_path(p)) {
        if p.is_empty() || clean.contains(&p) {
            continue;
        }
        clean.push(p);
    }
    if clean.is_empty() {
        return Err(AppError::BadRequest("add at least one folder".into()));
    }
    for p in &clean {
        let path = PathBuf::from(p);
        if !path.is_dir() {
            return Err(AppError::BadRequest(format!("folder not found: {p}")));
        }
        if std::fs::read_dir(&path).is_err() {
            return Err(AppError::BadRequest(format!("can't read folder: {p}")));
        }
    }
    for (i, a) in clean.iter().enumerate() {
        if let Some(b) = clean[i + 1..].iter().find(|b| overlaps(a, b)) {
            return Err(AppError::BadRequest(format!("{a} and {b} overlap; keep only the outer folder")));
        }
    }
    let others: Vec<(String, String)> = sqlx::query_as("SELECT name, paths FROM libraries WHERE id IS NOT ?")
        .bind(except_id)
        .fetch_all(&state.db)
        .await?;
    for (name, other_paths) in others {
        let other_paths: Vec<String> = serde_json::from_str(&other_paths).unwrap_or_default();
        for p in &clean {
            if let Some(o) = other_paths.iter().find(|o| overlaps(p, &normalize_path(o))) {
                return Err(AppError::Conflict(format!("{p} overlaps {o}, which the library {name} already uses")));
            }
        }
    }
    Ok(clean)
}

async fn create(
    State(state): State<AppState>,
    _admin: AdminUser,
    Json(input): Json<LibraryInput>,
) -> AppResult<Json<serde_json::Value>> {
    let name = validate_name(&state, &input.name, None).await?;
    validate_kind(&input.kind)?;
    let paths = validate_paths(&state, &input.paths, None).await?;
    let id = new_id();
    sqlx::query("INSERT INTO libraries (id, name, kind, paths, created_at) VALUES (?, ?, ?, ?, ?)")
        .bind(&id)
        .bind(&name)
        .bind(&input.kind)
        .bind(serde_json::to_string(&paths).map_err(anyhow::Error::from)?)
        .bind(now_ms())
        .execute(&state.db)
        .await?;
    scanner::start_scan(&state, &id).await;
    Ok(Json(serde_json::json!({ "id": id })))
}

/// Rename a library or change its folders. The id (and so every link to the
/// library) stays the same. Changing folders starts a rescan: files in new
/// folders are added, files no longer under any folder are removed, and
/// files that merely moved keep their items and watch progress.
async fn update(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(id): Path<String>,
    Json(input): Json<LibraryEdit>,
) -> AppResult<Json<serde_json::Value>> {
    let (old_name, kind, old_paths): (String, String, String) =
        sqlx::query_as("SELECT name, kind, paths FROM libraries WHERE id = ?").bind(&id).fetch_one(&state.db).await?;
    if input.kind.as_ref().is_some_and(|k| *k != kind) {
        return Err(AppError::BadRequest(
            "a library's type can't be changed; add a new library for the other type".into(),
        ));
    }
    let name = match &input.name {
        Some(n) => validate_name(&state, n, Some(&id)).await?,
        None => old_name,
    };
    let old_paths: Vec<String> = serde_json::from_str(&old_paths).unwrap_or_default();
    let paths = match &input.paths {
        Some(p) => validate_paths(&state, p, Some(&id)).await?,
        None => old_paths.clone(),
    };
    sqlx::query("UPDATE libraries SET name = ?, paths = ? WHERE id = ?")
        .bind(&name)
        .bind(serde_json::to_string(&paths).map_err(anyhow::Error::from)?)
        .bind(&id)
        .execute(&state.db)
        .await?;
    let rescan = paths != old_paths;
    if rescan {
        scanner::rescan_after_change(&state, &id).await;
    }
    Ok(Json(serde_json::json!({ "ok": true, "rescan": rescan })))
}

async fn remove(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(id): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    // Media files on disk are never touched; only our records are removed.
    sqlx::query("DELETE FROM libraries WHERE id = ?").bind(&id).execute(&state.db).await?;
    state.scans.forget(&id).await;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_normalized() {
        assert_eq!(normalize_path(" /media/tv/ "), "/media/tv");
        assert_eq!(normalize_path("/media/tv//"), "/media/tv");
        assert_eq!(normalize_path("/"), "/");
        assert_eq!(normalize_path(r"C:\"), r"C:\");
    }

    #[test]
    fn overlapping_folders() {
        assert!(overlaps("/media", "/media/tv"));
        assert!(overlaps("/media/tv", "/media"));
        assert!(overlaps("/media/tv", "/media/tv"));
        // A shared prefix isn't nesting.
        assert!(!overlaps("/media/tv", "/media/tv-4k"));
        assert!(!overlaps("/media/movies", "/media/tv"));
    }
}
