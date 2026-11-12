//! Library scanner: walk folders, probe new or changed files, build the
//! movie / series / season / episode tree, then fetch metadata.
//!
//! Incremental: a file whose size and modification time haven't changed is
//! skipped, so a rescan of a big library takes seconds.
//!
//! Moved files: a file that vanished from one place and turned up in another
//! (same file name and size) keeps its record, so its item, metadata and
//! everyone's watch progress follow it. This is what makes changing a
//! library's folders, or reorganising files on disk, safe.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use kinora_core::media::MediaInfo;
use kinora_core::naming::{is_video_file, parse_episode, parse_movie};
use kinora_plugin_api::{MediaKind, Metadata, MetadataProvider, ServerEvent};
use serde::Serialize;
use sqlx::SqlitePool;
use tokio::sync::{RwLock, Semaphore};

use crate::db::{new_id, now_ms};
use crate::modules;
use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Default)]
pub struct ScanStatus {
    pub running: bool,
    pub phase: String,
    pub processed: u32,
    pub total: u32,
    pub added: u32,
    pub removed: u32,
    pub errors: u32,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub last_error: Option<String>,
}

#[derive(Clone, Default)]
pub struct ScanManager {
    status: Arc<RwLock<HashMap<String, ScanStatus>>>,
    /// Libraries whose folders changed while a scan was running: they are
    /// scanned again as soon as that scan finishes.
    rerun: Arc<std::sync::Mutex<HashSet<String>>>,
}

impl ScanManager {
    pub async fn status(&self) -> HashMap<String, ScanStatus> {
        self.status.read().await.clone()
    }

    async fn update(&self, library_id: &str, f: impl FnOnce(&mut ScanStatus)) {
        let mut map = self.status.write().await;
        f(map.entry(library_id.to_string()).or_default());
    }

    async fn is_running(&self, library_id: &str) -> bool {
        self.status.read().await.get(library_id).map(|s| s.running).unwrap_or(false)
    }

    fn take_rerun(&self, library_id: &str) -> bool {
        self.rerun.lock().map(|mut set| set.remove(library_id)).unwrap_or(false)
    }

    /// Forget a deleted library's status.
    pub async fn forget(&self, library_id: &str) {
        self.status.write().await.remove(library_id);
        if let Ok(mut set) = self.rerun.lock() {
            set.remove(library_id);
        }
    }
}

/// Scan a library whose settings just changed. If a scan is already running
/// it read the old folders, so another one runs right after it.
pub async fn rescan_after_change(state: &AppState, library_id: &str) {
    if !start_scan(state, library_id).await {
        if let Ok(mut set) = state.scans.rerun.lock() {
            set.insert(library_id.to_string());
        }
    }
}

#[derive(sqlx::FromRow)]
struct LibraryRow {
    kind: String,
    paths: String,
}

/// Start a scan in the background. Returns false if one is already running.
pub async fn start_scan(state: &AppState, library_id: &str) -> bool {
    if state.scans.is_running(library_id).await {
        return false;
    }
    state
        .scans
        .update(library_id, |s| {
            *s =
                ScanStatus { running: true, phase: "starting".into(), started_at: Some(now_ms()), ..Default::default() }
        })
        .await;
    let state = state.clone();
    let id = library_id.to_string();
    tokio::spawn(async move {
        run_scan(&state, &id).await;
        while state.scans.take_rerun(&id) {
            state
                .scans
                .update(&id, |s| {
                    *s = ScanStatus {
                        running: true,
                        phase: "starting".into(),
                        started_at: Some(now_ms()),
                        ..Default::default()
                    }
                })
                .await;
            run_scan(&state, &id).await;
        }
    });
    true
}

