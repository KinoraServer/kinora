//! Installing packages from a registry (Admin > Modules): modules, themes,
//! player skins and app skins. The format and its rules are in
//! `kinora_plugin_api::registry`; docs/modules.md is the guide.
//!
//! An install downloads into a staging folder, checks every file against the
//! hash in the package's manifest (itself checked against the index, which is
//! checked against its signature when a key is set), runs the same checks a
//! hand-installed theme or skin gets, and only then moves the files into place.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::{Path as UrlPath, Query, State};
use axum::http::StatusCode;
use axum::Json;
use ed25519_dalek::{Signature, VerifyingKey};
use kinora_plugin_api::registry::{
    is_newer, Index, IndexPackage, IndexSignature, PackageKind, PackageManifest, Permissions, MAX_FILE_BYTES,
    MAX_INDEX_BYTES, MAX_MANIFEST_BYTES, MODULE_ENTRY,
};
use kinora_plugin_api::Theme;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;

use crate::auth::AdminUser;
use crate::db::{get_setting, new_id, now_ms, set_setting};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// How long a downloaded index is reused before asking the registry again.
const INDEX_TTL: Duration = Duration::from_secs(600);
/// Set once the packages that used to be built in were installed (or weren't needed).
const ADOPTED_KEY: &str = "packages_adopted";
/// What Kinora shipped inside the binary before packages: the modules and the extra themes.
const PREVIOUSLY_BUILT_IN: &[&str] = &["tmdb", "auto-tracks", "activity-log", "midnight", "oled", "ember"];

struct CachedIndex {
    at: Instant,
    index: Arc<Index>,
    signed: bool,
}

/// The registry client's state: the last index, and a lock so installs run one at a time.
#[derive(Clone, Default)]
pub struct Registry {
    cache: Arc<Mutex<Option<CachedIndex>>>,
    busy: Arc<Mutex<()>>,
}

/// A package as the `packages` table has it.
#[derive(Debug, Clone)]
pub struct InstalledPackage {
    pub manifest: PackageManifest,
    pub installed_at: i64,
}

fn sha256_hex(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let (mut buffer, mut bits) = (0u32, 0u32);
    for c in text.trim().trim_end_matches('=').bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => return None,
        };
        buffer = (buffer << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }
    Some(out)
}

/// Check `index.json`'s signature against the keys this server trusts.
pub fn verify_signature(index: &[u8], signature: &[u8], keys: &[String]) -> Result<(), String> {
    let sig: IndexSignature = serde_json::from_slice(signature).map_err(|_| "the signature file is unreadable")?;
    if sig.algorithm != "ed25519" {
        return Err(format!("unknown signature algorithm {:?}", sig.algorithm));
    }
    let bytes: [u8; 64] = base64_decode(&sig.signature)
        .and_then(|b| b.try_into().ok())
        .ok_or("the signature is not a valid Ed25519 signature")?;
    let signature = Signature::from_bytes(&bytes);
    for key in keys {
        let Some(raw) = base64_decode(key).and_then(|b| <[u8; 32]>::try_from(b).ok()) else { continue };
        if sha256_hex(&raw)[..16] != sig.key_id {
            continue;
        }
        let key = VerifyingKey::from_bytes(&raw).map_err(|_| "a registry key in the configuration is not valid")?;
        return key.verify_strict(index, &signature).map_err(|_| "the registry's signature does not match".into());
    }
    Err("the registry is signed with a key this server doesn't trust".into())
}

