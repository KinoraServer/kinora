//! Browsing: home rows, library grids, item details, images, watch progress.

use std::path::PathBuf;

use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use kinora_core::media::MediaInfo;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::auth::AuthUser;
use crate::db::now_ms;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/home", get(home))
        .route("/api/home/featured", get(featured))
        .route("/api/items", get(list_items))
        .route("/api/items/{id}", get(item_detail))
        .route("/api/items/{id}/progress", post(progress))
        .route("/api/items/{id}/played", post(set_played))
        .route("/api/items/{id}/images/{kind}", get(image))
}

/// Compact item for grids and rows.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ItemCard {
    pub id: String,
    pub library_id: String,
    pub kind: String,
    pub parent_id: Option<String>,
    pub series_id: Option<String>,
    pub title: String,
    pub year: Option<i64>,
    pub season_number: Option<i64>,
    pub episode_number: Option<i64>,
    pub series_title: Option<String>,
    pub has_poster: bool,
    pub has_backdrop: bool,
    pub has_thumb: bool,
    /// Title art (a transparent logo), see `images/logo`.
    pub has_logo: bool,
    pub duration_ms: Option<i64>,
    pub position_ms: Option<i64>,
    pub played: Option<bool>,
    pub rating: Option<f64>,
    /// When the item was added to the library (Unix ms).
    pub added_at: i64,
}

const CARD_SELECT: &str = "
    SELECT i.id, i.library_id, i.kind, i.parent_id, i.series_id, i.title, i.year, i.season_number, i.episode_number,
           s.title AS series_title,
           (COALESCE(i.poster_url, s.poster_url) IS NOT NULL) AS has_poster,
           (COALESCE(i.backdrop_url, s.backdrop_url) IS NOT NULL) AS has_backdrop,
           (i.thumb_url IS NOT NULL) AS has_thumb,
           (COALESCE(i.logo_url, s.logo_url) IS NOT NULL) AS has_logo,
           (SELECT MAX(f.duration_ms) FROM media_files f WHERE f.item_id = i.id) AS duration_ms,
           u.position_ms, (u.played = 1) AS played, i.rating, i.added_at
    FROM items i
    LEFT JOIN items s ON s.id = i.series_id
    LEFT JOIN user_item_data u ON u.item_id = i.id AND u.user_id = ?";

#[derive(Serialize)]
struct HomeRow {
    id: String,
    title: String,
    items: Vec<ItemCard>,
}

async fn home(State(state): State<AppState>, user: AuthUser) -> AppResult<Json<Vec<HomeRow>>> {
    let mut rows = Vec::new();

    let continue_watching: Vec<ItemCard> = sqlx::query_as(&format!(
        "{CARD_SELECT} WHERE u.position_ms > 0 AND u.played = 0 AND i.kind IN ('movie','episode')
         ORDER BY u.last_played_at DESC LIMIT 20"
    ))
    .bind(&user.id)
    .fetch_all(&state.db)
    .await?;
    if !continue_watching.is_empty() {
        rows.push(HomeRow { id: "continue".into(), title: "Continue watching".into(), items: continue_watching });
    }

    let libraries: Vec<(String, String, String)> =
        sqlx::query_as("SELECT id, name, kind FROM libraries ORDER BY name").fetch_all(&state.db).await?;
    for (id, name, kind) in libraries {
        let top_kind = if kind == "tv" { "series" } else { "movie" };
        let items: Vec<ItemCard> = sqlx::query_as(&format!(
            "{CARD_SELECT} WHERE i.library_id = ? AND i.kind = ? ORDER BY i.added_at DESC LIMIT 20"
        ))
        .bind(&user.id)
        .bind(&id)
        .bind(top_kind)
        .fetch_all(&state.db)
        .await?;
        if !items.is_empty() {
            rows.push(HomeRow { id: format!("recent-{id}"), title: format!("Recently added in {name}"), items });
        }
    }
    Ok(Json(rows))
}

/// A title for a home page hero: a card plus the text a hero shows.
#[derive(Serialize)]
struct FeaturedItem {
    #[serde(flatten)]
    card: ItemCard,
    overview: Option<String>,
    tagline: Option<String>,
    official_rating: Option<String>,
    runtime_minutes: Option<i64>,
    genres: Vec<String>,
}

