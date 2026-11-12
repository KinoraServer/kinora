//! App skins: whole-app markup and CSS, installed as folders in
//! `<config>/app-skins/<id>/` (module "app-skins"). See docs/app-skins.md.
//!
//! The server reads each folder, checks it with
//! [`kinora_plugin_api::app_skin::check_skin`], and serves the active skin to
//! the web app as one JSON bundle. Templates with errors are left out of the
//! bundle, so the web app shows Kinora's own page there instead. The web app
//! sanitises everything again before use; skins never run code.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::UNIX_EPOCH;

use axum::body::{Body, Bytes};
use axum::extract::{Path as UrlPath, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use kinora_plugin_api::app_skin::{
    asset_type, check_skin, html_custom_data, manifest_schema, rules, valid_asset_name, valid_skin_id, AppSkinManifest,
    Issue, SkinOption, SkinReport, SkinSource,
};
use kinora_plugin_api::{ExtensionPoint, ModuleInfo, PlayerSkin, SettingField, SettingKind, Theme};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::auth::{AdminUser, AuthUser};
use crate::db::{get_setting, set_setting};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

pub const MODULE_ID: &str = "app-skins";
/// Server setting: the skin everyone uses ("" for none).
const ACTIVE_KEY: &str = "app_skin";
/// Per-user preference key: "none", a skin id, or absent (follow the server).
const USER_KEY: &str = "app_skin";
/// Biggest single file we read (templates, CSS, assets).
const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
/// Upload limits (a bit above the skin limits so the check can say what's wrong).
const MAX_UPLOAD_FILES: usize = 96;
const MAX_UPLOAD_BYTES: u64 = 8 * 1024 * 1024;

pub fn info() -> ModuleInfo {
    ModuleInfo {
        id: MODULE_ID.into(),
        name: "Custom app skins".into(),
        description: "Whole-app skins (markup and CSS for the navigation, home, library, detail and search pages): \
                      the ones you install from the registry, and your own from the app-skins folder in your config \
                      directory. Admin pages always keep the standard look."
            .into(),
        version: env!("CARGO_PKG_VERSION").into(),
        provides: vec![ExtensionPoint::AppSkin],
        enabled_by_default: false,
        settings: vec![SettingField {
            key: "user_choice".into(),
            label: "Let viewers pick their own skin".into(),
            kind: SettingKind::Bool,
            help: Some("Adds a skin picker to User Settings → Display. Your choice below stays the default.".into()),
            default: Some("false".into()),
        }],
    }
}

// ---------------------------------------------------------------------------
// Starter skins
// ---------------------------------------------------------------------------

/// The starter skins, written as `<name>.sample` folders on first start.
/// Tests also get Marquee, a full skin to exercise the engine with.
pub fn samples() -> Vec<(&'static str, Vec<(&'static str, &'static str)>)> {
    #[allow(unused_mut)]
    let mut all = vec![("blank", super::app_skin_samples::BLANK.to_vec())];
    #[cfg(test)]
    all.push(("marquee", super::app_skin_samples::MARQUEE.to_vec()));
    all
}

/// Write a starter into `dir/<folder>`, with a new id when given. Also adds
/// the editor helper files (autocomplete for data-kn-* and skin.json).
pub fn write_starter(dir: &Path, from: &str, folder: &str, id: Option<&str>) -> std::io::Result<()> {
    let files = samples()
        .into_iter()
        .find(|(n, _)| *n == from)
        .map(|(_, f)| f)
        .ok_or_else(|| std::io::Error::other(format!("unknown starter {from:?} (there is one: blank)")))?;
    let target = dir.join(folder);
    std::fs::create_dir_all(&target)?;
    for (path, text) in files {
        let p = target.join(path);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = match (path, id) {
            ("skin.json", Some(id)) => text.replacen(&format!("\"id\": \"{from}\""), &format!("\"id\": \"{id}\""), 1),
            _ => text.to_string(),
        };
        std::fs::write(p, text)?;
    }
    std::fs::write(target.join("kinora-skin.html-data.json"), html_custom_data())?;
    std::fs::write(target.join("skin.schema.json"), manifest_schema())?;
    std::fs::write(
        target.join(".vscode-settings.sample.json"),
        "{\n  \"html.customData\": [\"./kinora-skin.html-data.json\"],\n  \"json.schemas\": [{ \"fileMatch\": [\"skin.json\"], \"url\": \"./skin.schema.json\" }]\n}\n",
    )?;
    Ok(())
}

/// First start: the starters as `.sample` folders, and no leftover uploads.
pub fn prepare_dir(dir: &Path) {
    for (name, _) in samples() {
        let folder = format!("{name}.sample");
        if !dir.join(&folder).exists() {
            if let Err(e) = write_starter(dir, name, &folder, None) {
                tracing::warn!(error = %e, "could not write the {name} starter skin");
            }
        }
    }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            if e.file_name().to_string_lossy().starts_with(".upload-") {
                let _ = std::fs::remove_dir_all(e.path());
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Loading
// ---------------------------------------------------------------------------

/// One skin folder, read and checked.
#[derive(Debug, Clone)]
pub struct LoadedSkin {
    pub folder: String,
    pub dir: PathBuf,
    pub source: SkinSource,
    pub report: SkinReport,
    /// Changes whenever any file changes (bundle ETag).
    pub hash: String,
}

impl LoadedSkin {
    pub fn manifest(&self) -> Option<&AppSkinManifest> {
        self.report.manifest.as_ref()
    }
    pub fn usable(&self) -> bool {
        self.report.valid && self.manifest().is_some()
    }
    pub fn id(&self) -> &str {
        self.manifest().map(|m| m.id.as_str()).unwrap_or(&self.folder)
    }
}

fn is_text(path: &str) -> bool {
    !path.starts_with("assets/") && [".html", ".css", ".json", ".md"].iter().any(|e| path.ends_with(e))
}

/// Read a folder into a [`SkinSource`]: text files by relative path, other files by size.
pub fn read_source(dir: &Path, folder: &str) -> Result<SkinSource, String> {
    let mut src = SkinSource { folder: folder.into(), ..Default::default() };
    for entry in walkdir::WalkDir::new(dir).max_depth(3).follow_links(false) {
        let entry = entry.map_err(|e| e.to_string())?;
        if !entry.file_type().is_file() {
            continue;
        }
        let rel = entry.path().strip_prefix(dir).map_err(|e| e.to_string())?;
        let rel = rel.components().map(|c| c.as_os_str().to_string_lossy().to_string()).collect::<Vec<_>>().join("/");
        let hidden = rel.split('/').any(|p| p.starts_with('.'));
        // Editor helpers written by `kinora skin new` aren't part of the skin.
        if hidden || rel == "kinora-skin.html-data.json" || rel == "skin.schema.json" {
            continue;
        }
        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
        if size > MAX_FILE_BYTES {
            return Err(format!("{rel} is larger than {} MB", MAX_FILE_BYTES / 1024 / 1024));
        }
        if is_text(&rel) {
            let text = std::fs::read_to_string(entry.path()).map_err(|e| format!("{rel}: {e}"))?;
            src.texts.insert(rel, text);
        } else {
            src.binaries.insert(rel, size);
        }
    }
    Ok(src)
}

fn load_one(dir: &Path, folder: &str) -> LoadedSkin {
    let (source, mut report) = match read_source(dir, folder) {
        Ok(src) => {
            let report = check_skin(&src);
            (src, report)
        }
        Err(e) => {
            let report = SkinReport {
                issues: vec![Issue {
                    file: String::new(),
                    line: 0,
                    col: 0,
                    severity: kinora_plugin_api::app_skin::Severity::Error,
                    message: e,
                }],
                ..Default::default()
            };
            (SkinSource { folder: folder.into(), ..Default::default() }, report)
        }
    };
    let mut h = Sha256::new();
    for (k, v) in &source.texts {
        h.update(k.as_bytes());
        h.update([0]);
        h.update(v.as_bytes());
        h.update([0]);
    }
    for (k, v) in &source.binaries {
        h.update(k.as_bytes());
        h.update(v.to_le_bytes());
    }
    let hash = hex::encode(&h.finalize()[..8]);
    if report.manifest.is_none() {
        report.valid = false;
    }
    LoadedSkin { folder: folder.into(), dir: dir.to_path_buf(), source, report, hash }
}

/// Check a skin about to be installed from a registry: it must be usable and
/// carry the package's id. `dir` is the staging folder holding its files.
pub fn check_package(dir: &Path, id: &str) -> Result<(), String> {
    let skin = load_one(dir, id);
    if skin.manifest().map(|m| m.id.as_str()) != Some(id) {
        return Err("the skin's id must be the package's id".into());
    }
    if !skin.usable() {
        let first = skin.report.issues.iter().find(|i| i.severity == kinora_plugin_api::app_skin::Severity::Error);
        return Err(format!("the skin has errors{}", first.map(|i| format!(": {i}")).unwrap_or_default()));
    }
    Ok(())
}

/// A skin's folder is gone: stop using it as the server's skin.
pub async fn forget(state: &AppState, id: &str) -> sqlx::Result<()> {
    if get_setting(&state.db, ACTIVE_KEY).await?.as_deref() == Some(id) {
        set_setting(&state.db, ACTIVE_KEY, "").await?;
    }
    Ok(())
}

/// Cheap change detector: every file's path, size and modified time.
fn fingerprint(dir: &Path) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for entry in walkdir::WalkDir::new(dir).max_depth(4).follow_links(false).sort_by_file_name().into_iter().flatten() {
        entry.path().hash(&mut h);
        if let Ok(m) = entry.metadata() {
            m.len().hash(&mut h);
            m.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_nanos()).hash(&mut h);
        }
    }
    h.finish()
}

type Cache = Option<(PathBuf, u64, Arc<Vec<LoadedSkin>>)>;
static CACHE: Mutex<Cache> = Mutex::new(None);

/// Every skin folder (valid or not), sorted by name. Re-read only when a
/// file changed, so editing a skin shows up on the next page load.
pub fn load_all(dir: &Path) -> Arc<Vec<LoadedSkin>> {
    let fp = fingerprint(dir);
    if let Some((d, f, skins)) = CACHE.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        if d == dir && *f == fp {
            return skins.clone();
        }
    }
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if !path.is_dir() || name.ends_with(".sample") || name.starts_with('.') {
                continue;
            }
            let skin = load_one(&path, &name);
            if !skin.usable() {
                tracing::warn!(folder = %path.display(), errors = skin.report.errors(), "app skin has errors and is not used");
            }
            out.push(skin);
        }
    }
    out.sort_by(|a, b| {
        let name = |s: &LoadedSkin| s.manifest().map(|m| m.name.to_lowercase()).unwrap_or_else(|| s.folder.clone());
        name(a).cmp(&name(b))
    });
    let skins = Arc::new(out);
    *CACHE.lock().unwrap_or_else(|e| e.into_inner()) = Some((dir.to_path_buf(), fp, skins.clone()));
    skins
}