async fn fetch(state: &AppState, path: &str, max_bytes: usize) -> Result<Vec<u8>, String> {
    let url = format!("{}/{path}", state.config.registry_url);
    let unreachable = |e: reqwest::Error| format!("could not reach the registry: {}", e.without_url());
    let mut response = state.http.get(&url).send().await.map_err(unreachable)?;
    if !response.status().is_success() {
        return Err(format!("the registry answered {} for {path}", response.status().as_u16()));
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(unreachable)? {
        if body.len() + chunk.len() > max_bytes {
            return Err(format!("{path} is larger than expected"));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

impl Registry {
    /// The registry's index, from memory when it is recent. `signed` says whether a signature was checked.
    pub async fn index(&self, state: &AppState, refresh: bool) -> Result<(Arc<Index>, bool), String> {
        let mut cache = self.cache.lock().await;
        if let Some(c) = cache.as_ref().filter(|c| !refresh && c.at.elapsed() < INDEX_TTL) {
            return Ok((c.index.clone(), c.signed));
        }
        let raw = fetch(state, "index.json", MAX_INDEX_BYTES).await?;
        let keys = &state.config.registry_keys;
        if !keys.is_empty() {
            let signature = fetch(state, "index.json.sig", 4096)
                .await
                .map_err(|e| format!("the registry's signature is missing ({e})"))?;
            verify_signature(&raw, &signature, keys)?;
        }
        let index: Index =
            serde_json::from_slice(&raw).map_err(|e| format!("the registry's index is unreadable: {e}"))?;
        let index = Arc::new(index.check()?);
        *cache = Some(CachedIndex { at: Instant::now(), index: index.clone(), signed: !keys.is_empty() });
        Ok((index, !keys.is_empty()))
    }
}

pub async fn installed(state: &AppState) -> sqlx::Result<Vec<InstalledPackage>> {
    let rows: Vec<(String, i64)> =
        sqlx::query_as("SELECT manifest, installed_at FROM packages ORDER BY id").fetch_all(&state.db).await?;
    Ok(rows
        .into_iter()
        .filter_map(|(manifest, installed_at)| {
            Some(InstalledPackage { manifest: serde_json::from_str(&manifest).ok()?, installed_at })
        })
        .collect())
}

async fn installed_one(state: &AppState, id: &str) -> sqlx::Result<Option<InstalledPackage>> {
    Ok(installed(state).await?.into_iter().find(|p| p.manifest.id == id))
}

/// Where a package's files live once installed. Themes are single files in
/// the themes folder; everything else gets a folder named after the package.
fn target_dir(state: &AppState, kind: PackageKind, id: &str) -> PathBuf {
    match kind {
        PackageKind::Module => state.config.modules_dir().join(id),
        PackageKind::Theme => state.config.themes_dir(),
        PackageKind::PlayerSkin => state.config.skins_dir().join(id),
        PackageKind::AppSkin => state.config.app_skins_dir().join(id),
    }
}

fn still_on_disk(state: &AppState, m: &PackageManifest) -> bool {
    let dir = target_dir(state, m.kind, &m.id);
    match m.kind {
        PackageKind::Theme => m.files.iter().any(|f| dir.join(&f.path).is_file()),
        _ => dir.is_dir(),
    }
}

/// Swap a checked staging folder in for `target`, replacing what was there.
fn replace_dir(state: &AppState, staging: &Path, target: &Path) -> std::io::Result<()> {
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let old = state.config.staging_dir().join(format!("old-{}", new_id()));
    let had_old = target.exists();
    if had_old {
        std::fs::rename(target, &old)?;
    }
    if let Err(e) = std::fs::rename(staging, target) {
        if had_old {
            let _ = std::fs::rename(&old, target);
        }
        return Err(e);
    }
    let _ = std::fs::remove_dir_all(&old);
    Ok(())
}

/// Check the files in `staging` as their kind demands and move them into
/// place. `staging` holds exactly the manifest's files, already matched
/// against their hashes.
pub async fn install_staged(state: &AppState, manifest: &PackageManifest, staging: &Path) -> Result<(), String> {
    let id = manifest.id.as_str();
    let previous = installed_one(state, id).await.map_err(|e| e.to_string())?;
    if previous.as_ref().is_some_and(|p| p.manifest.kind != manifest.kind) {
        return Err(format!("{id} is already installed as a different kind of package; uninstall it first"));
    }
    let target = target_dir(state, manifest.kind, id);
    let io = |e: std::io::Error| format!("could not write the package: {e}");
    // A folder that is there but wasn't installed from a registry is the admin's own: leave it alone.
    let taken = |what: &str| {
        format!("there is already a {what} called {id:?} that was not installed from the registry; remove or rename it first")
    };

    match manifest.kind {
        PackageKind::Module => {
            let wasm = std::fs::read(staging.join(MODULE_ENTRY)).map_err(io)?;
            super::wasm::check(&wasm)?;
            replace_dir(state, staging, &target).map_err(io)?;
        }
        PackageKind::Theme => {
            let owned = |path: &str| previous.as_ref().is_some_and(|p| p.manifest.files.iter().any(|f| f.path == path));
            for f in &manifest.files {
                let text = std::fs::read_to_string(staging.join(&f.path)).map_err(io)?;
                let theme: Theme = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", f.path))?;
                theme.validate().map_err(|e| format!("{}: {e}", f.path))?;
                if format!("{}.json", theme.id) != f.path {
                    return Err(format!("{}: the file must be named after the theme's id", f.path));
                }
                if super::themes::builtin_themes().iter().any(|b| b.id == theme.id) {
                    return Err(format!("{:?} is one of Kinora's own themes", theme.id));
                }
                if target.join(&f.path).exists() && !owned(&f.path) {
                    return Err(taken("theme file"));
                }
            }
            std::fs::create_dir_all(&target).map_err(io)?;
            if let Some(p) = &previous {
                for f in &p.manifest.files {
                    let _ = std::fs::remove_file(target.join(&f.path));
                }
            }
            for f in &manifest.files {
                std::fs::copy(staging.join(&f.path), target.join(&f.path)).map_err(io)?;
            }
            let _ = std::fs::remove_dir_all(staging);
        }
        PackageKind::PlayerSkin => {
            let skin = super::skins::load_one(staging)?;
            if skin.id != id {
                return Err(format!("the skin's id ({:?}) must be the package's id", skin.id));
            }
            if target.exists() && previous.is_none() {
                return Err(taken("player skin"));
            }
            replace_dir(state, staging, &target).map_err(io)?;
        }
        PackageKind::AppSkin => {
            super::app_skins::check_package(staging, id)?;
            if target.exists() && previous.is_none() {
                return Err(taken("app skin"));
            }
            replace_dir(state, staging, &target).map_err(io)?;
        }
    }

    sqlx::query(
        "INSERT INTO packages (id, kind, version, manifest, registry, installed_at) VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET kind = excluded.kind, version = excluded.version,
            manifest = excluded.manifest, registry = excluded.registry, installed_at = excluded.installed_at",
    )
    .bind(id)
    .bind(manifest.kind.as_str())
    .bind(&manifest.version)
    .bind(serde_json::to_string(manifest).map_err(|e| e.to_string())?)
    .bind(&state.config.registry_url)
    .bind(now_ms())
    .execute(&state.db)
    .await
    .map_err(|e| e.to_string())?;
    state.modules.invalidate().await;
    Ok(())
}

/// Download, check and install the newest version of a package this server can run.
pub async fn install(state: &AppState, id: &str) -> Result<PackageManifest, String> {
    let _busy = state.registry.busy.lock().await;
    let (index, _) = state.registry.index(state, false).await?;
    let package = index.packages.iter().find(|p| p.id == id).ok_or("the registry has no package with that id")?;
    let version = package
        .best_version(env!("CARGO_PKG_VERSION"))
        .ok_or("no version of this package works with this version of Kinora")?;

    let raw = fetch(state, &version.manifest, MAX_MANIFEST_BYTES).await?;
    if sha256_hex(&raw) != version.sha256 {
        return Err("the package's manifest does not match the registry's index".into());
    }
    let manifest: PackageManifest =
        serde_json::from_slice(&raw).map_err(|e| format!("the package's manifest is unreadable: {e}"))?;
    manifest.validate(package, &version.version)?;

    let staging = state.config.staging_dir().join(format!("{id}-{}", new_id()));
    let result = async {
        for file in &manifest.files {
            let data = fetch(state, &manifest.file_url_path(file), MAX_FILE_BYTES as usize).await?;
            if data.len() as u64 != file.size || sha256_hex(&data) != file.sha256 {
                return Err(format!("{} does not match its checksum; nothing was installed", file.path));
            }
            let path = staging.join(&file.path);
            let write = |e: std::io::Error| format!("could not write the package: {e}");
            std::fs::create_dir_all(path.parent().unwrap_or(&staging)).map_err(write)?;
            std::fs::write(&path, &data).map_err(write)?;
        }
        install_staged(state, &manifest, &staging).await
    }
    .await;
    let _ = std::fs::remove_dir_all(&staging);
    result?;
    tracing::info!(package = id, version = %manifest.version, "installed from the registry");
    Ok(manifest)
}

/// Remove an installed package's files. With `purge`, a module's settings and stored data go too.
pub async fn uninstall(state: &AppState, id: &str, purge: bool) -> AppResult<()> {
    let _busy = state.registry.busy.lock().await;
    let package = installed_one(state, id).await?.ok_or(AppError::NotFound)?;
    let m = &package.manifest;
    let target = target_dir(state, m.kind, id);
    let gone = |r: std::io::Result<()>| match r {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    };
    match m.kind {
        PackageKind::Theme => {
            for f in &m.files {
                gone(std::fs::remove_file(target.join(&f.path)))?;
            }
        }
        _ => gone(std::fs::remove_dir_all(&target))?,
    }
    if m.kind == PackageKind::AppSkin {
        super::app_skins::forget(state, id).await?;
    }
    sqlx::query("DELETE FROM packages WHERE id = ?").bind(id).execute(&state.db).await?;
    if m.kind == PackageKind::Module && purge {
        sqlx::query("DELETE FROM modules WHERE id = ?").bind(id).execute(&state.db).await?;
        sqlx::query("DELETE FROM module_data WHERE module_id = ?").bind(id).execute(&state.db).await?;
    }
    state.modules.invalidate().await;
    tracing::info!(package = id, purge, "uninstalled");
    Ok(())
}

/// Forget a package whose files were removed some other way (a skin deleted
/// from its own card, a folder deleted by hand).
pub async fn forget_missing(state: &AppState) -> sqlx::Result<()> {
    let mut changed = false;
    for p in installed(state).await? {
        if !still_on_disk(state, &p.manifest) {
            sqlx::query("DELETE FROM packages WHERE id = ?").bind(&p.manifest.id).execute(&state.db).await?;
            changed = true;
        }
    }
    if changed {
        state.modules.invalidate().await;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// API
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct PackageView {
    id: String,
    kind: PackageKind,
    name: String,
    description: String,
    author: Option<String>,
    license: Option<String>,
    homepage: Option<String>,
    recommended: bool,
    /// Modules: the extension points it provides.
    provides: Vec<String>,
    /// Modules: what it may do outside its sandbox.
    permissions: Permissions,
    /// The newest version this server can install; missing when there is none
    /// (not in the registry any more, or it needs a newer Kinora).
    version: Option<String>,
    /// Download size of that version in bytes.
    size: Option<u64>,
    installed_version: Option<String>,
    /// Unix ms.
    installed_at: Option<i64>,
    update_available: bool,
}

#[derive(Serialize)]
struct RegistryView {
    url: String,
    /// The index's signature was checked against a pinned key.
    signed: bool,
    /// Why the registry couldn't be read. Installed packages are still listed.
    error: Option<String>,
    packages: Vec<PackageView>,
}

fn view(package: Option<&IndexPackage>, installed: Option<&InstalledPackage>) -> Option<PackageView> {
    let best = package.and_then(|p| p.best_version(env!("CARGO_PKG_VERSION")));
    let installed_version = installed.map(|i| i.manifest.version.clone());
    let m = installed.map(|i| &i.manifest);
    Some(PackageView {
        id: package.map(|p| p.id.clone()).or(m.map(|m| m.id.clone()))?,
        kind: package.map(|p| p.kind).or(m.map(|m| m.kind))?,
        name: package.map(|p| p.name.clone()).or(m.map(|m| m.name.clone()))?,
        description: package.map(|p| p.description.clone()).or(m.map(|m| m.description.clone()))?,
        author: package.and_then(|p| p.author.clone()).or(m.and_then(|m| m.author.clone())),
        license: package.and_then(|p| p.license.clone()).or(m.and_then(|m| m.license.clone())),
        homepage: package.and_then(|p| p.homepage.clone()).or(m.and_then(|m| m.homepage.clone())),
        recommended: package.is_some_and(|p| p.recommended),
        provides: match (package, m.and_then(|m| m.module.as_ref())) {
            (Some(p), _) => p.provides.clone(),
            (None, Some(spec)) => {
                spec.provides.iter().filter_map(|p| serde_json::to_value(p).ok()?.as_str().map(String::from)).collect()
            }
            _ => vec![],
        },
        permissions: package
            .map(|p| p.permissions.clone())
            .or(m.and_then(|m| m.module.as_ref()).map(|s| s.permissions.clone()))
            .unwrap_or_default(),
        update_available: matches!((best, &installed_version), (Some(b), Some(i)) if is_newer(&b.version, i)),
        version: best.map(|b| b.version.clone()),
        size: best.map(|b| b.size),
        installed_at: installed.map(|i| i.installed_at),
        installed_version,
    })
}

#[derive(Deserialize)]
pub struct ListQuery {
    /// Ask the registry again instead of using the copy in memory.
    #[serde(default)]
    refresh: bool,
}

/// `GET /api/registry`: every package in the registry and everything installed.
async fn list(
    State(state): State<AppState>,
    _admin: AdminUser,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<RegistryView>> {
    forget_missing(&state).await?;
    let installed = installed(&state).await?;
    let (index, signed, error) = match state.registry.index(&state, q.refresh).await {
        Ok((index, signed)) => (Some(index), signed, None),
        Err(e) => (None, false, Some(e)),
    };
    let mut packages = Vec::new();
    for p in index.iter().flat_map(|i| i.packages.iter()) {
        packages.extend(view(Some(p), installed.iter().find(|i| i.manifest.id == p.id)));
    }
    for i in &installed {
        if !packages.iter().any(|p| p.id == i.manifest.id) {
            packages.extend(view(None, Some(i)));
        }
    }
    packages.sort_by(|a, b| {
        b.recommended.cmp(&a.recommended).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(Json(RegistryView { url: state.config.registry_url.clone(), signed, error, packages }))
}

/// `POST /api/registry/{id}/install`: install a package, or update it to the newest version.
async fn install_route(
    State(state): State<AppState>,
    _admin: AdminUser,
    UrlPath(id): UrlPath<String>,
) -> AppResult<Json<serde_json::Value>> {
    let manifest = install(&state, &id).await.map_err(AppError::BadRequest)?;
    Ok(Json(serde_json::json!({ "id": manifest.id, "version": manifest.version })))
}

#[derive(Deserialize)]
pub struct UninstallQuery {
    /// Also delete a module's settings and the data it stored.
    #[serde(default)]
    purge: bool,
}

/// `DELETE /api/registry/{id}`: uninstall a package.
async fn uninstall_route(
    State(state): State<AppState>,
    _admin: AdminUser,
    UrlPath(id): UrlPath<String>,
    Query(q): Query<UninstallQuery>,
) -> AppResult<StatusCode> {
    uninstall(&state, &id, q.purge).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub fn routes() -> axum::Router<AppState> {
    use axum::routing::{delete, get, post};
    axum::Router::new()
        .route("/api/registry", get(list))
        .route("/api/registry/{id}/install", post(install_route))
        .route("/api/registry/{id}", delete(uninstall_route))
}

// ---------------------------------------------------------------------------
// Upgrading from a version that had its modules built in
// ---------------------------------------------------------------------------

/// A server that was set up before packages existed had TMDB, auto tracks, the
/// activity log and three extra themes built in. Install those once, in the
/// background, so an upgrade doesn't take features away. A new server starts
/// with nothing installed. Modules the admin had switched off are skipped.
/// When the registry can't be reached this is tried again on the next start.
pub fn spawn_adoption(state: AppState) {
    tokio::spawn(async move { adopt(&state).await });
}

pub async fn adopt(state: &AppState) {
    {
        if get_setting(&state.db, ADOPTED_KEY).await.ok().flatten().is_some() {
            return;
        }
        let users: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users").fetch_one(&state.db).await.unwrap_or(0);
        if users == 0 {
            let _ = set_setting(&state.db, ADOPTED_KEY, "new").await;
            return;
        }
        let mut complete = true;
        for id in PREVIOUSLY_BUILT_IN {
            let switched_off: Option<i64> = sqlx::query_scalar("SELECT enabled FROM modules WHERE id = ?")
                .bind(id)
                .fetch_optional(&state.db)
                .await
                .ok()
                .flatten();
            if switched_off == Some(0) || installed_one(state, id).await.ok().flatten().is_some() {
                continue;
            }
            match install(state, id).await {
                Ok(m) => {
                    tracing::info!(package = id, version = %m.version, "installed a module this server used to have built in")
                }
                Err(e) => {
                    tracing::warn!(package = id, error = %e, "could not install a module this server used to have built in; install it from Admin > Modules");
                    complete = false;
                }
            }
        }
        if complete {
            let _ = set_setting(&state.db, ADOPTED_KEY, "upgraded").await;
        }
    }
}