/// Recently added movies and series with a backdrop and a description that
/// this viewer hasn't watched yet: what app skins show in a home page hero.
async fn featured(State(state): State<AppState>, user: AuthUser) -> AppResult<Json<Vec<FeaturedItem>>> {
    let cards: Vec<ItemCard> = sqlx::query_as(&format!(
        "{CARD_SELECT} WHERE i.kind IN ('movie', 'series') AND i.backdrop_url IS NOT NULL
         AND i.overview IS NOT NULL AND i.overview != '' AND COALESCE(u.played, 0) = 0
         ORDER BY i.added_at DESC LIMIT 8"
    ))
    .bind(&user.id)
    .fetch_all(&state.db)
    .await?;
    let mut out = Vec::with_capacity(cards.len());
    for card in cards {
        let extra: (Option<String>, Option<String>, Option<String>, Option<i64>, String) = sqlx::query_as(
            "SELECT overview, tagline, official_rating, runtime_minutes, genres FROM items WHERE id = ?",
        )
        .bind(&card.id)
        .fetch_one(&state.db)
        .await?;
        out.push(FeaturedItem {
            card,
            overview: extra.0,
            tagline: extra.1,
            official_rating: extra.2,
            runtime_minutes: extra.3,
            genres: serde_json::from_str(&extra.4).unwrap_or_default(),
        });
    }
    Ok(Json(out))
}

#[derive(Deserialize)]
struct ListQuery {
    library_id: Option<String>,
    parent_id: Option<String>,
    kind: Option<String>,
    q: Option<String>,
    #[serde(default)]
    sort: Option<String>,
    #[serde(default)]
    limit: Option<i64>,
    #[serde(default)]
    offset: Option<i64>,
}

