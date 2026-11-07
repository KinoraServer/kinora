//! TMDB metadata provider (built-in module "tmdb").
//!
//! Accepts either a v3 API key or a v4 read access token (starts with "eyJ").
//! Attribution required by TMDB is shown in the web app's settings page.

use std::collections::HashMap;

use async_trait::async_trait;
use kinora_plugin_api::{
    ExtensionPoint, MediaKind, Metadata, MetadataProvider, ModuleInfo, SettingField, SettingKind, Settings,
};
use serde_json::Value;
use tokio::sync::Mutex;

const API: &str = "https://api.themoviedb.org/3";
const IMAGES: &str = "https://image.tmdb.org/t/p";

pub fn info() -> ModuleInfo {
    ModuleInfo {
        id: "tmdb".into(),
        name: "TMDB metadata".into(),
        description: "Titles, descriptions, artwork and ratings for movies and TV from The Movie Database.".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        provides: vec![ExtensionPoint::MetadataProvider],
        enabled_by_default: true,
        settings: vec![
            SettingField {
                key: "api_key".into(),
                label: "API key or read access token".into(),
                kind: SettingKind::Secret,
                help: Some("Free at themoviedb.org → Settings → API.".into()),
                default: None,
            },
            SettingField {
                key: "language".into(),
                label: "Metadata language".into(),
                kind: SettingKind::Text,
                help: Some("For example en-US, de-DE, ja-JP.".into()),
                default: None,
            },
        ],
    }
}

pub struct Tmdb {
    http: reqwest::Client,
    key: String,
    language: String,
    season_cache: Mutex<HashMap<(String, u32), Value>>,
}

impl Tmdb {
    /// Returns `None` when no API key is configured.
    pub fn from_settings(http: reqwest::Client, settings: &Settings) -> Option<Self> {
        let key = settings.get("api_key").map(|k| k.trim().to_string()).filter(|k| !k.is_empty())?;
        let language = settings
            .get("language")
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .unwrap_or_else(|| "en-US".into());
        Some(Tmdb { http, key, language, season_cache: Mutex::new(HashMap::new()) })
    }

    async fn get(&self, path: &str, query: &[(&str, String)]) -> anyhow::Result<Option<Value>> {
        let mut req = self.http.get(format!("{API}{path}"));
        let mut params: Vec<(&str, String)> = vec![("language", self.language.clone())];
        params.extend_from_slice(query);
        if self.key.starts_with("eyJ") {
            req = req.bearer_auth(&self.key);
        } else {
            params.push(("api_key", self.key.clone()));
        }
        let resp = req.query(&params).send().await?;
        match resp.status() {
            s if s == reqwest::StatusCode::NOT_FOUND => Ok(None),
            s if s == reqwest::StatusCode::UNAUTHORIZED => anyhow::bail!("TMDB rejected the API key"),
            s if !s.is_success() => anyhow::bail!("TMDB returned {s}"),
            _ => Ok(Some(resp.json().await?)),
        }
    }
}

fn image(path: Option<&str>, size: &str) -> Option<String> {
    path.filter(|p| !p.is_empty()).map(|p| format!("{IMAGES}/{size}{p}"))
}

fn s(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).filter(|x| !x.is_empty()).map(str::to_string)
}

fn year_of(date: Option<&str>) -> Option<u16> {
    date.and_then(|d| d.get(0..4)).and_then(|y| y.parse().ok())
}

/// Convert a TMDB movie/tv/episode JSON object into our metadata shape.
pub fn to_metadata(v: &Value, kind: Option<MediaKind>) -> Metadata {
    let id = v.get("id").map(|i| i.to_string()).unwrap_or_default();
    let date = s(v, "release_date").or_else(|| s(v, "first_air_date")).or_else(|| s(v, "air_date"));
    let title = s(v, "title").or_else(|| s(v, "name"));
    let original = s(v, "original_title").or_else(|| s(v, "original_name"));
    let runtime = v
        .get("runtime")
        .and_then(Value::as_u64)
        .or_else(|| v.get("episode_run_time").and_then(Value::as_array).and_then(|a| a.first()).and_then(Value::as_u64))
        .map(|n| n as u32)
        .filter(|n| *n > 0);
    let genres = v
        .get("genres")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|g| s(g, "name")).collect())
        .unwrap_or_default();
    let is_episode = kind.is_none();
    Metadata {
        provider_id: id,
        title,
        original_title: original,
        year: year_of(date.as_deref()),
        overview: s(v, "overview"),
        tagline: s(v, "tagline"),
        rating: v.get("vote_average").and_then(Value::as_f64).map(|r| r as f32).filter(|r| *r > 0.0),
        official_rating: None,
        genres,
        runtime_minutes: runtime,
        poster_url: if is_episode { None } else { image(v.get("poster_path").and_then(Value::as_str), "w500") },
        backdrop_url: image(v.get("backdrop_path").and_then(Value::as_str), "w1280"),
        thumb_url: image(v.get("still_path").and_then(Value::as_str), "w780"),
        release_date: date,
    }
}

