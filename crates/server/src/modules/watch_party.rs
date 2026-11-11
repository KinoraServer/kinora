//! Watch parties (module "watch-party"), off by default.
//!
//! Viewers watch together through Open Watch Party (https://openwatch.party):
//! a relay keeps everyone's player in sync, and each viewer plays their own
//! copy from their own server. The browser talks to the relay itself, with
//! the `openwatchparty` SDK. The server only says whether the module is on and
//! which relay to use, and answers the SDK's resolver: "which of my items is
//! the title the room is watching?" (docs/watch-party.md, decision D23).
//!
//! Everything the resolver receives came from another participant through the
//! relay, so it is untrusted. It is only ever used as bound SQL values.

use axum::extract::{Path, Query, State};
use axum::Json;
use kinora_plugin_api::{ExtensionPoint, ModuleInfo, SettingField, SettingKind};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::collections::BTreeMap;

use crate::auth::AuthUser;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

pub const ID: &str = "watch-party";
pub const RELAY_URL: &str = "relay_url";

/// The catalogue namespaces Open Watch Party v1 knows. Kinora stores `tmdb` today.
const NAMESPACES: &[&str] = &["imdb", "tmdb", "tvdb", "plex", "anilist", "anidb", "kitsu", "mal"];
/// The most rows one resolver call returns.
const MAX_ROWS: i64 = 100;

pub fn info() -> ModuleInfo {
    ModuleInfo {
        id: ID.into(),
        name: "Watch parties".into(),
        description: "Watch together in sync with Open Watch Party. One person starts a party and shares a code; \
                      everyone who joins plays their own copy, and play, pause and seeking stay in step for all."
            .into(),
        version: env!("CARGO_PKG_VERSION").into(),
        provides: vec![ExtensionPoint::WatchParty],
        enabled_by_default: false,
        settings: vec![SettingField {
            key: RELAY_URL.into(),
            label: "Your own relay (optional)".into(),
            kind: SettingKind::Text,
            help: Some(
                "Leave empty to use the public Open Watch Party relay. To keep parties fully private, run your own \
                 relay and enter its base URL, e.g. https://relay.example.com."
                    .into(),
            ),
            default: None,
        }],
    }
}

/// Checks a relay base URL before it is saved. Empty means "use the public relay".
pub fn check_relay_url(url: &str) -> Result<(), String> {
    let url = url.trim();
    if url.is_empty() {
        return Ok(());
    }
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .ok_or("The relay URL must start with https:// (or http:// on your own network).")?;
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    if host.is_empty() || host.contains('@') || url.len() > 512 || url.chars().any(|c| c.is_whitespace()) {
        return Err("That doesn't look like a relay URL, e.g. https://relay.example.com.".into());
    }
    if url.contains(['?', '#']) {
        return Err("The relay URL can't have a query (?) or fragment (#).".into());
    }
    Ok(())
}

#[derive(Serialize)]
pub struct Config {
    enabled: bool,
    /// The admin's own relay; `None` means the public relays.
    relay_url: Option<String>,
    /// What Kinora calls itself to the relay: `Kinora/<version>`.
    client: String,
}

/// `GET /api/watch-party`: any signed-in viewer, so the web app knows whether to offer parties.
pub async fn config(State(state): State<AppState>, _user: AuthUser) -> Json<Config> {
    let enabled = super::is_enabled(&state.db, ID).await;
    let relay_url = if enabled {
        super::settings(&state.db, ID).await.get(RELAY_URL).map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
    } else {
        None
    };
    Json(Config { enabled, relay_url, client: format!("Kinora/{}", env!("CARGO_PKG_VERSION")) })
}

async fn require_enabled(db: &SqlitePool) -> AppResult<()> {
    if super::is_enabled(db, ID).await {
        Ok(())
    } else {
        Err(AppError::NotFound)
    }
}