fn find(state: &AppState, id: &str) -> Option<LoadedSkin> {
    load_all(&state.config.app_skins_dir()).iter().find(|s| s.id() == id).cloned()
}

async fn enabled(state: &AppState) -> bool {
    super::is_enabled(&state.db, MODULE_ID).await
}

/// Usable skins, or none when the module is off.
pub async fn usable(state: &AppState) -> Vec<LoadedSkin> {
    if !enabled(state).await {
        return vec![];
    }
    load_all(&state.config.app_skins_dir()).iter().filter(|s| s.usable()).cloned().collect()
}

// ---------------------------------------------------------------------------
// Bundled players and themes (picked up by the player-skins and themes lists)
// ---------------------------------------------------------------------------

/// Id under which a skin's bundled player is listed.
pub fn player_id(skin_id: &str) -> String {
    format!("{skin_id}-player")
}

pub async fn bundled_players(state: &AppState) -> Vec<PlayerSkin> {
    let mut out = Vec::new();
    for s in usable(state).await {
        let Some(m) = s.manifest() else { continue };
        if m.player.as_deref() != Some("./player") {
            continue;
        }
        let Some(html) = s.source.texts.get("player/skin.html") else { continue };
        let skin = PlayerSkin {
            id: player_id(&m.id),
            name: format!("{} (from the {} app skin)", m.name, m.name),
            author: m.author.clone(),
            html: html.clone(),
            css: s.source.texts.get("player/skin.css").cloned().unwrap_or_default(),
        };
        if skin.validate().is_ok() {
            out.push(skin);
        }
    }
    out
}

