use std::sync::Arc;

use kinora_plugin_api::ServerEvent;
use sqlx::SqlitePool;
use tokio::sync::{broadcast, RwLock};

use crate::config::Config;
use crate::hw::HardwareReport;
use crate::modules::registry::Registry;
use crate::modules::wasm::ModuleHost;
use crate::now_playing::NowPlaying;
use crate::scanner::ScanManager;
use crate::transcode::TranscodeManager;

/// Shared state handed to every request handler. Cheap to clone (all `Arc`s).
#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub config: Arc<Config>,
    pub hardware: Arc<RwLock<HardwareReport>>,
    pub transcodes: TranscodeManager,
    /// Every playback in progress, for the dashboard.
    pub now_playing: NowPlaying,
    pub scans: ScanManager,
    pub http: reqwest::Client,
    /// Runs installed modules.
    pub modules: ModuleHost,
    /// Where packages are installed from.
    pub registry: Registry,
    pub events: broadcast::Sender<ServerEvent>,
}

impl AppState {
    pub fn emit(&self, event: ServerEvent) {
        // No subscribers is fine.
        let _ = self.events.send(event);
    }
}