/// One playable title (a movie or an episode) and its main file, as the resolver sees it.
#[derive(Debug, Serialize, PartialEq)]
pub struct PartyItem {
    id: String,
    kind: String,
    title: String,
    year: Option<i64>,
    ids: BTreeMap<String, String>,
    show: Option<PartyShow>,
    season: Option<i64>,
    episode: Option<i64>,
    duration_ms: Option<i64>,
    size_bytes: Option<i64>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct PartyShow {
    title: String,
    year: Option<i64>,
    ids: BTreeMap<String, String>,
}

#[derive(sqlx::FromRow)]
struct Row {
    id: String,
    kind: String,
    title: String,
    year: Option<i64>,
    provider_ids: String,
    season_number: Option<i64>,
    episode_number: Option<i64>,
    series_title: Option<String>,
    series_year: Option<i64>,
    series_ids: Option<String>,
    duration_ms: Option<i64>,
    size: Option<i64>,
}

/// Provider IDs as strings, keeping only the namespaces Open Watch Party knows.
fn ids_of(json: Option<&str>) -> BTreeMap<String, String> {
    let Some(value) = json.and_then(|j| serde_json::from_str::<serde_json::Value>(j).ok()) else {
        return BTreeMap::new();
    };
    let Some(map) = value.as_object() else { return BTreeMap::new() };
    map.iter()
        .filter(|(ns, _)| NAMESPACES.contains(&ns.as_str()))
        .filter_map(|(ns, v)| {
            let id = match v {
                serde_json::Value::String(s) => s.trim().to_string(),
                serde_json::Value::Number(n) => n.to_string(),
                _ => return None,
            };
            (!id.is_empty()).then(|| (ns.clone(), id))
        })
        .collect()
}

impl From<Row> for PartyItem {
    fn from(r: Row) -> Self {
        let show = (r.kind == "episode").then(|| PartyShow {
            title: r.series_title.clone().unwrap_or_default(),
            year: r.series_year,
            ids: ids_of(r.series_ids.as_deref()),
        });
        PartyItem {
            ids: ids_of(Some(&r.provider_ids)),
            id: r.id,
            kind: r.kind,
            title: r.title,
            year: r.year,
            show,
            season: r.season_number,
            episode: r.episode_number,
            duration_ms: r.duration_ms,
            size_bytes: r.size,
        }
    }
}

/// Movies and episodes with their largest file, which is the one the player opens.
const SELECT: &str = "
    SELECT i.id, i.kind, i.title, i.year, i.provider_ids, i.season_number, i.episode_number,
           s.title AS series_title, s.year AS series_year, s.provider_ids AS series_ids,
           f.duration_ms, f.size
    FROM items i
    LEFT JOIN items s ON s.id = i.series_id
    JOIN media_files f ON f.id = (SELECT id FROM media_files WHERE item_id = i.id ORDER BY size DESC LIMIT 1)
    WHERE i.kind IN ('movie', 'episode')";

/// `GET /api/watch-party/items/{id}`: the host's own item, to describe it to the room.
pub async fn item(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Json<PartyItem>> {
    require_enabled(&state.db).await?;
    let row: Option<Row> =
        sqlx::query_as(&format!("{SELECT} AND i.id = ?")).bind(&id).fetch_optional(&state.db).await?;
    row.map(|r| Json(r.into())).ok_or(AppError::NotFound)
}

#[derive(Deserialize)]
pub struct FindBody {
    /// The title's own IDs (a movie's, or an episode's).
    #[serde(default)]
    ids: BTreeMap<String, String>,
    /// An episode's series IDs.
    #[serde(default)]
    series_ids: BTreeMap<String, String>,
    /// With `series_ids`: only this season and episode.
    season: Option<i64>,
    episode: Option<i64>,
}

/// Valid (namespace, id) pairs from an untrusted map.
fn clean_ids(ids: &BTreeMap<String, String>) -> Vec<(String, String)> {
    ids.iter()
        .filter(|(ns, id)| NAMESPACES.contains(&ns.as_str()) && !id.trim().is_empty() && id.len() <= 128)
        .map(|(ns, id)| (format!("$.{ns}"), id.trim().to_string()))
        .collect()
}

/// `POST /api/watch-party/find`: the resolver's step 1, items sharing a catalogue ID with the room's
/// media (its own IDs), or episodes of a series sharing one (narrowed to the same season and episode).
pub async fn find(
    State(state): State<AppState>,
    _user: AuthUser,
    Json(body): Json<FindBody>,
) -> AppResult<Json<Vec<PartyItem>>> {
    require_enabled(&state.db).await?;
    let own = clean_ids(&body.ids);
    let series = clean_ids(&body.series_ids);
    if own.is_empty() && series.is_empty() {
        return Ok(Json(Vec::new()));
    }
    let mut clauses = Vec::new();
    let mut binds: Vec<String> = Vec::new();
    for (path, id) in &own {
        clauses.push("CAST(json_extract(i.provider_ids, ?) AS TEXT) = ?".to_string());
        binds.extend([path.clone(), id.clone()]);
    }
    for (path, id) in &series {
        let mut c = "(i.kind = 'episode' AND CAST(json_extract(s.provider_ids, ?) AS TEXT) = ?".to_string();
        binds.extend([path.clone(), id.clone()]);
        if let Some(season) = body.season {
            c.push_str(" AND i.season_number = ?");
            binds.push(season.to_string());
        }
        if let Some(episode) = body.episode {
            c.push_str(" AND i.episode_number = ?");
            binds.push(episode.to_string());
        }
        c.push(')');
        clauses.push(c);
    }
    let sql = format!("{SELECT} AND ({}) ORDER BY i.id LIMIT ?", clauses.join(" OR "));
    let mut q = sqlx::query_as::<_, Row>(&sql);
    for b in &binds {
        q = q.bind(b);
    }
    let rows = q.bind(MAX_ROWS).fetch_all(&state.db).await?;
    Ok(Json(rows.into_iter().map(Into::into).collect()))
}

#[derive(Deserialize)]
pub struct SearchQuery {
    q: String,
    /// "movie" or "episode".
    kind: String,
    /// For episodes: the room's season and episode come first.
    season: Option<i64>,
    episode: Option<i64>,
}

/// `GET /api/watch-party/search`: the resolver's step 2 and its near misses. Movies by their title,
/// episodes by their show's title, never their own.
pub async fn search(
    State(state): State<AppState>,
    _user: AuthUser,
    Query(q): Query<SearchQuery>,
) -> AppResult<Json<Vec<PartyItem>>> {
    require_enabled(&state.db).await?;
    let text = q.q.trim();
    if text.is_empty() || text.chars().count() > 200 {
        return Ok(Json(Vec::new()));
    }
    let like = format!("%{}%", text.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_"));
    let rows: Vec<Row> = match q.kind.as_str() {
        "movie" => {
            sqlx::query_as(&format!(
                "{SELECT} AND i.kind = 'movie' AND (i.title LIKE ? ESCAPE '\\' OR i.original_title LIKE ? ESCAPE '\\')
                 ORDER BY i.sort_title LIMIT ?"
            ))
            .bind(&like)
            .bind(&like)
            .bind(MAX_ROWS)
            .fetch_all(&state.db)
            .await?
        }
        "episode" => {
            sqlx::query_as(&format!(
                "{SELECT} AND i.kind = 'episode' AND (s.title LIKE ? ESCAPE '\\' OR s.original_title LIKE ? ESCAPE '\\')
                 ORDER BY (i.season_number IS ? AND i.episode_number IS ?) DESC, (i.season_number IS ?) DESC,
                          s.sort_title, i.season_number, i.episode_number
                 LIMIT ?"
            ))
            .bind(&like)
            .bind(&like)
            .bind(q.season)
            .bind(q.episode)
            .bind(q.season)
            .bind(MAX_ROWS)
            .fetch_all(&state.db)
            .await?
        }
        _ => return Err(AppError::BadRequest("kind must be movie or episode".into())),
    };
    Ok(Json(rows.into_iter().map(Into::into).collect()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relay_urls() {
        assert!(check_relay_url("").is_ok());
        assert!(check_relay_url("https://relay.example.com").is_ok());
        assert!(check_relay_url("http://192.168.1.5:8080/owp").is_ok());
        assert!(check_relay_url("relay.example.com").is_err());
        assert!(check_relay_url("wss://relay.example.com").is_err());
        assert!(check_relay_url("https://user@relay.example.com").is_err());
        assert!(check_relay_url("https://").is_err());
        assert!(check_relay_url("https://relay.example.com/?x=1").is_err());
    }

    #[test]
    fn ids_keep_known_namespaces_as_strings() {
        let ids = ids_of(Some(r#"{"tmdb": 1396, "imdb": "tt0903747", "custom": "x", "tvdb": ""}"#));
        assert_eq!(ids.len(), 2);
        assert_eq!(ids["tmdb"], "1396");
        assert_eq!(ids["imdb"], "tt0903747");
        assert!(ids_of(Some("not json")).is_empty());
    }
}
