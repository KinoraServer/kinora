//! Metadata providers (extension point `metadata_provider`): an installed
//! module, such as TMDB, that looks titles up for the scanner.

use std::sync::Arc;

use async_trait::async_trait;
use kinora_plugin_api::{ExtensionPoint, MediaKind, Metadata, MetadataProvider};
use serde_json::{json, Value};

use super::wasm::InstalledModule;
use crate::state::AppState;

struct ModuleProvider {
    state: AppState,
    module: Arc<InstalledModule>,
}

impl ModuleProvider {
    async fn lookup(&self, op: &str, args: Value) -> anyhow::Result<Option<Metadata>> {
        let answer = self.state.modules.call(&self.state, &self.module, op, args).await?;
        Ok(serde_json::from_value(answer)?)
    }
}

#[async_trait]
impl MetadataProvider for ModuleProvider {
    fn id(&self) -> &str {
        &self.module.id
    }

    async fn search(&self, kind: MediaKind, title: &str, year: Option<u16>) -> anyhow::Result<Option<Metadata>> {
        self.lookup("metadata.search", json!({ "kind": kind, "title": title, "year": year })).await
    }

    async fn details(&self, kind: MediaKind, provider_id: &str) -> anyhow::Result<Option<Metadata>> {
        self.lookup("metadata.details", json!({ "kind": kind, "provider_id": provider_id })).await
    }

    async fn episode(&self, series_provider_id: &str, season: u32, episode: u32) -> anyhow::Result<Option<Metadata>> {
        let args = json!({ "series_provider_id": series_provider_id, "season": season, "episode": episode });
        self.lookup("metadata.episode", args).await
    }
}

/// The metadata provider to use: the first installed, switched-on module that
/// says it is ready (TMDB is not until it has an API key). `None` when there isn't one.
pub async fn provider(state: &AppState) -> Option<Arc<dyn MetadataProvider>> {
    for module in state.modules.providers(state, ExtensionPoint::MetadataProvider).await {
        match state.modules.call(state, &module, "metadata.ready", Value::Null).await {
            Ok(v) if v.get("ready").and_then(Value::as_bool) == Some(true) => {
                return Some(Arc::new(ModuleProvider { state: state.clone(), module }));
            }
            Ok(v) => {
                let reason = v.get("reason").and_then(Value::as_str).unwrap_or("not set up");
                tracing::debug!(module = %module.id, reason, "metadata provider is not ready");
            }
            Err(e) => tracing::warn!(error = %e, "metadata provider failed"),
        }
    }
    None
}