async fn list_items(
    State(state): State<AppState>,
    user: AuthUser,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<Vec<ItemCard>>> {
    let mut sql = format!("{CARD_SELECT} WHERE 1=1");
    let mut binds: Vec<String> = vec![user.id.clone()];
    if let Some(l) = &q.library_id {
        sql.push_str(" AND i.library_id = ?");
        binds.push(l.clone());
    }
    if let Some(p) = &q.parent_id {
        sql.push_str(" AND i.parent_id = ?");
        binds.push(p.clone());
    }
    if let Some(k) = &q.kind {
        sql.push_str(" AND i.kind = ?");
        binds.push(k.clone());
    } else if q.parent_id.is_none() {
        sql.push_str(" AND i.kind IN ('movie', 'series')");
    }
    if let Some(text) = q.q.as_ref().filter(|t| !t.trim().is_empty()) {
        sql.push_str(" AND i.title LIKE ? ESCAPE '\\'");
        let escaped = text.trim().replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
        binds.push(format!("%{escaped}%"));
    }
    sql.push_str(match q.sort.as_deref() {
        Some("added") => " ORDER BY i.added_at DESC",
        Some("year") => " ORDER BY i.year DESC, i.sort_title",
        Some("rating") => " ORDER BY i.rating DESC NULLS LAST, i.sort_title",
        _ if q.parent_id.is_some() => " ORDER BY i.season_number, i.episode_number, i.sort_title",
        _ => " ORDER BY i.sort_title",
    });
    sql.push_str(" LIMIT ? OFFSET ?");

    let mut query = sqlx::query_as::<_, ItemCard>(&sql);
    for b in &binds {
        query = query.bind(b);
    }
    let items = query
        .bind(q.limit.unwrap_or(500).clamp(1, 2000))
        .bind(q.offset.unwrap_or(0).max(0))
        .fetch_all(&state.db)
        .await?;
    Ok(Json(items))
}

#[derive(Serialize, sqlx::FromRow)]
struct ItemRow {
    id: String,
    library_id: String,
    kind: String,
    parent_id: Option<String>,
    series_id: Option<String>,
    title: String,
    original_title: Option<String>,
    year: Option<i64>,
    season_number: Option<i64>,
    episode_number: Option<i64>,
    episode_end: Option<i64>,
    overview: Option<String>,
    tagline: Option<String>,
    rating: Option<f64>,
    official_rating: Option<String>,
    /// Stored as a JSON array in text; `ItemDetail` sends the parsed list
    /// under the same key, so this copy must not be serialized as well.
    #[serde(skip_serializing)]
    genres: String,
    runtime_minutes: Option<i64>,
    release_date: Option<String>,
    metadata_status: String,
    added_at: i64,
}

#[derive(Serialize)]
struct MediaFileView {
    id: String,
    path: String,
    size: i64,
    duration_ms: Option<i64>,
    label: Option<String>,
    info: Option<MediaInfo>,
}

#[derive(Serialize)]
struct ItemDetail {
    #[serde(flatten)]
    item: ItemRow,
    genres: Vec<String>,
    series_title: Option<String>,
    has_poster: bool,
    has_backdrop: bool,
    has_thumb: bool,
    has_logo: bool,
    files: Vec<MediaFileView>,
    children: Vec<ItemCard>,
    position_ms: i64,
    played: bool,
    /// For series/seasons: the episode to play next.
    next_up: Option<ItemCard>,
}

async fn item_detail(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Json<ItemDetail>> {
    let item: ItemRow = sqlx::query_as(
        "SELECT id, library_id, kind, parent_id, series_id, title, original_title, year, season_number,
                episode_number, episode_end, overview, tagline, rating, official_rating, genres, runtime_minutes,
                release_date, metadata_status, added_at
         FROM items WHERE id = ?",
    )
    .bind(&id)
    .fetch_one(&state.db)
    .await?;

    let images: (Option<String>, bool, bool, bool, bool) = sqlx::query_as(
        "SELECT s.title, (COALESCE(i.poster_url, s.poster_url) IS NOT NULL),
                (COALESCE(i.backdrop_url, s.backdrop_url) IS NOT NULL), (i.thumb_url IS NOT NULL),
                (COALESCE(i.logo_url, s.logo_url) IS NOT NULL)
         FROM items i LEFT JOIN items s ON s.id = i.series_id WHERE i.id = ?",
    )
    .bind(&id)
    .fetch_one(&state.db)
    .await?;

    let files: Vec<(String, String, i64, Option<i64>, Option<String>)> = sqlx::query_as(
        "SELECT id, path, size, duration_ms, info FROM media_files WHERE item_id = ? ORDER BY size DESC",
    )
    .bind(&id)
    .fetch_all(&state.db)
    .await?;
    let files = files
        .into_iter()
        .map(|(fid, path, size, duration_ms, info)| {
            let info: Option<MediaInfo> = info.and_then(|i| serde_json::from_str(&i).ok());
            MediaFileView {
                id: fid,
                path: if user.role.at_least(kinora_core::roles::Role::Admin) {
                    path
                } else {
                    PathBuf::from(path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
                },
                size,
                duration_ms,
                label: info.as_ref().and_then(|i| i.video_label()),
                info,
            }
        })
        .collect();

    let children: Vec<ItemCard> = sqlx::query_as(&format!(
        "{CARD_SELECT} WHERE i.parent_id = ? ORDER BY i.season_number, i.episode_number, i.sort_title"
    ))
    .bind(&user.id)
    .bind(&id)
    .fetch_all(&state.db)
    .await?;

    let ud: Option<(i64, i64)> =
        sqlx::query_as("SELECT position_ms, played FROM user_item_data WHERE user_id = ? AND item_id = ?")
            .bind(&user.id)
            .bind(&id)
            .fetch_optional(&state.db)
            .await?;

    let next_up = if item.kind == "series" || item.kind == "season" {
        let filter = if item.kind == "series" { "i.series_id = ?" } else { "i.parent_id = ?" };
        sqlx::query_as::<_, ItemCard>(&format!(
            "{CARD_SELECT} WHERE {filter} AND i.kind = 'episode' AND COALESCE(u.played, 0) = 0
             ORDER BY i.season_number = 0, i.season_number, i.episode_number LIMIT 1"
        ))
        .bind(&user.id)
        .bind(&id)
        .fetch_optional(&state.db)
        .await?
    } else {
        None
    };

    Ok(Json(ItemDetail {
        genres: serde_json::from_str(&item.genres).unwrap_or_default(),
        item,
        series_title: images.0,
        has_poster: images.1,
        has_backdrop: images.2,
        has_thumb: images.3,
        has_logo: images.4,
        files,
        children,
        position_ms: ud.map(|u| u.0).unwrap_or(0),
        played: ud.map(|u| u.1 != 0).unwrap_or(false),
        next_up,
    }))
}

#[derive(Deserialize)]
struct Progress {
    position_ms: i64,
    duration_ms: Option<i64>,
}

/// Saved every few seconds by the player. Marks the item watched past 90%.
async fn progress(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    Json(p): Json<Progress>,
) -> AppResult<StatusCode> {
    state.now_playing.progress(&user.id, &id, p.position_ms.max(0) as u64);
    let finished = p.duration_ms.map(|d| d > 0 && p.position_ms * 10 >= d * 9).unwrap_or(false);
    let position = if finished { 0 } else { p.position_ms.max(0) };
    sqlx::query(
        "INSERT INTO user_item_data (user_id, item_id, position_ms, played, play_count, last_played_at)
         VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT(user_id, item_id) DO UPDATE SET
            position_ms = excluded.position_ms,
            played = MAX(played, excluded.played),
            play_count = play_count + (excluded.played > played),
            last_played_at = excluded.last_played_at",
    )
    .bind(&user.id)
    .bind(&id)
    .bind(position)
    .bind(finished as i64)
    .bind(finished as i64)
    .bind(now_ms())
    .execute(&state.db)
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct Played {
    played: bool,
}

/// Mark watched / unwatched. For a series or season, applies to every episode.
async fn set_played(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    Json(body): Json<Played>,
) -> AppResult<StatusCode> {
    let targets: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM items WHERE id = ? AND kind IN ('movie','episode')
         UNION SELECT id FROM items WHERE kind = 'episode' AND (series_id = ? OR parent_id = ?)",
    )
    .bind(&id)
    .bind(&id)
    .bind(&id)
    .fetch_all(&state.db)
    .await?;
    let now = now_ms();
    for t in targets {
        sqlx::query(
            "INSERT INTO user_item_data (user_id, item_id, position_ms, played, last_played_at) VALUES (?, ?, 0, ?, ?)
             ON CONFLICT(user_id, item_id) DO UPDATE SET played = excluded.played, position_ms = 0",
        )
        .bind(&user.id)
        .bind(&t)
        .bind(body.played as i64)
        .bind(now)
        .execute(&state.db)
        .await?;
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct ImageQuery {
    /// Resize to this width (one of the app skin image widths); the original otherwise.
    w: Option<u32>,
}

/// Resizes run through ffmpeg; a few at a time so a page full of cards can't swamp the CPU.
static RESIZES: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(4);

fn image_mime(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(b"\x89PNG") {
        "image/png"
    } else if bytes.get(8..12) == Some(b"WEBP") {
        "image/webp"
    } else if bytes.starts_with(b"<svg") || bytes.starts_with(b"<?xml") {
        "image/svg+xml"
    } else {
        "image/jpeg"
    }
}

/// Serve an image, downloading and caching it on first request.
/// Seasons and episodes fall back to their series' artwork.
/// `?w=` serves a smaller copy (made once with ffmpeg and cached).
async fn image(
    State(state): State<AppState>,
    _user: AuthUser,
    Path((id, kind)): Path<(String, String)>,
    Query(q): Query<ImageQuery>,
) -> AppResult<Response> {
    let column = match kind.as_str() {
        "poster" => "COALESCE(i.poster_url, s.poster_url)",
        "backdrop" => "COALESCE(i.backdrop_url, s.backdrop_url)",
        "thumb" => "COALESCE(i.thumb_url, i.backdrop_url, s.backdrop_url)",
        "logo" => "COALESCE(i.logo_url, s.logo_url)",
        _ => return Err(AppError::NotFound),
    };
    let url: Option<String> = sqlx::query_scalar(&format!(
        "SELECT {column} FROM items i LEFT JOIN items s ON s.id = i.series_id WHERE i.id = ?"
    ))
    .bind(&id)
    .fetch_one(&state.db)
    .await?;
    let url = url.ok_or(AppError::NotFound)?;

    let name = hex::encode(&Sha256::digest(url.as_bytes())[..16]);
    let path = state.config.image_cache_dir().join(format!("{name}.img"));
    let mut bytes = match tokio::fs::read(&path).await {
        Ok(b) => b,
        Err(_) => {
            let resp = state.http.get(&url).send().await.map_err(anyhow::Error::from)?;
            if !resp.status().is_success() {
                return Err(AppError::NotFound);
            }
            let b = resp.bytes().await.map_err(anyhow::Error::from)?.to_vec();
            tokio::fs::create_dir_all(state.config.image_cache_dir()).await?;
            tokio::fs::write(&path, &b).await?;
            b
        }
    };
    let mut mime = image_mime(&bytes);
    let widths = &kinora_plugin_api::app_skin::rules().image_widths;
    if let Some(w) = q.w.filter(|w| widths.contains(w)) {
        if mime != "image/svg+xml" {
            // Logos keep their transparency (PNG); everything else becomes a JPEG.
            let png = mime == "image/png" && kind == "logo";
            let out = state.config.image_cache_dir().join(format!("{name}-w{w}.{}", if png { "png" } else { "jpg" }));
            if let Some(small) = resized(&state.config.ffmpeg, &path, &out, w, png).await {
                bytes = small;
                mime = if png { "image/png" } else { "image/jpeg" };
            }
        }
    }
    let mut resp =
        ([(header::CONTENT_TYPE, mime), (header::CACHE_CONTROL, "private, max-age=604800")], Body::from(bytes))
            .into_response();
    resp.headers_mut().insert(header::X_CONTENT_TYPE_OPTIONS, header::HeaderValue::from_static("nosniff"));
    if mime == "image/svg+xml" {
        resp.headers_mut().insert(
            header::CONTENT_SECURITY_POLICY,
            header::HeaderValue::from_static("default-src 'none'; style-src 'unsafe-inline'"),
        );
    }
    Ok(resp)
}

/// A cached smaller copy of `src`, made on first use. None when ffmpeg fails
/// (the caller then sends the original).
async fn resized(ffmpeg: &str, src: &std::path::Path, out: &std::path::Path, width: u32, png: bool) -> Option<Vec<u8>> {
    if let Ok(b) = tokio::fs::read(out).await {
        return Some(b);
    }
    let _permit = RESIZES.acquire().await.ok()?;
    // Another request may have made it while this one waited.
    if let Ok(b) = tokio::fs::read(out).await {
        return Some(b);
    }
    let tmp = out.with_extension(format!("tmp-{}", crate::db::new_id()));
    let codec: &[&str] = if png { &["-c:v", "png"] } else { &["-c:v", "mjpeg", "-q:v", "4"] };
    let status = tokio::process::Command::new(ffmpeg)
        .args(["-v", "error", "-y", "-i"])
        .arg(src)
        .args(["-vf", &format!("scale='min(iw,{width})':-2"), "-frames:v", "1"])
        .args(codec)
        .args(["-f", "image2"])
        .arg(&tmp)
        .kill_on_drop(true)
        .status()
        .await
        .ok()?;
    if !status.success() {
        let _ = tokio::fs::remove_file(&tmp).await;
        tracing::debug!(src = %src.display(), "image resize failed; sending the original");
        return None;
    }
    tokio::fs::rename(&tmp, out).await.ok()?;
    tokio::fs::read(out).await.ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_detail_sends_genres_once_as_a_list() {
        let item = ItemRow {
            id: "i1".into(),
            library_id: "l1".into(),
            kind: "movie".into(),
            parent_id: None,
            series_id: None,
            title: "Dune".into(),
            original_title: None,
            year: Some(2021),
            season_number: None,
            episode_number: None,
            episode_end: None,
            overview: None,
            tagline: None,
            rating: None,
            official_rating: None,
            genres: r#"["Action","Science Fiction"]"#.into(),
            runtime_minutes: None,
            release_date: None,
            metadata_status: "matched".into(),
            added_at: 0,
        };
        let detail = ItemDetail {
            genres: serde_json::from_str(&item.genres).unwrap(),
            item,
            series_title: None,
            has_poster: false,
            has_backdrop: false,
            has_thumb: false,
            has_logo: false,
            files: vec![],
            children: vec![],
            position_ms: 0,
            played: false,
            next_up: None,
        };
        let json = serde_json::to_string(&detail).unwrap();
        assert_eq!(json.matches("\"genres\"").count(), 1, "{json}");
        assert!(json.contains(r#""genres":["Action","Science Fiction"]"#), "{json}");
    }
}
