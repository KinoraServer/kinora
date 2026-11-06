//! Activity log (built-in module "activity-log"): records server events so
//! admins can see who watched what and when scans ran.

use kinora_plugin_api::{ExtensionPoint, ModuleInfo, ServerEvent};
use sqlx::SqlitePool;
use tokio::sync::broadcast;

pub fn info() -> ModuleInfo {
    ModuleInfo {
        id: "activity-log".into(),
        name: "Activity log".into(),
        description: "Keeps a history of playback, library scans and new users on the admin dashboard.".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        provides: vec![ExtensionPoint::EventSubscriber],
        enabled_by_default: true,
        settings: vec![],
    }
}

fn user_of(e: &ServerEvent) -> Option<&str> {
    match e {
        ServerEvent::PlaybackStarted { user_id, .. } | ServerEvent::PlaybackStopped { user_id, .. } => Some(user_id),
        _ => None,
    }
}

pub fn spawn(db: SqlitePool, mut rx: broadcast::Receiver<ServerEvent>) {
    tokio::spawn(async move {
        loop {
            let event = match rx.recv().await {
                Ok(e) => e,
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            };
            if !super::is_enabled(&db, "activity-log").await {
                continue;
            }
            let payload = serde_json::to_value(&event).unwrap_or_default();
            let kind = payload.get("type").and_then(|t| t.as_str()).unwrap_or("event").to_string();
            let _ = sqlx::query("INSERT INTO activity_log (time, user_id, event, payload) VALUES (?, ?, ?, ?)")
                .bind(crate::db::now_ms())
                .bind(user_of(&event))
                .bind(kind)
                .bind(payload.to_string())
                .execute(&db)
                .await;
        }
    });
}
