//! Hands server events to the installed modules that subscribe to them
//! (extension point `event_subscriber`), e.g. the activity log.

use kinora_plugin_api::ExtensionPoint;
use serde_json::json;
use tokio::sync::broadcast;

use crate::state::AppState;

pub fn spawn(state: AppState) {
    let mut rx = state.events.subscribe();
    tokio::spawn(async move {
        loop {
            let event = match rx.recv().await {
                Ok(e) => e,
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            };
            let subscribers = state.modules.providers(&state, ExtensionPoint::EventSubscriber).await;
            if subscribers.is_empty() {
                continue;
            }
            let args = json!({ "event": event });
            for module in subscribers {
                if let Err(e) = state.modules.call(&state, &module, "event", args.clone()).await {
                    tracing::warn!(error = %e, "a module could not handle an event");
                }
            }
        }
    });
}