/// One scan, start to finish, with its status and events. The caller has
/// already marked the library as scanning.
async fn run_scan(state: &AppState, id: &str) {
    let id = id.to_string();
    state.emit(ServerEvent::LibraryScanStarted { library_id: id.clone() });
    let result = scan_library(state, &id).await;
    let (added, removed) = match &result {
        Ok(counts) => *counts,
        Err(_) => (0, 0),
    };
    state
        .scans
        .update(&id, |s| {
            s.running = false;
            s.phase = if result.is_ok() { "done".into() } else { "failed".into() };
            s.finished_at = Some(now_ms());
            if let Err(e) = &result {
                s.last_error = Some(e.to_string());
            }
        })
        .await;
    match result {
        Ok(_) => {
            let _ = sqlx::query("UPDATE libraries SET last_scan_at = ? WHERE id = ?")
                .bind(now_ms())
                .bind(&id)
                .execute(&state.db)
                .await;
            state.emit(ServerEvent::LibraryScanFinished { library_id: id, added, removed });
        }
        Err(e) => tracing::error!(library = %id, error = ?e, "scan failed"),
    }
}

pub async fn scan_all(state: &AppState) {
    let ids: Vec<String> =
        sqlx::query_scalar("SELECT id FROM libraries").fetch_all(&state.db).await.unwrap_or_default();
    for id in ids {
        start_scan(state, &id).await;
    }
}

struct Found {
    path: PathBuf,
    root: PathBuf,
    size: i64,
    mtime: i64,
}

fn walk(roots: &[PathBuf]) -> Vec<Found> {
    let mut out = Vec::new();
    for root in roots {
        for entry in walkdir::WalkDir::new(root).follow_links(true).into_iter().filter_map(|e| e.ok()) {
            if !entry.file_type().is_file() || !is_video_file(entry.path()) {
                continue;
            }
            // Skip macOS resource forks and sample clips.
            let name = entry.file_name().to_string_lossy().to_lowercase();
            if name.starts_with("._") || name.contains("-sample.") || name.contains(".sample.") {
                continue;
            }
            let Ok(meta) = entry.metadata() else { continue };
            let mtime = meta
                .modified()
                .ok()
                .and_then(|m| m.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);
            out.push(Found { path: entry.path().to_path_buf(), root: root.clone(), size: meta.len() as i64, mtime });
        }
    }
    out
}