pub async fn bundled_themes(state: &AppState) -> Vec<Theme> {
    let mut out = Vec::new();
    for s in usable(state).await {
        for (path, text) in s.source.texts.iter().filter(|(p, _)| p.starts_with("themes/") && p.ends_with(".json")) {
            match serde_json::from_str::<Theme>(text).map_err(|e| e.to_string()).and_then(|t| t.validate().map(|_| t)) {
                Ok(t) => out.push(t),
                Err(e) => tracing::warn!(skin = %s.folder, file = %path, error = %e, "skipping bundled theme"),
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// The bundle the web app loads
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BundlePlayer {
    /// A built-in style id, or the id the bundled player is listed under.
    pub style: String,
    pub bundled: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bundle {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: Option<String>,
    pub skin_api: u32,
    pub devices: Vec<String>,
    pub scheme: Option<String>,
    pub theme: Option<String>,
    pub player: Option<BundlePlayer>,
    pub options: BTreeMap<String, SkinOption>,
    pub option_values: BTreeMap<String, String>,
    /// `shell`, `pages/home`, `pages/item.series.tv`, `partials/card` -> markup.
    pub templates: BTreeMap<String, String>,
    pub css: String,
    pub assets: Vec<String>,
    pub hash: String,
}

fn bundle(skin: &LoadedSkin, option_values: BTreeMap<String, String>) -> Option<Bundle> {
    let m = skin.manifest()?;
    let templates = skin
        .report
        .templates
        .iter()
        .filter_map(|k| skin.source.texts.get(&format!("{k}.html")).map(|t| (k.clone(), t.clone())))
        .collect();
    let mut css = String::new();
    for style in &m.styles {
        if let Some(t) = skin.source.texts.get(style) {
            css.push_str(t);
            css.push('\n');
        }
    }
    let assets = skin
        .source
        .binaries
        .keys()
        .filter_map(|p| p.strip_prefix("assets/"))
        .filter(|n| valid_asset_name(n) && asset_type(n).is_some())
        .map(String::from)
        .collect();
    let player = m.player.as_ref().map(|p| {
        if p == "./player" {
            BundlePlayer { style: player_id(&m.id), bundled: true }
        } else {
            BundlePlayer { style: p.clone(), bundled: false }
        }
    });
    let mut values: BTreeMap<String, String> = m.options.iter().map(|(k, o)| (k.clone(), o.default_string())).collect();
    for (k, v) in option_values {
        if let Some(opt) = m.options.get(&k) {
            if let Ok(v) = opt.check_value(&v) {
                values.insert(k, v);
            }
        }
    }
    let mut h = Sha256::new();
    h.update(skin.hash.as_bytes());
    h.update(serde_json::to_string(&values).unwrap_or_default().as_bytes());
    Some(Bundle {
        id: m.id.clone(),
        name: m.name.clone(),
        version: m.version.clone(),
        author: m.author.clone(),
        skin_api: m.skin_api,
        devices: m.devices.clone(),
        scheme: m.scheme.clone(),
        theme: m.theme.clone(),
        player,
        options: m.options.clone(),
        option_values: values,
        templates,
        css,
        assets,
        hash: hex::encode(&h.finalize()[..8]),
    })
}

async fn option_values(state: &AppState, id: &str) -> BTreeMap<String, String> {
    get_setting(&state.db, &format!("app_skin_options:{id}"))
        .await
        .ok()
        .flatten()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

async fn user_choice_allowed(state: &AppState) -> bool {
    super::flag(&super::settings(&state.db, MODULE_ID).await, "user_choice", false)
}

async fn user_pref(state: &AppState, user_id: &str) -> Option<String> {
    sqlx::query_scalar("SELECT value FROM user_prefs WHERE user_id = ? AND key = ?")
        .bind(user_id)
        .bind(USER_KEY)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
}

/// The skin this viewer sees, if any.
async fn effective(state: &AppState, user: &AuthUser) -> Option<LoadedSkin> {
    let skins = usable(state).await;
    if skins.is_empty() {
        return None;
    }
    let admin_choice = get_setting(&state.db, ACTIVE_KEY).await.ok().flatten().unwrap_or_default();
    let wanted = if user_choice_allowed(state).await {
        match user_pref(state, &user.id).await.as_deref() {
            Some("none") => return None,
            Some(id) => id.to_string(),
            None => admin_choice,
        }
    } else {
        admin_choice
    };
    skins.into_iter().find(|s| s.id() == wanted)
}

fn bundle_response(headers: &HeaderMap, b: Bundle) -> Response {
    let etag = format!("\"{}\"", b.hash);
    if headers.get(header::IF_NONE_MATCH).and_then(|v| v.to_str().ok()) == Some(etag.as_str()) {
        return (StatusCode::NOT_MODIFIED, [(header::ETAG, etag)]).into_response();
    }
    let mut resp = Json(b).into_response();
    if let Ok(v) = HeaderValue::from_str(&etag) {
        resp.headers_mut().insert(header::ETAG, v);
    }
    resp.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    resp
}

/// `GET /api/app-skin/active`: this viewer's skin, or 204 for Kinora's own look.
pub async fn active(State(state): State<AppState>, user: AuthUser, headers: HeaderMap) -> AppResult<Response> {
    let Some(skin) = effective(&state, &user).await else {
        return Ok(StatusCode::NO_CONTENT.into_response());
    };
    let values = option_values(&state, skin.id()).await;
    Ok(bundle(&skin, values)
        .map(|b| bundle_response(&headers, b))
        .unwrap_or_else(|| StatusCode::NO_CONTENT.into_response()))
}

/// `GET /api/app-skins/{id}/bundle`: any installed skin, for an admin's preview.
pub async fn preview_bundle(
    State(state): State<AppState>,
    _admin: AdminUser,
    UrlPath(id): UrlPath<String>,
    headers: HeaderMap,
) -> AppResult<Response> {
    let skin = find(&state, &id).filter(|s| s.usable()).ok_or(AppError::NotFound)?;
    let values = option_values(&state, &id).await;
    bundle(&skin, values).map(|b| bundle_response(&headers, b)).ok_or(AppError::NotFound)
}

// ---------------------------------------------------------------------------
// Assets and preview images
// ---------------------------------------------------------------------------

fn serve_file(path: &Path, mime: &str) -> AppResult<Response> {
    let bytes = std::fs::read(path).map_err(|_| AppError::NotFound)?;
    let mut resp = (
        [(header::CONTENT_TYPE, mime.to_string()), (header::CACHE_CONTROL, "private, max-age=3600".to_string())],
        Body::from(bytes),
    )
        .into_response();
    let h = resp.headers_mut();
    h.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    // Assets are images and fonts: nothing in them may load or run anything.
    h.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("default-src 'none'; style-src 'unsafe-inline'"),
    );
    Ok(resp)
}

/// `GET /api/app-skins/{id}/assets/{file}`
pub async fn asset(
    State(state): State<AppState>,
    _user: AuthUser,
    UrlPath((id, file)): UrlPath<(String, String)>,
) -> AppResult<Response> {
    if !valid_skin_id(&id) || !valid_asset_name(&file) {
        return Err(AppError::NotFound);
    }
    let mime = asset_type(&file).ok_or(AppError::NotFound)?;
    let skin = find(&state, &id).ok_or(AppError::NotFound)?;
    if !skin.source.binaries.contains_key(&format!("assets/{file}")) {
        return Err(AppError::NotFound);
    }
    serve_file(&skin.dir.join("assets").join(&file), mime)
}

/// `GET /api/app-skins/{id}/preview`: the skin's preview.png for the picker.
pub async fn preview_image(
    State(state): State<AppState>,
    _user: AuthUser,
    UrlPath(id): UrlPath<String>,
) -> AppResult<Response> {
    let skin = find(&state, &id).ok_or(AppError::NotFound)?;
    if !skin.source.binaries.contains_key("preview.png") {
        return Err(AppError::NotFound);
    }
    serve_file(&skin.dir.join("preview.png"), "image/png")
}

// ---------------------------------------------------------------------------
// Runtime errors reported by browsers
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeError {
    pub page: String,
    pub message: String,
    #[serde(default)]
    pub at: i64,
    #[serde(default)]
    pub count: u32,
}

static RUNTIME_ERRORS: Mutex<Option<HashMap<String, VecDeque<RuntimeError>>>> = Mutex::new(None);

fn runtime_errors(id: &str) -> Vec<RuntimeError> {
    RUNTIME_ERRORS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .and_then(|m| m.get(id))
        .map(|q| q.iter().cloned().collect())
        .unwrap_or_default()
}

#[derive(Deserialize)]
pub struct ErrorReport {
    skin: String,
    page: String,
    message: String,
}

/// `POST /api/app-skins/errors`: a page of a skin failed to render in a browser.
/// Kept in memory (last 20 distinct per skin), shown on the admin's skin card.
pub async fn report_error(
    State(state): State<AppState>,
    _user: AuthUser,
    Json(r): Json<ErrorReport>,
) -> AppResult<StatusCode> {
    if !valid_skin_id(&r.skin) || find(&state, &r.skin).is_none() {
        return Ok(StatusCode::NO_CONTENT);
    }
    let clip = |s: &str, n: usize| s.chars().take(n).collect::<String>();
    let page = clip(&r.page, 80);
    let message = clip(&r.message, 300);
    let mut guard = RUNTIME_ERRORS.lock().unwrap_or_else(|e| e.into_inner());
    let queue = guard.get_or_insert_with(HashMap::new).entry(r.skin).or_default();
    if let Some(e) = queue.iter_mut().find(|e| e.page == page && e.message == message) {
        e.count = e.count.saturating_add(1);
        e.at = crate::db::now_ms();
    } else {
        queue.push_back(RuntimeError { page, message, at: crate::db::now_ms(), count: 1 });
        while queue.len() > 20 {
            queue.pop_front();
        }
    }
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Admin
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct SkinSummary {
    pub id: String,
    pub folder: String,
    pub name: String,
    pub version: Option<String>,
    pub author: Option<String>,
    pub description: Option<String>,
    pub homepage: Option<String>,
    pub skin_api: Option<u32>,
    pub devices: Vec<String>,
    pub usable: bool,
    pub theme: Option<String>,
    pub scheme: Option<String>,
    pub player: Option<String>,
    pub has_preview: bool,
    pub readme: Option<String>,
    pub options: BTreeMap<String, SkinOption>,
    pub option_values: BTreeMap<String, String>,
    pub templates: Vec<String>,
    pub disabled: Vec<String>,
    pub issues: Vec<Issue>,
    pub runtime_errors: Vec<RuntimeError>,
}

#[derive(Serialize)]
pub struct SkinList {
    pub enabled: bool,
    pub active: String,
    pub user_choice: bool,
    pub skin_api: u32,
    pub supported: Vec<u32>,
    pub skins: Vec<SkinSummary>,
    /// Starter skins that can be added with one click.
    pub starters: Vec<String>,
}

async fn summary(state: &AppState, s: &LoadedSkin) -> SkinSummary {
    let m = s.manifest();
    let mut values = option_values(state, s.id()).await;
    if let Some(m) = m {
        for (k, o) in &m.options {
            values.entry(k.clone()).or_insert_with(|| o.default_string());
        }
    }
    SkinSummary {
        id: s.id().to_string(),
        folder: s.folder.clone(),
        name: m.map(|m| m.name.clone()).unwrap_or_else(|| s.folder.clone()),
        version: m.map(|m| m.version.clone()),
        author: m.and_then(|m| m.author.clone()),
        description: m.and_then(|m| m.description.clone()),
        homepage: m.and_then(|m| m.homepage.clone()),
        skin_api: m.map(|m| m.skin_api),
        devices: m.map(|m| m.devices.clone()).unwrap_or_default(),
        usable: s.usable(),
        theme: m.and_then(|m| m.theme.clone()),
        scheme: m.and_then(|m| m.scheme.clone()),
        player: m.and_then(|m| m.player.clone()),
        has_preview: s.source.binaries.contains_key("preview.png"),
        readme: s.source.texts.get("README.md").map(|t| t.chars().take(20_000).collect()),
        options: m.map(|m| m.options.clone()).unwrap_or_default(),
        option_values: values,
        templates: s.report.templates.clone(),
        disabled: s.report.disabled.clone(),
        issues: s.report.issues.clone(),
        runtime_errors: runtime_errors(s.id()),
    }
}

/// `GET /api/app-skins`: every installed skin with its check results.
pub async fn list(State(state): State<AppState>, _admin: AdminUser) -> AppResult<Json<SkinList>> {
    let mut skins = Vec::new();
    for s in load_all(&state.config.app_skins_dir()).iter() {
        skins.push(summary(&state, s).await);
    }
    let r = rules();
    Ok(Json(SkinList {
        enabled: enabled(&state).await,
        active: get_setting(&state.db, ACTIVE_KEY).await?.unwrap_or_default(),
        user_choice: user_choice_allowed(&state).await,
        skin_api: r.skin_api,
        supported: r.supported.clone(),
        skins,
        starters: samples().into_iter().map(|(n, _)| n.to_string()).collect(),
    }))
}

#[derive(Deserialize)]
pub struct ActiveUpdate {
    /// "" for Kinora's own look.
    skin: String,
    /// Option values for that skin (only the keys given are changed).
    #[serde(default)]
    options: BTreeMap<String, String>,
}

/// Check and store option values for a skin (only the keys given change).
async fn save_options(state: &AppState, skin: &LoadedSkin, options: &BTreeMap<String, String>) -> AppResult<()> {
    if options.is_empty() {
        return Ok(());
    }
    let m = skin.manifest().ok_or(AppError::NotFound)?;
    let mut values = option_values(state, skin.id()).await;
    for (k, v) in options {
        let opt = m.options.get(k).ok_or_else(|| AppError::BadRequest(format!("unknown option {k}")))?;
        let v = opt.check_value(v).map_err(|e| AppError::BadRequest(format!("{}: {e}", opt.label)))?;
        values.insert(k.clone(), v);
    }
    set_setting(
        &state.db,
        &format!("app_skin_options:{}", skin.id()),
        &serde_json::to_string(&values).unwrap_or_default(),
    )
    .await?;
    Ok(())
}

/// `PUT /api/app-skin`: the skin everyone uses, and its options.
pub async fn put_active(
    State(state): State<AppState>,
    _admin: AdminUser,
    Json(body): Json<ActiveUpdate>,
) -> AppResult<Json<serde_json::Value>> {
    let id = body.skin.trim();
    if !id.is_empty() {
        let skin = find(&state, id).ok_or_else(|| AppError::BadRequest(format!("no skin called {id:?}")))?;
        if !skin.usable() {
            return Err(AppError::BadRequest("that skin has errors; fix them first".into()));
        }
        save_options(&state, &skin, &body.options).await?;
    }
    set_setting(&state.db, ACTIVE_KEY, id).await?;
    Ok(Json(serde_json::json!({ "skin": id })))
}

/// `PUT /api/app-skins/{id}/options`: change a skin's options without switching to it.
pub async fn put_options(
    State(state): State<AppState>,
    _admin: AdminUser,
    UrlPath(id): UrlPath<String>,
    Json(options): Json<BTreeMap<String, String>>,
) -> AppResult<StatusCode> {
    let skin = find(&state, &id).ok_or(AppError::NotFound)?;
    save_options(&state, &skin, &options).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /api/app-skins/starters/{name}`: copy a starter skin in as a real skin
/// (as `<name>`, or `<name>-2` and so on when that's taken).
pub async fn add_starter(
    State(state): State<AppState>,
    _admin: AdminUser,
    UrlPath(name): UrlPath<String>,
) -> AppResult<Json<serde_json::Value>> {
    if !samples().iter().any(|(n, _)| *n == name) {
        return Err(AppError::NotFound);
    }
    let dir = state.config.app_skins_dir();
    let taken = |id: &str| dir.join(id).exists() || load_all(&dir).iter().any(|s| s.id() == id);
    let id = (1..100)
        .map(|n| if n == 1 { name.clone() } else { format!("{name}-{n}") })
        .find(|id| !taken(id))
        .ok_or_else(|| AppError::BadRequest("too many copies of that starter".into()))?;
    write_starter(&dir, &name, &id, Some(&id))?;
    Ok(Json(serde_json::json!({ "id": id })))
}

/// `DELETE /api/app-skins/{id}`: remove an installed skin folder.
pub async fn remove(
    State(state): State<AppState>,
    _admin: AdminUser,
    UrlPath(id): UrlPath<String>,
) -> AppResult<StatusCode> {
    let skin = find(&state, &id).ok_or(AppError::NotFound)?;
    let root = state.config.app_skins_dir();
    // Only ever a direct child of the skins folder.
    if skin.dir.parent() != Some(root.as_path()) {
        return Err(AppError::Forbidden);
    }
    std::fs::remove_dir_all(&skin.dir)?;
    forget(&state, &id).await?;
    // If it came from the registry, it is no longer installed.
    super::registry::forget_missing(&state).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Upload: files are sent one at a time, then checked and moved into place.
// ---------------------------------------------------------------------------

fn valid_token(t: &str) -> bool {
    (8..=64).contains(&t.len()) && t.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// A safe relative path: up to 4 parts of letters, digits, dot, dash, underscore and space.
fn safe_rel_path(p: &str) -> Option<PathBuf> {
    let parts: Vec<&str> = p.split('/').filter(|s| !s.is_empty()).collect();
    if parts.is_empty() || parts.len() > 4 {
        return None;
    }
    let mut out = PathBuf::new();
    for part in parts {
        if part.starts_with('.')
            || part.len() > 100
            || !part.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ' '))
        {
            return None;
        }
        out.push(part);
    }
    Some(out)
}

fn upload_dir(state: &AppState, token: &str) -> PathBuf {
    state.config.app_skins_dir().join(format!(".upload-{token}"))
}

fn dir_usage(dir: &Path) -> (usize, u64) {
    walkdir::WalkDir::new(dir)
        .into_iter()
        .flatten()
        .filter(|e| e.file_type().is_file())
        .fold((0, 0), |(n, b), e| (n + 1, b + e.metadata().map(|m| m.len()).unwrap_or(0)))
}

/// `PUT /api/app-skins/upload/{token}/{*path}`: one file of a skin being uploaded.
pub async fn upload_file(
    State(state): State<AppState>,
    _admin: AdminUser,
    UrlPath((token, path)): UrlPath<(String, String)>,
    body: Bytes,
) -> AppResult<StatusCode> {
    if !valid_token(&token) {
        return Err(AppError::BadRequest("invalid upload id".into()));
    }
    let rel = safe_rel_path(&path).ok_or_else(|| AppError::BadRequest(format!("invalid file name {path:?}")))?;
    let dir = upload_dir(&state, &token);
    std::fs::create_dir_all(&dir)?;
    let (files, bytes) = dir_usage(&dir);
    if files + 1 > MAX_UPLOAD_FILES || bytes + body.len() as u64 > MAX_UPLOAD_BYTES {
        let _ = std::fs::remove_dir_all(&dir);
        return Err(AppError::BadRequest("the skin is too big (too many files or too many bytes)".into()));
    }
    let target = dir.join(rel);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(target, &body)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct FinishQuery {
    #[serde(default)]
    replace: bool,
}

#[derive(Serialize)]
pub struct UploadResult {
    id: String,
    replaced: bool,
    issues: Vec<Issue>,
}

/// `POST /api/app-skins/finish/{token}`: check the uploaded files and install them.
pub async fn upload_finish(
    State(state): State<AppState>,
    _admin: AdminUser,
    UrlPath(token): UrlPath<String>,
    Query(q): Query<FinishQuery>,
) -> AppResult<Response> {
    if !valid_token(&token) {
        return Err(AppError::BadRequest("invalid upload id".into()));
    }
    let dir = upload_dir(&state, &token);
    if !dir.is_dir() {
        return Err(AppError::BadRequest("nothing was uploaded".into()));
    }
    let result = install_from(&state, &dir, q.replace).await;
    let _ = std::fs::remove_dir_all(&dir);
    match result {
        Ok(r) => Ok(Json(r).into_response()),
        Err(InstallError::Invalid(issues)) => Ok((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({ "error": "the skin has errors", "issues": issues })),
        )
            .into_response()),
        Err(InstallError::App(e)) => Err(e),
    }
}

enum InstallError {
    Invalid(Vec<Issue>),
    App(AppError),
}

impl From<AppError> for InstallError {
    fn from(e: AppError) -> Self {
        InstallError::App(e)
    }
}

async fn install_from(state: &AppState, upload: &Path, replace: bool) -> Result<UploadResult, InstallError> {
    // The skin is either at the top or inside one folder (a zipped folder).
    let root = if upload.join("skin.json").is_file() {
        upload.to_path_buf()
    } else {
        let dirs: Vec<PathBuf> = std::fs::read_dir(upload)
            .map_err(AppError::from)?
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        match dirs.as_slice() {
            [one] if one.join("skin.json").is_file() => one.clone(),
            _ => return Err(AppError::BadRequest("no skin.json found at the top of the upload".into()).into()),
        }
    };
    let manifest: AppSkinManifest = std::fs::read_to_string(root.join("skin.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .ok_or_else(|| AppError::BadRequest("skin.json is missing or invalid".into()))?;
    if !valid_skin_id(&manifest.id) {
        return Err(AppError::BadRequest(format!("invalid skin id {:?}", manifest.id)).into());
    }
    let source = read_source(&root, &manifest.id).map_err(AppError::BadRequest)?;
    let report = check_skin(&source);
    if !report.valid {
        return Err(InstallError::Invalid(report.issues));
    }
    let target = state.config.app_skins_dir().join(&manifest.id);
    let replaced = target.exists();
    if replaced && !replace {
        return Err(AppError::Conflict(format!("a skin called {:?} is already installed", manifest.id)).into());
    }
    if replaced {
        std::fs::remove_dir_all(&target).map_err(AppError::from)?;
    }
    std::fs::rename(&root, &target).map_err(AppError::from)?;
    Ok(UploadResult { id: manifest.id, replaced, issues: report.issues })
}

// ---------------------------------------------------------------------------
// Viewers' own choice
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct Choice {
    id: String,
    name: String,
    description: Option<String>,
    has_preview: bool,
}

#[derive(Serialize)]
pub struct MySkin {
    /// Whether viewers may pick (module setting).
    allowed: bool,
    /// The server's skin ("" for none).
    server: String,
    /// This viewer's pick: null = follow the server, "none", or a skin id.
    mine: Option<String>,
    choices: Vec<Choice>,
}

/// `GET /api/me/app-skin`
pub async fn get_mine(State(state): State<AppState>, user: AuthUser) -> AppResult<Json<MySkin>> {
    let allowed = enabled(&state).await && user_choice_allowed(&state).await;
    let choices = if allowed {
        usable(&state)
            .await
            .iter()
            .filter_map(|s| {
                s.manifest().map(|m| Choice {
                    id: m.id.clone(),
                    name: m.name.clone(),
                    description: m.description.clone(),
                    has_preview: s.source.binaries.contains_key("preview.png"),
                })
            })
            .collect()
    } else {
        vec![]
    };
    Ok(Json(MySkin {
        allowed,
        server: get_setting(&state.db, ACTIVE_KEY).await?.unwrap_or_default(),
        mine: user_pref(&state, &user.id).await,
        choices,
    }))
}

#[derive(Deserialize)]
pub struct MineUpdate {
    /// null = follow the server, "none" = Kinora's own look, or a skin id.
    skin: Option<String>,
}

/// `PUT /api/me/app-skin`
pub async fn put_mine(
    State(state): State<AppState>,
    user: AuthUser,
    Json(body): Json<MineUpdate>,
) -> AppResult<StatusCode> {
    if !(enabled(&state).await && user_choice_allowed(&state).await) {
        return Err(AppError::Forbidden);
    }
    match body.skin.as_deref().map(str::trim) {
        None | Some("") => {
            sqlx::query("DELETE FROM user_prefs WHERE user_id = ? AND key = ?")
                .bind(&user.id)
                .bind(USER_KEY)
                .execute(&state.db)
                .await?;
        }
        Some(v) => {
            if v != "none" && !usable(&state).await.iter().any(|s| s.id() == v) {
                return Err(AppError::BadRequest(format!("no skin called {v:?}")));
            }
            sqlx::query(
                "INSERT INTO user_prefs (user_id, key, value) VALUES (?, ?, ?)
                 ON CONFLICT(user_id, key) DO UPDATE SET value = excluded.value",
            )
            .bind(&user.id)
            .bind(USER_KEY)
            .bind(v)
            .execute(&state.db)
            .await?;
        }
    }
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Command line: `kinora skin new|check`
// ---------------------------------------------------------------------------

const CLI_HELP: &str = "Usage:
  kinora skin new <id> [--dir <folder>]
      Create a new app skin folder <folder>/<id> from the Blank starter.
  kinora skin check <folder> [--json]
      Check a skin folder the same way the server does. Exits with 1 on errors.
";

/// Runs `kinora skin …`. Returns the process exit code.
pub fn cli(args: &[String]) -> i32 {
    let flag = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    match args.first().map(String::as_str) {
        Some("new") => {
            let Some(id) = args.get(1).filter(|a| !a.starts_with("--")) else {
                eprint!("{CLI_HELP}");
                return 2;
            };
            if !valid_skin_id(id) {
                eprintln!("The id must be lowercase letters, digits and dashes, like \"my-skin\".");
                return 2;
            }
            let from = flag("--from").unwrap_or_else(|| "blank".into());
            let parent = PathBuf::from(flag("--dir").unwrap_or_else(|| ".".into()));
            if parent.join(id).exists() {
                eprintln!("{} already exists.", parent.join(id).display());
                return 1;
            }
            match write_starter(&parent, &from, id, Some(id)) {
                Ok(()) => {
                    let dir = parent.join(id);
                    println!("Created {} from the {from} starter.", dir.display());
                    println!("Next:");
                    println!("  1. Edit skin.json (name, author) and the templates in pages/ and partials/.");
                    println!("  2. Check it:  kinora skin check {}", dir.display());
                    println!("  3. Copy the folder into app-skins/ in your Kinora config directory, then pick it in");
                    println!("     Admin → Modules & themes → App skin. Edits show up on the next page load.");
                    println!("Editor autocomplete: copy .vscode-settings.sample.json into .vscode/settings.json.");
                    0
                }
                Err(e) => {
                    eprintln!("Could not create the skin: {e}");
                    1
                }
            }
        }
        Some("check") => {
            let Some(dir) = args.get(1).filter(|a| !a.starts_with("--")) else {
                eprint!("{CLI_HELP}");
                return 2;
            };
            let dir = PathBuf::from(dir);
            let folder = dir
                .canonicalize()
                .ok()
                .and_then(|p| p.file_name().map(|n| n.to_string_lossy().trim_end_matches(".sample").to_string()))
                .unwrap_or_default();
            let skin = load_one(&dir, &folder);
            let json = args.iter().any(|a| a == "--json");
            if json {
                println!("{}", serde_json::to_string_pretty(&skin.report).unwrap_or_default());
            } else {
                for issue in &skin.report.issues {
                    println!("{issue}");
                }
                let r = &skin.report;
                println!(
                    "\n{}: {} error(s), {} warning(s). Templates used: {}{}",
                    if skin.usable() { "OK" } else { "NOT USABLE" },
                    r.errors(),
                    r.warnings(),
                    if r.templates.is_empty() { "none".to_string() } else { r.templates.join(", ") },
                    if r.disabled.is_empty() {
                        String::new()
                    } else {
                        format!(". Disabled by errors: {}", r.disabled.join(", "))
                    }
                );
            }
            if skin.usable() && skin.report.errors() == 0 {
                0
            } else {
                1
            }
        }
        _ => {
            eprint!("{CLI_HELP}");
            2
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starters_are_clean() {
        for (name, _) in samples() {
            let dir = tempfile::tempdir().unwrap();
            write_starter(dir.path(), name, name, None).unwrap();
            let skin = load_one(&dir.path().join(name), name);
            let issues: Vec<String> = skin.report.issues.iter().map(|i| i.to_string()).collect();
            assert!(skin.usable(), "{name}: {issues:?}");
            assert!(issues.is_empty(), "{name} should have no warnings: {issues:?}");
            assert!(skin.report.templates.contains(&"shell".to_string()), "{name}");
        }
    }

    #[test]
    fn samples_are_not_loaded_and_bad_skins_are_listed_but_unusable() {
        let dir = tempfile::tempdir().unwrap();
        prepare_dir(dir.path());
        assert!(dir.path().join("blank.sample/skin.json").exists());
        assert!(load_all(dir.path()).is_empty());
        let bad = dir.path().join("bad");
        std::fs::create_dir_all(&bad).unwrap();
        std::fs::write(bad.join("skin.json"), r#"{"id":"bad","name":"Bad","skinApi":99}"#).unwrap();
        let all = load_all(dir.path());
        assert_eq!(all.len(), 1);
        assert!(!all[0].usable());
    }

    #[test]
    fn new_skin_gets_its_id() {
        let dir = tempfile::tempdir().unwrap();
        write_starter(dir.path(), "marquee", "neon-nights", Some("neon-nights")).unwrap();
        let skin = load_one(&dir.path().join("neon-nights"), "neon-nights");
        assert!(skin.usable(), "{:?}", skin.report.issues);
        assert_eq!(skin.id(), "neon-nights");
        let b = bundle(&skin, BTreeMap::new()).unwrap();
        assert!(b.templates.contains_key("pages/home"));
        assert!(!b.css.is_empty());
    }

    #[test]
    fn options_are_validated_in_the_bundle() {
        let dir = tempfile::tempdir().unwrap();
        write_starter(dir.path(), "marquee", "marquee", None).unwrap();
        let skin = load_one(&dir.path().join("marquee"), "marquee");
        let m = skin.manifest().unwrap();
        let key = m.options.keys().next().unwrap().clone();
        let b = bundle(&skin, [(key.clone(), "definitely not valid".to_string())].into()).unwrap();
        assert_eq!(b.option_values[&key], m.options[&key].default_string());
    }

    #[test]
    fn safe_paths() {
        assert!(safe_rel_path("pages/home.html").is_some());
        assert!(safe_rel_path("my skin/assets/a.woff2").is_some());
        assert!(safe_rel_path("../x").is_none());
        assert!(safe_rel_path(".hidden").is_none());
        assert!(safe_rel_path("a/b/c/d/e").is_none());
        assert!(safe_rel_path("a\\b").is_none());
        assert!(valid_token("abcdef12-3456"));
        assert!(!valid_token("../../x"));
    }
}
