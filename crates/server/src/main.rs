//! Kinora: a lightweight, self-hosted media server.

mod auth;
mod config;
mod db;
mod error;
mod hw;
mod items;
mod libraries;
mod modules;
mod now_playing;
mod page_cache;
mod playback;
mod probe;
mod scanner;
mod state;
mod system;
mod transcode;
mod web;

#[cfg(test)]
mod api_tests;

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use tokio::sync::{broadcast, RwLock};
use tower_http::compression::{CompressionLayer, Predicate};
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

use crate::config::Config;
use crate::hw::HardwareReport;
use crate::now_playing::NowPlaying;
use crate::scanner::ScanManager;
use crate::state::AppState;
use crate::transcode::TranscodeManager;

pub fn router(state: AppState) -> Router {
    Router::new()
        .merge(auth::routes())
        .merge(system::routes())
        .merge(libraries::routes())
        .merge(items::routes())
        .merge(playback::routes())
        .merge(modules::routes())
        .fallback(web::static_handler)
        // Compress JSON and the web app; video is already compressed.
        .layer(
            CompressionLayer::new().compress_when(
                tower_http::compression::predicate::DefaultPredicate::new()
                    .and(tower_http::compression::predicate::NotForContentType::new("video/")),
            ),
        )
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

pub async fn build_state(config: Config) -> anyhow::Result<AppState> {
    for dir in
        [&config.config_dir, &config.cache_dir, &config.themes_dir(), &config.skins_dir(), &config.app_skins_dir()]
    {
        std::fs::create_dir_all(dir)?;
    }
    // Downloads an earlier run didn't finish.
    let _ = std::fs::remove_dir_all(config.staging_dir());
    let example = config.themes_dir().join("example-ocean.json.sample");
    if !example.exists() {
        let _ = std::fs::write(&example, modules::themes::EXAMPLE_THEME);
    }
    modules::skins::write_example(&config.skins_dir());
    modules::app_skins::prepare_dir(&config.app_skins_dir());
    let db = db::connect(&config.database_path()).await?;
    modules::seed_from_env(&db, config.tmdb_api_key.as_deref()).await?;

    let (events, _) = broadcast::channel(256);
    let http = reqwest::Client::builder()
        .user_agent(concat!("Kinora/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(20))
        .build()?;
    let transcodes = TranscodeManager::new(config.transcode_dir.clone(), config.ffmpeg.clone());
    Ok(AppState {
        db,
        config: Arc::new(config),
        hardware: Arc::new(RwLock::new(HardwareReport::default())),
        transcodes,
        now_playing: NowPlaying::new(),
        scans: ScanManager::default(),
        http,
        modules: modules::wasm::ModuleHost::new()?,
        registry: modules::registry::Registry::default(),
        events,
    })
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // `kinora skin new|check …`: tools for skin authors; no server is started.
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("skin") {
        std::process::exit(modules::app_skins::cli(&args[1..]));
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_env("KINORA_LOG").unwrap_or_else(|_| EnvFilter::new("info,tower_http=warn,sqlx=warn")),
        )
        .init();

    let config = Config::from_env();
    tracing::info!(config_dir = %config.config_dir.display(), "starting Kinora {}", env!("CARGO_PKG_VERSION"));
    let addr = format!("{}:{}", config.bind, config.port);
    let state = build_state(config).await?;

    modules::events::spawn(state.clone());
    modules::registry::spawn_adoption(state.clone());
    system::spawn_detection(&state);
    spawn_scheduled_scans(state.clone());

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("listening on http://{addr}");
    // Connection info gives the dashboard each viewer's address.
    axum::serve(listener, router(state).into_make_service_with_connect_info::<std::net::SocketAddr>())
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

/// Rescan every library on startup and then every N minutes (setting).
fn spawn_scheduled_scans(state: AppState) {
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(5)).await;
        loop {
            scanner::scan_all(&state).await;
            let minutes = system::load_general(&state).await.map(|g| g.scan_interval_minutes).unwrap_or(60);
            let wait = if minutes == 0 { 3600 } else { u64::from(minutes) * 60 };
            tokio::time::sleep(Duration::from_secs(wait)).await;
            if minutes == 0 {
                // Disabled: keep checking whether it's been turned back on.
                while system::load_general(&state).await.map(|g| g.scan_interval_minutes).unwrap_or(0) == 0 {
                    tokio::time::sleep(Duration::from_secs(300)).await;
                }
            }
        }
    });
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("shutting down");
}
