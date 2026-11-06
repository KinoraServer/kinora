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
}

impl Config {
    pub fn from_env() -> Self {
        let var = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
        let config_dir = PathBuf::from(var("KINORA_CONFIG_DIR").unwrap_or_else(|| "./data".into()));
        let cache_dir = var("KINORA_CACHE_DIR").map(PathBuf::from).unwrap_or_else(|| config_dir.join("cache"));
        let transcode_dir =
            var("KINORA_TRANSCODE_DIR").map(PathBuf::from).unwrap_or_else(|| cache_dir.join("transcodes"));
        Config {
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

    pub fn image_cache_dir(&self) -> PathBuf {
        self.cache_dir.join("images")
    }
}
