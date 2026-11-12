//! Configuration from environment variables (documented in .env.example).

use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Config {
    pub bind: String,
    pub port: u16,
    /// Database, settings, themes and plugins live here. Back this folder up.
    pub config_dir: PathBuf,
    /// Image cache. Safe to delete.
    pub cache_dir: PathBuf,
    /// Temporary transcode segments. Put on fast storage or a RAM disk.
    pub transcode_dir: PathBuf,
    pub ffmpeg: String,
    pub ffprobe: String,
    /// Optional: seeds the TMDB module's API key on first start.
    pub tmdb_api_key: Option<String>,
    /// How many files are read with ffprobe at once during a scan, and how many
    /// TMDB lookups run at once. Slow or spinning disks like 4-8; SSDs can go higher.
    pub scan_concurrency: usize,
    /// Where Admin > Modules installs packages from: the folder that holds `index.json`.
    pub registry_url: String,
    /// Base64 Ed25519 public keys the registry's index must be signed with.
    /// Empty: the index is not checked for a signature (files are still checked against its hashes).
    pub registry_keys: Vec<String>,
}

/// The official registry (github.com/KinoraServer/kinora-registry, branch `published`).
pub const DEFAULT_REGISTRY_URL: &str = "https://raw.githubusercontent.com/KinoraServer/kinora-registry/published";
/// Keys the official registry signs its index with. More than one while a key is being replaced.
pub const DEFAULT_REGISTRY_KEYS: &[&str] = &[];

impl Config {
    pub fn from_env() -> Self {
        let var = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
        let config_dir = PathBuf::from(var("KINORA_CONFIG_DIR").unwrap_or_else(|| "./data".into()));
        let cache_dir = var("KINORA_CACHE_DIR").map(PathBuf::from).unwrap_or_else(|| config_dir.join("cache"));
        let transcode_dir =
            var("KINORA_TRANSCODE_DIR").map(PathBuf::from).unwrap_or_else(|| cache_dir.join("transcodes"));
        // Another registry brings its own key (or none); the built-in keys only fit the official one.
        let registry_url = var("KINORA_REGISTRY_URL").map(|u| u.trim().trim_end_matches('/').to_string());
        let registry_keys = match (var("KINORA_REGISTRY_KEY"), &registry_url) {
            (Some(keys), _) => keys.split(',').map(|k| k.trim().to_string()).filter(|k| !k.is_empty()).collect(),
            (None, Some(_)) => vec![],
            (None, None) => DEFAULT_REGISTRY_KEYS.iter().map(|k| k.to_string()).collect(),
        };
        Config {
            registry_url: registry_url.unwrap_or_else(|| DEFAULT_REGISTRY_URL.into()),
            registry_keys,
            bind: var("KINORA_BIND").unwrap_or_else(|| "0.0.0.0".into()),
            port: var("KINORA_PORT").and_then(|p| p.parse().ok()).unwrap_or(8420),
            config_dir,
            cache_dir,
            transcode_dir,
            ffmpeg: var("KINORA_FFMPEG").unwrap_or_else(|| "ffmpeg".into()),
            ffprobe: var("KINORA_FFPROBE").unwrap_or_else(|| "ffprobe".into()),
            tmdb_api_key: var("TMDB_API_KEY"),
            scan_concurrency: var("KINORA_SCAN_CONCURRENCY")
                .and_then(|v| v.parse().ok())
                .map(|n: usize| n.clamp(1, 64))
                .unwrap_or(8),
        }
    }

    pub fn database_path(&self) -> PathBuf {
        self.config_dir.join("kinora.db")
    }

    pub fn themes_dir(&self) -> PathBuf {
        self.config_dir.join("themes")
    }

    pub fn skins_dir(&self) -> PathBuf {
        self.config_dir.join("player-skins")
    }

    pub fn app_skins_dir(&self) -> PathBuf {
        self.config_dir.join("app-skins")
    }

    /// Installed modules, one folder each.
    pub fn modules_dir(&self) -> PathBuf {
        self.config_dir.join("modules")
    }

    /// Packages being downloaded, before they are checked and moved into place.
    pub fn staging_dir(&self) -> PathBuf {
        self.config_dir.join(".staging")
    }

    pub fn image_cache_dir(&self) -> PathBuf {
        self.cache_dir.join("images")
    }
}