#[async_trait]
impl MetadataProvider for Tmdb {
    fn id(&self) -> &str {
        "tmdb"
    }

    async fn search(&self, kind: MediaKind, title: &str, year: Option<u16>) -> anyhow::Result<Option<Metadata>> {
        let (path, year_param) = match kind {
            MediaKind::Movie => ("/search/movie", "year"),
            MediaKind::Series => ("/search/tv", "first_air_date_year"),
        };
        let mut q = vec![("query", title.to_string()), ("include_adult", "false".to_string())];
        if let Some(y) = year {
            q.push((year_param, y.to_string()));
        }
        let mut results = self.get(path, &q).await?;
        // Retry without the year: release years in file names are often off by one.
        let empty = |r: &Option<Value>| {
            r.as_ref().and_then(|v| v.get("results")).and_then(Value::as_array).map(|a| a.is_empty()).unwrap_or(true)
        };
        if year.is_some() && empty(&results) {
            q.pop();
            results = self.get(path, &q).await?;
        }
        Ok(results
            .as_ref()
            .and_then(|v| v.get("results"))
            .and_then(Value::as_array)
            .and_then(|a| a.first())
            .map(|first| to_metadata(first, Some(kind))))
    }

    async fn details(&self, kind: MediaKind, provider_id: &str) -> anyhow::Result<Option<Metadata>> {
        let path = match kind {
            MediaKind::Movie => format!("/movie/{provider_id}"),
            MediaKind::Series => format!("/tv/{provider_id}"),
        };
        Ok(self.get(&path, &[]).await?.map(|v| to_metadata(&v, Some(kind))))
    }

    async fn episode(&self, series_id: &str, season: u32, episode: u32) -> anyhow::Result<Option<Metadata>> {
        let key = (series_id.to_string(), season);
        let season_json = {
            let cache = self.season_cache.lock().await;
            cache.get(&key).cloned()
        };
        let season_json = match season_json {
            Some(v) => v,
            None => {
                let Some(v) = self.get(&format!("/tv/{series_id}/season/{season}"), &[]).await? else {
                    return Ok(None);
                };
                self.season_cache.lock().await.insert(key, v.clone());
                v
            }
        };
        Ok(season_json
            .get("episodes")
            .and_then(Value::as_array)
            .and_then(|eps| {
                eps.iter().find(|e| e.get("episode_number").and_then(Value::as_u64) == Some(u64::from(episode)))
            })
            .map(|e| to_metadata(e, None)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn movie_json_to_metadata() {
        let v = json!({
            "id": 27205, "title": "Inception", "original_title": "Inception",
            "release_date": "2010-07-15", "overview": "A thief...", "vote_average": 8.4,
            "runtime": 148, "genres": [{"id": 28, "name": "Action"}],
            "poster_path": "/p.jpg", "backdrop_path": "/b.jpg"
        });
        let m = to_metadata(&v, Some(MediaKind::Movie));
        assert_eq!(m.provider_id, "27205");
        assert_eq!(m.year, Some(2010));
        assert_eq!(m.runtime_minutes, Some(148));
        assert_eq!(m.genres, vec!["Action"]);
        assert_eq!(m.poster_url.as_deref(), Some("https://image.tmdb.org/t/p/w500/p.jpg"));
    }

    #[test]
    fn episode_json_uses_still_not_poster() {
        let v =
            json!({"id": 1, "name": "Pilot", "air_date": "2008-01-20", "still_path": "/s.jpg", "episode_number": 1});
        let m = to_metadata(&v, None);
        assert_eq!(m.title.as_deref(), Some("Pilot"));
        assert_eq!(m.thumb_url.as_deref(), Some("https://image.tmdb.org/t/p/w780/s.jpg"));
        assert_eq!(m.poster_url, None);
    }

    #[test]
    fn no_key_no_provider() {
        let http = reqwest::Client::new();
        assert!(Tmdb::from_settings(http.clone(), &Settings::new()).is_none());
        let mut s = Settings::new();
        s.insert("api_key".into(), "  ".into());
        assert!(Tmdb::from_settings(http, &s).is_none());
    }
}