async fn scan_library(state: &AppState, library_id: &str) -> anyhow::Result<(u32, u32)> {
    let lib: LibraryRow =
        sqlx::query_as("SELECT kind, paths FROM libraries WHERE id = ?").bind(library_id).fetch_one(&state.db).await?;
    let roots: Vec<PathBuf> = serde_json::from_str::<Vec<String>>(&lib.paths)?.into_iter().map(PathBuf::from).collect();
    for r in &roots {
        if !r.is_dir() {
            anyhow::bail!("folder not found or not readable: {}", r.display());
        }
    }

    state.scans.update(library_id, |s| s.phase = "finding files".into()).await;
    let roots_clone = roots.clone();
    let found = tokio::task::spawn_blocking(move || walk(&roots_clone)).await?;

    let existing: Vec<(String, String, i64, i64)> =
        sqlx::query_as("SELECT id, path, size, mtime FROM media_files WHERE library_id = ?")
            .bind(library_id)
            .fetch_all(&state.db)
            .await?;
    let mut existing: HashMap<String, (String, i64, i64)> =
        existing.into_iter().map(|(id, path, size, mtime)| (path, (id, size, mtime))).collect();

    let seen: HashSet<String> = found.iter().map(|f| f.path.display().to_string()).collect();
    let moved = relink_moved_files(&state.db, &found, &seen, &mut existing).await?;
    if moved > 0 {
        tracing::info!(library = library_id, files = moved, "kept the records of moved files");
    }
    let changed: Vec<Found> = found
        .into_iter()
        .filter(|f| match existing.get(&f.path.display().to_string()) {
            Some((_, size, mtime)) => *size != f.size || *mtime != f.mtime,
            None => true,
        })
        .collect();

    let total = changed.len() as u32;
    state
        .scans
        .update(library_id, |s| {
            s.phase = "reading files".into();
            s.total = total;
        })
        .await;

    // ffprobe spends most of its time waiting on the disk, so run several at once.
    let scan_started = std::time::Instant::now();
    let sem = Arc::new(Semaphore::new(state.config.scan_concurrency));
    let mut tasks = tokio::task::JoinSet::new();
    for f in changed {
        let sem = sem.clone();
        let ffprobe = state.config.ffprobe.clone();
        tasks.spawn(async move {
            let _permit = sem.acquire_owned().await;
            let info = crate::probe::probe(&ffprobe, &f.path).await;
            (f, info)
        });
    }

    let mut added = 0u32;
    while let Some(joined) = tasks.join_next().await {
        let Ok((f, info)) = joined else { continue };
        match info {
            Ok(info) => {
                let is_new = !existing.contains_key(&f.path.display().to_string());
                match upsert_file(&state.db, library_id, &lib.kind, &f, &info).await {
                    Ok(Some((item_id, title))) if is_new => {
                        added += 1;
                        state.emit(ServerEvent::ItemAdded { item_id, title });
                    }
                    Ok(_) => {}
                    Err(e) => {
                        tracing::warn!(file = %f.path.display(), error = ?e, "could not save file");
                        state.scans.update(library_id, |s| s.errors += 1).await;
                    }
                }
            }
            Err(e) => {
                tracing::warn!(file = %f.path.display(), error = %e, "could not read file");
                state
                    .scans
                    .update(library_id, |s| {
                        s.errors += 1;
                        s.last_error = Some(format!("{}: {e}", f.path.display()));
                    })
                    .await;
            }
        }
        state.scans.update(library_id, |s| s.processed += 1).await;
    }

    // Remove files that disappeared, then items left without files.
    let mut removed = 0u32;
    for (path, (id, _, _)) in &existing {
        if !seen.contains(path) {
            sqlx::query("DELETE FROM media_files WHERE id = ?").bind(id).execute(&state.db).await?;
            removed += 1;
        }
    }
    remove_orphans(&state.db, library_id).await?;

    tracing::info!(
        library = library_id,
        files = total,
        seconds = scan_started.elapsed().as_secs(),
        "finished reading files"
    );
    let metadata_started = std::time::Instant::now();
    if let Err(e) = fetch_metadata(state, library_id).await {
        tracing::warn!(error = %e, "metadata fetch failed");
        state.scans.update(library_id, |s| s.last_error = Some(format!("metadata: {e}"))).await;
    }
    tracing::info!(library = library_id, seconds = metadata_started.elapsed().as_secs(), "finished fetching metadata");

    state
        .scans
        .update(library_id, |s| {
            s.added = added;
            s.removed = removed;
        })
        .await;
    Ok((added, removed))
}

