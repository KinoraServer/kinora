//! Test helpers: the module the tests install (built from
//! `crates/server/test-module`) and a way to install it without a registry.

use kinora_plugin_api::registry::PackageManifest;
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::state::AppState;

pub const FIXTURE_WASM: &[u8] = include_bytes!("../../test-module/test_module.wasm");

/// A manifest for the test module under `id`, providing every module extension point.
pub fn fixture_manifest(id: &str, version: &str, permissions: serde_json::Value) -> PackageManifest {
    serde_json::from_value(json!({
        "format": 1,
        "id": id,
        "kind": "module",
        "name": "Test module",
        "description": "Installed by the tests.",
        "version": version,
        "module": {
            "abi": 1,
            "entry": "module.wasm",
            "provides": ["metadata_provider", "track_selection", "event_subscriber"],
            "permissions": permissions,
            "settings": [
                { "key": "api_key", "label": "Key", "kind": "secret" },
                { "key": "language", "label": "Language", "kind": "text", "default": "en-US" },
                { "key": "base_url", "label": "Base URL", "kind": "text" },
                { "key": "per_season", "label": "Per season", "kind": "bool", "default": "false" },
            ],
        },
        "files": [{
            "path": "module.wasm",
            "size": FIXTURE_WASM.len(),
            "sha256": hex::encode(Sha256::digest(FIXTURE_WASM)),
        }],
    }))
    .unwrap()
}

/// Install the test module as if it had been downloaded and checked.
pub async fn install_fixture(state: &AppState, id: &str, permissions: serde_json::Value) {
    let manifest = fixture_manifest(id, "1.0.0", permissions);
    let staging = state.config.staging_dir().join(format!("test-{id}"));
    std::fs::create_dir_all(&staging).unwrap();
    std::fs::write(staging.join("module.wasm"), FIXTURE_WASM).unwrap();
    super::registry::install_staged(state, &manifest, &staging).await.unwrap();
}

/// A registry on disk, served over HTTP on a local port for as long as it lives.
pub struct TestRegistry {
    pub dir: tempfile::TempDir,
    pub url: String,
    packages: Vec<serde_json::Value>,
}

impl TestRegistry {
    pub async fn start() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let app = axum::Router::new().fallback_service(tower_http::services::ServeDir::new(dir.path()));
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        TestRegistry { dir, url, packages: Vec::new() }
    }

    /// Add one version of a package. `manifest` is a package.json without `format` and `files`.
    pub fn add(&mut self, mut manifest: serde_json::Value, files: &[(&str, &[u8])]) {
        let (id, version) =
            (manifest["id"].as_str().unwrap().to_string(), manifest["version"].as_str().unwrap().to_string());
        let root = self.dir.path().join(format!("packages/{id}/{version}"));
        let mut listed = Vec::new();
        for (path, data) in files {
            let file = root.join("files").join(path);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, data).unwrap();
            listed.push(json!({ "path": path, "size": data.len(), "sha256": hex::encode(Sha256::digest(data)) }));
        }
        manifest["format"] = json!(1);
        manifest["files"] = json!(listed);
        let raw = serde_json::to_vec_pretty(&manifest).unwrap();
        std::fs::write(root.join("package.json"), &raw).unwrap();
        let entry = json!({
            "version": version,
            "abi": manifest["module"]["abi"],
            "min_kinora": manifest["min_kinora"],
            "size": files.iter().map(|(_, d)| d.len()).sum::<usize>(),
            "manifest": format!("packages/{id}/{version}/package.json"),
            "sha256": hex::encode(Sha256::digest(&raw)),
        });
        match self.packages.iter_mut().find(|p| p["id"] == id.as_str()) {
            Some(p) => p["versions"].as_array_mut().unwrap().insert(0, entry),
            None => self.packages.push(json!({
                "id": id, "kind": manifest["kind"], "name": manifest["name"], "description": manifest["description"],
                "recommended": manifest["recommended"] == true,
                "provides": manifest["module"]["provides"].as_array().cloned().unwrap_or_default(),
                "permissions": manifest["module"].get("permissions").cloned().unwrap_or(json!({})),
                "versions": [entry],
            })),
        }
    }

    /// The test module as a package called `id`.
    pub fn add_module(&mut self, id: &str, version: &str) {
        let mut manifest = serde_json::to_value(fixture_manifest(id, version, json!({ "storage": true }))).unwrap();
        manifest["recommended"] = json!(true);
        self.add(manifest, &[("module.wasm", FIXTURE_WASM)]);
    }

    /// Write index.json, signed with an Ed25519 key when a seed is given.
    /// Returns the public key (base64) to pin.
    pub fn publish(&self, seed: Option<[u8; 32]>) -> Option<String> {
        use ed25519_dalek::Signer;
        let raw = serde_json::to_vec_pretty(&json!({ "format": 1, "packages": self.packages })).unwrap();
        std::fs::write(self.dir.path().join("index.json"), &raw).unwrap();
        let _ = std::fs::remove_file(self.dir.path().join("index.json.sig"));
        let key = ed25519_dalek::SigningKey::from_bytes(&seed?);
        let public = key.verifying_key().to_bytes();
        let signature = json!({
            "algorithm": "ed25519",
            "key_id": hex::encode(Sha256::digest(public))[..16],
            "signature": base64(&key.sign(&raw).to_bytes()),
        });
        std::fs::write(self.dir.path().join("index.json.sig"), signature.to_string()).unwrap();
        Some(base64(&public))
    }
}

pub fn base64(data: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let n = chunk.iter().fold(0u32, |n, b| (n << 8) | u32::from(*b)) << (8 * (3 - chunk.len()));
        for i in 0..4 {
            match i <= chunk.len() {
                true => out.push(ALPHABET[(n >> (18 - 6 * i)) as usize & 63] as char),
                false => out.push('='),
            }
        }
    }
    out
}