/// Find files that moved: a known file that is gone, and a new file with the
/// same file name and size. Only one-to-one matches count, so two copies of
/// `Episode 1.mkv` are never mixed up. The file's record (and with it the
/// item, metadata and watch progress) gets the new path, and `existing` is
/// updated so the rest of the scan treats it as a known, unchanged file.
async fn relink_moved_files(
    db: &SqlitePool,
    found: &[Found],
    seen: &HashSet<String>,
    existing: &mut HashMap<String, (String, i64, i64)>,
) -> anyhow::Result<u32> {
    type Key = (String, i64);
    fn key(path: &str, size: i64) -> Option<Key> {
        Path::new(path).file_name().map(|n| (n.to_string_lossy().to_string(), size))
    }

    let mut gone: HashMap<Key, Vec<String>> = HashMap::new();
    for (path, (_, size, _)) in existing.iter() {
        if !seen.contains(path) {
            if let Some(k) = key(path, *size) {
                gone.entry(k).or_default().push(path.clone());
            }
        }
    }
    if gone.is_empty() {
        return Ok(0);
    }
    let mut arrived: HashMap<Key, Vec<&Found>> = HashMap::new();
    for f in found {
        let path = f.path.display().to_string();
        if !existing.contains_key(&path) {
            if let Some(k) = key(&path, f.size) {
                arrived.entry(k).or_default().push(f);
            }
        }
    }

    let mut moved = 0;
    for (k, old_paths) in gone {
        let (Some(new_files), [old_path]) = (arrived.get(&k), old_paths.as_slice()) else { continue };
        let [new_file] = new_files.as_slice() else { continue };
        let new_path = new_file.path.display().to_string();
        let Some((file_id, _, _)) = existing.remove(old_path) else { continue };

        let mut tx = db.begin().await?;
        sqlx::query("UPDATE media_files SET path = ?, mtime = ?, scanned_at = ? WHERE id = ?")
            .bind(&new_path)
            .bind(new_file.mtime)
            .bind(now_ms())
            .bind(&file_id)
            .execute(&mut *tx)
            .await?;
        // A movie's match key is its path; episodes are matched by show,
        // season and number, so theirs don't change.
        sqlx::query("UPDATE items SET match_key = ? WHERE match_key = ? AND id = (SELECT item_id FROM media_files WHERE id = ?)")
            .bind(format!("movie:{new_path}"))
            .bind(format!("movie:{old_path}"))
            .bind(&file_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;

        existing.insert(new_path, (file_id, new_file.size, new_file.mtime));
        moved += 1;
    }
    Ok(moved)
}

/// Sort titles ignore a leading "The", "A" or "An".
pub fn sort_title(title: &str) -> String {
    let lower = title.to_lowercase();
    for article in ["the ", "a ", "an "] {
        if let Some(rest) = lower.strip_prefix(article) {
            return rest.to_string();
        }
    }
    lower
}

/// Find or create an item by its match key. Returns (id, created).
#[allow(clippy::too_many_arguments)]
async fn ensure_item(
    db: &SqlitePool,
    library_id: &str,
    kind: &str,
    match_key: &str,
    parent_id: Option<&str>,
    series_id: Option<&str>,
    title: &str,
    year: Option<u16>,
    season: Option<u32>,
    episode: Option<u32>,
    episode_end: Option<u32>,
) -> sqlx::Result<(String, bool)> {
    if let Some(id) = sqlx::query_scalar::<_, String>("SELECT id FROM items WHERE library_id = ? AND match_key = ?")
        .bind(library_id)
        .bind(match_key)
        .fetch_optional(db)
        .await?
    {
        return Ok((id, false));
    }
    let id = new_id();
    let now = now_ms();
    sqlx::query(
        "INSERT INTO items (id, library_id, kind, parent_id, series_id, match_key, title, sort_title, year,
                            season_number, episode_number, episode_end, added_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(library_id)
    .bind(kind)
    .bind(parent_id)
    .bind(series_id)
    .bind(match_key)
    .bind(title)
    .bind(sort_title(title))
    .bind(year.map(i64::from))
    .bind(season.map(i64::from))
    .bind(episode.map(i64::from))
    .bind(episode_end.map(i64::from))
    .bind(now)
    .bind(now)
    .execute(db)
    .await?;
    Ok((id, true))
}

/// Save one probed file and the items it belongs to. Returns the top-level
/// item that was created (for "item added" events), if any.
async fn upsert_file(
    db: &SqlitePool,
    library_id: &str,
    library_kind: &str,
    f: &Found,
    info: &MediaInfo,
) -> anyhow::Result<Option<(String, String)>> {
    let rel = f.path.strip_prefix(&f.root).unwrap_or(&f.path);
    let mut created: Option<(String, String)> = None;

    let item_id = match (library_kind, parse_episode(rel)) {
        ("tv", Some(ep)) => {
            let show = if ep.show.is_empty() { "Unknown show".to_string() } else { ep.show.clone() };
            let (series_id, new_series) = ensure_item(
                db,
                library_id,
                "series",
                &format!("series:{}", show.to_lowercase()),
                None,
                None,
                &show,
                ep.show_year,
                None,
                None,
                None,
            )
            .await?;
            if new_series {
                created = Some((series_id.clone(), show.clone()));
            }
            let season_title = if ep.season == 0 { "Specials".to_string() } else { format!("Season {}", ep.season) };
            let (season_id, _) = ensure_item(
                db,
                library_id,
                "season",
                &format!("season:{series_id}:{}", ep.season),
                Some(&series_id),
                Some(&series_id),
                &season_title,
                None,
                Some(ep.season),
                None,
                None,
            )
            .await?;
            let ep_title = ep.title.clone().unwrap_or_else(|| format!("Episode {}", ep.episode));
            let (episode_id, _) = ensure_item(
                db,
                library_id,
                "episode",
                &format!("episode:{series_id}:{}:{}", ep.season, ep.episode),
                Some(&season_id),
                Some(&series_id),
                &ep_title,
                None,
                Some(ep.season),
                Some(ep.episode),
                ep.episode_end,
            )
            .await?;
            episode_id
        }
        _ => {
            let m = parse_movie(rel);
            let title = if m.title.is_empty() { file_title(&f.path) } else { m.title };
            let (id, new) = ensure_item(
                db,
                library_id,
                "movie",
                &format!("movie:{}", f.path.display()),
                None,
                None,
                &title,
                m.year,
                None,
                None,
                None,
            )
            .await?;
            if new {
                created = Some((id.clone(), title));
            }
            id
        }
    };

    sqlx::query(
        "INSERT INTO media_files (id, item_id, library_id, path, size, mtime, container, duration_ms, info, scanned_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(path) DO UPDATE SET item_id = excluded.item_id, size = excluded.size, mtime = excluded.mtime,
           container = excluded.container, duration_ms = excluded.duration_ms, info = excluded.info,
           scanned_at = excluded.scanned_at",
    )
    .bind(new_id())
    .bind(&item_id)
    .bind(library_id)
    .bind(f.path.display().to_string())
    .bind(f.size)
    .bind(f.mtime)
    .bind(&info.container)
    .bind(info.duration_ms as i64)
    .bind(serde_json::to_string(info)?)
    .bind(now_ms())
    .execute(db)
    .await?;

    Ok(created)
}

fn file_title(path: &Path) -> String {
    path.file_stem().and_then(|s| s.to_str()).unwrap_or("Untitled").to_string()
}

async fn remove_orphans(db: &SqlitePool, library_id: &str) -> sqlx::Result<()> {
    sqlx::query(
        "DELETE FROM items WHERE library_id = ? AND kind IN ('movie', 'episode')
         AND NOT EXISTS (SELECT 1 FROM media_files f WHERE f.item_id = items.id)",
    )
    .bind(library_id)
    .execute(db)
    .await?;
    for kind in ["season", "series"] {
        sqlx::query(
            "DELETE FROM items WHERE library_id = ? AND kind = ?
             AND NOT EXISTS (SELECT 1 FROM items c WHERE c.parent_id = items.id)",
        )
        .bind(library_id)
        .bind(kind)
        .execute(db)
        .await?;
    }
    Ok(())
}

/// Where a provider's id for an item is kept in `items.provider_ids` (module ids need no escaping).
fn provider_path(provider: &str) -> String {
    format!("$.\"{provider}\"")
}

async fn apply_metadata(
    db: &SqlitePool,
    provider: &str,
    item_id: &str,
    m: &Metadata,
    keep_title: bool,
) -> sqlx::Result<()> {
    let title = if keep_title { None } else { m.title.clone() };
    sqlx::query(
        "UPDATE items SET
            title = COALESCE(?, title), sort_title = COALESCE(?, sort_title),
            original_title = COALESCE(?, original_title), year = COALESCE(?, year),
            overview = COALESCE(?, overview), tagline = COALESCE(?, tagline), rating = COALESCE(?, rating),
            genres = ?, runtime_minutes = COALESCE(?, runtime_minutes), release_date = COALESCE(?, release_date),
            poster_url = COALESCE(?, poster_url), backdrop_url = COALESCE(?, backdrop_url),
            thumb_url = COALESCE(?, thumb_url), logo_url = COALESCE(?, logo_url),
            provider_ids = json_set(provider_ids, ?, ?),
            metadata_status = 'matched', updated_at = ?
         WHERE id = ?",
    )
    .bind(&title)
    .bind(title.as_deref().map(sort_title))
    .bind(&m.original_title)
    .bind(m.year.map(i64::from))
    .bind(&m.overview)
    .bind(&m.tagline)
    .bind(m.rating.map(f64::from))
    .bind(serde_json::to_string(&m.genres).unwrap_or_else(|_| "[]".into()))
    .bind(m.runtime_minutes.map(i64::from))
    .bind(&m.release_date)
    .bind(&m.poster_url)
    .bind(&m.backdrop_url)
    .bind(&m.thumb_url)
    .bind(&m.logo_url)
    .bind(provider_path(provider))
    .bind(&m.provider_id)
    .bind(now_ms())
    .bind(item_id)
    .execute(db)
    .await?;
    Ok(())
}

async fn mark_unmatched(db: &SqlitePool, item_id: &str) -> sqlx::Result<()> {
    sqlx::query("UPDATE items SET metadata_status = 'unmatched' WHERE id = ?").bind(item_id).execute(db).await?;
    Ok(())
}

#[derive(sqlx::FromRow)]
struct PendingItem {
    id: String,
    kind: String,
    title: String,
    year: Option<i64>,
    season_number: Option<i64>,
    episode_number: Option<i64>,
    /// The provider's id for the episode's series.
    series_provider_id: Option<String>,
}

/// Look up one item. Returns `Ok(None)` when the provider has no match.
async fn lookup(provider: &dyn MetadataProvider, item: &PendingItem) -> anyhow::Result<Option<Metadata>> {
    Ok(match item.kind.as_str() {
        "movie" | "series" => {
            let kind = if item.kind == "movie" { MediaKind::Movie } else { MediaKind::Series };
            let year = item.year.map(|y| y as u16);
            match provider.search(kind, &item.title, year).await? {
                Some(hit) => provider.details(kind, &hit.provider_id).await?.or(Some(hit)),
                None => None,
            }
        }
        _ => match (&item.series_provider_id, item.season_number, item.episode_number) {
            (Some(series), Some(s), Some(e)) => provider.episode(series, s as u32, e as u32).await?,
            _ => None,
        },
    })
}

/// Fill in metadata for items that don't have it yet, using the installed
/// metadata provider module (TMDB, or any other `metadata_provider`).
/// Lookups run several at a time and progress is shown in the library list.
pub async fn fetch_metadata(state: &AppState, library_id: &str) -> anyhow::Result<()> {
    let Some(provider) = modules::metadata::provider(state).await else {
        return Ok(());
    };

    // Movies and series first, so episodes can use their series' id.
    for (phase, kinds) in [("fetching titles", vec!["movie", "series"]), ("fetching episodes", vec!["episode"])] {
        let placeholders = kinds.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            "SELECT i.id, i.kind, i.title, i.year, i.season_number, i.episode_number,
                    json_extract(s.provider_ids, ?) AS series_provider_id
             FROM items i LEFT JOIN items s ON s.id = i.series_id
             WHERE i.library_id = ? AND i.metadata_status = 'pending' AND i.kind IN ({placeholders})"
        );
        let mut q = sqlx::query_as::<_, PendingItem>(&sql).bind(provider_path(provider.id())).bind(library_id);
        for k in &kinds {
            q = q.bind(*k);
        }
        let pending = q.fetch_all(&state.db).await?;
        if pending.is_empty() {
            continue;
        }

        let total = pending.len() as u32;
        state
            .scans
            .update(library_id, |s| {
                s.phase = phase.into();
                s.processed = 0;
                s.total = total;
            })
            .await;

        let sem = Arc::new(Semaphore::new(state.config.scan_concurrency));
        let mut tasks = tokio::task::JoinSet::new();
        for item in pending {
            let (sem, provider) = (sem.clone(), provider.clone());
            tasks.spawn(async move {
                let _permit = sem.acquire_owned().await;
                let result = lookup(provider.as_ref(), &item).await;
                (item, result)
            });
        }

        while let Some(joined) = tasks.join_next().await {
            let Ok((item, result)) = joined else { continue };
            match result {
                Ok(Some(m)) => apply_metadata(&state.db, provider.id(), &item.id, &m, false).await?,
                Ok(None) => mark_unmatched(&state.db, &item.id).await?,
                Err(e) if e.to_string().contains("rejected the API key") => {
                    tasks.abort_all();
                    return Err(e);
                }
                Err(e) => {
                    // Network hiccup or rate limit: leave it pending so the next scan retries.
                    tracing::warn!(title = %item.title, error = %e, "metadata lookup failed");
                    state.scans.update(library_id, |s| s.errors += 1).await;
                }
            }
            state.scans.update(library_id, |s| s.processed += 1).await;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sort_titles() {
        assert_eq!(sort_title("The Matrix"), "matrix");
        assert_eq!(sort_title("A Quiet Place"), "quiet place");
        assert_eq!(sort_title("Theodore"), "theodore");
    }

    fn info() -> MediaInfo {
        MediaInfo {
            container: "matroska".into(),
            duration_ms: 1000,
            bitrate: None,
            video: None,
            audio: vec![],
            subtitles: vec![],
        }
    }

    async fn library(db: &SqlitePool, kind: &str) -> String {
        let id = new_id();
        sqlx::query("INSERT INTO libraries (id, name, kind, paths, created_at) VALUES (?, 'L', ?, '[]', 0)")
            .bind(&id)
            .bind(kind)
            .execute(db)
            .await
            .unwrap();
        id
    }

    fn found(root: &str, rel: &str) -> Found {
        Found { path: PathBuf::from(root).join(rel), root: PathBuf::from(root), size: 1, mtime: 1 }
    }

    #[tokio::test]
    async fn builds_tv_tree_and_cleans_up() {
        let db = crate::db::connect_memory().await;
        let lib = library(&db, "tv").await;
        for rel in
            ["Show/Season 01/Show - S01E01.mkv", "Show/Season 01/Show - S01E02.mkv", "Show/Season 02/Show - S02E01.mkv"]
        {
            upsert_file(&db, &lib, "tv", &found("/tv", rel), &info()).await.unwrap();
        }
        let count = |kind: &'static str| {
            let db = db.clone();
            async move {
                sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM items WHERE kind = ?")
                    .bind(kind)
                    .fetch_one(&db)
                    .await
                    .unwrap()
            }
        };
        assert_eq!((count("series").await, count("season").await, count("episode").await), (1, 2, 3));

        // Season 2's only file disappears.
        sqlx::query("DELETE FROM media_files WHERE path LIKE '%S02E01%'").execute(&db).await.unwrap();
        remove_orphans(&db, &lib).await.unwrap();
        assert_eq!((count("series").await, count("season").await, count("episode").await), (1, 1, 2));
    }

    #[tokio::test]
    async fn rescanning_a_file_does_not_duplicate() {
        let db = crate::db::connect_memory().await;
        let lib = library(&db, "movies").await;
        let f = found("/m", "Inception (2010)/Inception (2010).mkv");
        let first = upsert_file(&db, &lib, "movies", &f, &info()).await.unwrap();
        let second = upsert_file(&db, &lib, "movies", &f, &info()).await.unwrap();
        assert!(first.is_some());
        assert!(second.is_none());
        let (title, year): (String, Option<i64>) =
            sqlx::query_as("SELECT title, year FROM items").fetch_one(&db).await.unwrap();
        assert_eq!((title.as_str(), year), ("Inception", Some(2010)));
        let files: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM media_files").fetch_one(&db).await.unwrap();
        assert_eq!(files, 1);
    }

    async fn known_files(db: &SqlitePool, lib: &str) -> HashMap<String, (String, i64, i64)> {
        sqlx::query_as::<_, (String, String, i64, i64)>(
            "SELECT id, path, size, mtime FROM media_files WHERE library_id = ?",
        )
        .bind(lib)
        .fetch_all(db)
        .await
        .unwrap()
        .into_iter()
        .map(|(id, path, size, mtime)| (path, (id, size, mtime)))
        .collect()
    }

    async fn add_user_with_progress(db: &SqlitePool, item_id: &str) {
        sqlx::query(
            "INSERT INTO users (id, username, password_hash, role, created_at) VALUES ('u', 'u', 'x', 'owner', 0)",
        )
        .execute(db)
        .await
        .unwrap();
        sqlx::query("INSERT INTO user_item_data (user_id, item_id, position_ms) VALUES ('u', ?, 60000)")
            .bind(item_id)
            .execute(db)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn a_moved_movie_keeps_its_item_and_progress() {
        let db = crate::db::connect_memory().await;
        let lib = library(&db, "movies").await;
        let old = found("/old", "Inception (2010)/Inception (2010).mkv");
        upsert_file(&db, &lib, "movies", &old, &info()).await.unwrap();
        let item_id: String = sqlx::query_scalar("SELECT id FROM items").fetch_one(&db).await.unwrap();
        add_user_with_progress(&db, &item_id).await;

        // The library's folder changed from /old to /new.
        let now = vec![found("/new", "Inception (2010)/Inception (2010).mkv")];
        let seen: HashSet<String> = now.iter().map(|f| f.path.display().to_string()).collect();
        let mut existing = known_files(&db, &lib).await;
        let moved = relink_moved_files(&db, &now, &seen, &mut existing).await.unwrap();
        assert_eq!(moved, 1);
        assert!(existing.contains_key("/new/Inception (2010)/Inception (2010).mkv"));
        assert!(!existing.contains_key("/old/Inception (2010)/Inception (2010).mkv"));

        // Same file record, same item, new match key; nothing left to clean up.
        let (path, file_item): (String, String) =
            sqlx::query_as("SELECT path, item_id FROM media_files").fetch_one(&db).await.unwrap();
        assert_eq!(path, "/new/Inception (2010)/Inception (2010).mkv");
        assert_eq!(file_item, item_id);
        let key: String = sqlx::query_scalar("SELECT match_key FROM items").fetch_one(&db).await.unwrap();
        assert_eq!(key, "movie:/new/Inception (2010)/Inception (2010).mkv");
        remove_orphans(&db, &lib).await.unwrap();
        let position: i64 = sqlx::query_scalar("SELECT position_ms FROM user_item_data").fetch_one(&db).await.unwrap();
        assert_eq!(position, 60000);

        // A rescan of the new folder now finds the known file again.
        let second = upsert_file(&db, &lib, "movies", &now[0], &info()).await.unwrap();
        assert!(second.is_none(), "no duplicate item");
    }

    #[tokio::test]
    async fn moves_need_a_unique_match() {
        let db = crate::db::connect_memory().await;
        let lib = library(&db, "tv").await;
        for root in ["/a", "/b"] {
            upsert_file(&db, &lib, "tv", &found(root, "Show/Show - S01E01.mkv"), &info()).await.unwrap();
        }
        // Both copies vanish and one file with the same name and size appears:
        // which one moved is a guess, so neither is relinked.
        let now = vec![found("/c", "Show/Show - S01E01.mkv")];
        let seen: HashSet<String> = now.iter().map(|f| f.path.display().to_string()).collect();
        let mut existing = known_files(&db, &lib).await;
        assert_eq!(relink_moved_files(&db, &now, &seen, &mut existing).await.unwrap(), 0);

        // A different size is a different file.
        let mut bigger = found("/c", "Show/Show - S01E02.mkv");
        bigger.size = 2;
        upsert_file(&db, &lib, "tv", &found("/a", "Show/Show - S01E02.mkv"), &info()).await.unwrap();
        let now = vec![bigger];
        let seen: HashSet<String> = now.iter().map(|f| f.path.display().to_string()).collect();
        let mut existing = known_files(&db, &lib).await;
        assert_eq!(relink_moved_files(&db, &now, &seen, &mut existing).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn a_moved_episode_keeps_its_file_record() {
        let db = crate::db::connect_memory().await;
        let lib = library(&db, "tv").await;
        upsert_file(&db, &lib, "tv", &found("/tv", "Show/Season 01/Show - S01E01.mkv"), &info()).await.unwrap();
        let file_id: String = sqlx::query_scalar("SELECT id FROM media_files").fetch_one(&db).await.unwrap();

        let now = vec![found("/anime", "Show/Season 01/Show - S01E01.mkv")];
        let seen: HashSet<String> = now.iter().map(|f| f.path.display().to_string()).collect();
        let mut existing = known_files(&db, &lib).await;
        assert_eq!(relink_moved_files(&db, &now, &seen, &mut existing).await.unwrap(), 1);
        let (id, path): (String, String) =
            sqlx::query_as("SELECT id, path FROM media_files").fetch_one(&db).await.unwrap();
        assert_eq!((id, path.as_str()), (file_id, "/anime/Show/Season 01/Show - S01E01.mkv"));
        let key: String =
            sqlx::query_scalar("SELECT match_key FROM items WHERE kind = 'episode'").fetch_one(&db).await.unwrap();
        assert!(key.starts_with("episode:"), "episode keys don't hold paths: {key}");
    }
}
