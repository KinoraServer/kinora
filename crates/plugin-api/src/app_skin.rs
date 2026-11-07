//! App skins: whole-app markup + CSS (module "app-skins", docs/app-skins.md).
//!
//! The contract lives in `skin-rules.json` (allowed tags and attributes, the
//! `data-kn-*` bindings, the page view models). The web app reads the same
//! file, so the server's checks and the browser's sanitiser can't drift.
//!
//! [`check_skin`] is pure: it takes the files of a skin folder and returns the
//! parsed manifest plus a list of issues with file, line and column. Errors in
//! the manifest make the whole skin invalid; an error in one template disables
//! only that file (the web app then shows Kinora's own page there). Warnings
//! never disable anything. The server loader and `kinora skin check` both use it.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The rules file, shared with the web app.
pub const SKIN_RULES_JSON: &str = include_str!("../skin-rules.json");

/// Built-in player styles a skin may point at with `"player"`.
pub const BUILTIN_PLAYER_STYLES: &[&str] = &["minimal", "cinematic", "floating", "pro"];

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rules {
    pub skin_api: u32,
    pub supported: Vec<u32>,
    pub limits: Limits,
    pub devices: Vec<String>,
    pub pages: BTreeMap<String, PageRule>,
    pub asset_types: BTreeMap<String, String>,
    pub tags: Vec<String>,
    pub tags_need_binding: BTreeMap<String, String>,
    pub void_tags: Vec<String>,
    pub attributes: AttributeRules,
    pub bindings: BTreeMap<String, BindingRule>,
    pub renamed: Renamed,
    pub image_kinds: Vec<String>,
    pub image_widths: Vec<u32>,
    pub loop_helpers: Vec<String>,
    pub types: BTreeMap<String, BTreeMap<String, String>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Limits {
    pub text_bytes: u64,
    pub asset_bytes: u64,
    pub files: usize,
    pub partial_depth: usize,
    pub card_elements: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PageRule {
    pub scope: String,
    pub variants: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttributeRules {
    pub global: Vec<String>,
    pub prefixes: Vec<String>,
    pub per_tag: BTreeMap<String, Vec<String>>,
    pub svg: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BindingRule {
    pub value: String,
    #[serde(default)]
    pub values: Vec<String>,
    #[serde(default)]
    pub help: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Renamed {
    pub bindings: BTreeMap<String, String>,
    pub fields: BTreeMap<String, String>,
}

/// The parsed rules (parsed once).
pub fn rules() -> &'static Rules {
    static RULES: OnceLock<Rules> = OnceLock::new();
    RULES.get_or_init(|| serde_json::from_str(SKIN_RULES_JSON).expect("skin-rules.json is valid"))
}

// ---------------------------------------------------------------------------
// Manifest
// ---------------------------------------------------------------------------

/// `skin.json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppSkinManifest {
    pub id: String,
    pub name: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub homepage: Option<String>,
    pub skin_api: u32,
    #[serde(default = "default_styles")]
    pub styles: Vec<String>,
    #[serde(default)]
    pub devices: Vec<String>,
    /// "./player" (bundled), a built-in style id, or none.
    #[serde(default)]
    pub player: Option<String>,
    /// Suggested theme id (may be one bundled in `themes/`).
    #[serde(default)]
    pub theme: Option<String>,
    #[serde(default)]
    pub scheme: Option<String>,
    #[serde(default)]
    pub options: BTreeMap<String, SkinOption>,
}

fn default_version() -> String {
    "0.0.0".into()
}

fn default_styles() -> Vec<String> {
    vec!["skin.css".into()]
}

/// An admin-tunable knob. Applied as `data-opt-<key>` and `--opt-<key>`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SkinOption {
    #[serde(rename = "type")]
    pub kind: String,
    pub label: String,
    #[serde(default)]
    pub default: Option<Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
}

pub fn valid_skin_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !id.starts_with('-')
}

impl SkinOption {
    /// Check a value an admin wants to save (as a string). Returns the normalised value.
    pub fn check_value(&self, v: &str) -> Result<String, String> {
        let v = v.trim();
        match self.kind.as_str() {
            "bool" if v == "true" || v == "false" => Ok(v.into()),
            "bool" => Err("must be true or false".into()),
            "enum" if self.values.iter().any(|x| x == v) => Ok(v.into()),
            "enum" => Err(format!("must be one of {}", self.values.join(", "))),
            "number" => {
                let n: f64 = v.parse().map_err(|_| "must be a number".to_string())?;
                if self.min.is_some_and(|m| n < m) || self.max.is_some_and(|m| n > m) {
                    return Err("is out of range".into());
                }
                Ok(v.into())
            }
            "color" if is_css_color(v) => Ok(v.into()),
            "color" => Err("must be a CSS colour".into()),
            _ => Err("unknown option type".into()),
        }
    }

    pub fn default_string(&self) -> String {
        match &self.default {
            Some(Value::String(s)) => s.clone(),
            Some(Value::Bool(b)) => b.to_string(),
            Some(Value::Number(n)) => n.to_string(),
            _ => match self.kind.as_str() {
                "bool" => "false".into(),
                "enum" => self.values.first().cloned().unwrap_or_default(),
                "number" => "0".into(),
                _ => String::new(),
            },
        }
    }
}

/// Empty (use the theme), `#hex`, a colour keyword, or a colour function.
fn is_css_color(v: &str) -> bool {
    v.is_empty()
        || (v.starts_with('#') && matches!(v.len(), 4 | 5 | 7 | 9) && v[1..].chars().all(|c| c.is_ascii_hexdigit()))
        || (v.len() <= 32 && v.chars().all(|c| c.is_ascii_alphabetic()))
        || is_css_color_fn(v)
}

/// `rgb(...)`, `oklch(...)` and friends, without nested calls or breakouts.
fn is_css_color_fn(v: &str) -> bool {
    let lower = v.to_ascii_lowercase();
    let ok_prefix = ["rgb(", "rgba(", "hsl(", "hsla(", "oklch(", "oklab(", "lab(", "lch(", "color("]
        .iter()
        .any(|p| lower.starts_with(p));
    ok_prefix
        && lower.ends_with(')')
        && lower.matches('(').count() == 1
        && !lower.contains([';', '{', '}', '<', '>'])
        && !lower.contains("url")
}

// ---------------------------------------------------------------------------
// Issues and the report
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
pub struct Issue {
    pub file: String,
    pub line: u32,
    pub col: u32,
    pub severity: Severity,
    pub message: String,
}

impl std::fmt::Display for Issue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let sev = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        if self.line > 0 {
            write!(f, "{}:{}:{}: {sev}: {}", self.file, self.line, self.col, self.message)
        } else {
            write!(f, "{}: {sev}: {}", self.file, self.message)
        }
    }
}

/// The files of one skin folder, read by the caller.
#[derive(Debug, Clone, Default)]
pub struct SkinSource {
    /// Folder name (must equal the manifest id).
    pub folder: String,
    /// Text files by path relative to the folder, `/`-separated:
    /// `skin.json`, `shell.html`, `pages/home.html`, `partials/card.html`,
    /// `*.css`, `player/skin.html`, `themes/x.json`, `README.md`.
    pub texts: BTreeMap<String, String>,
    /// Binary files (`assets/…`, `preview.png`) with their sizes.
    pub binaries: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct SkinReport {
    pub manifest: Option<AppSkinManifest>,
    /// The skin can be used (manifest fine, within limits, contract supported).
    pub valid: bool,
    pub issues: Vec<Issue>,
    /// Templates that have errors and are left out of the bundle.
    pub disabled: Vec<String>,
    /// Templates that are used: `shell`, `pages/home`, `pages/item.series.tv`, `partials/card`.
    pub templates: Vec<String>,
}

impl SkinReport {
    pub fn errors(&self) -> usize {
        self.issues.iter().filter(|i| i.severity == Severity::Error).count()
    }
    pub fn warnings(&self) -> usize {
        self.issues.iter().filter(|i| i.severity == Severity::Warning).count()
    }
}

struct Collector {
    issues: Vec<Issue>,
}

impl Collector {
    fn push(&mut self, file: &str, pos: (u32, u32), severity: Severity, message: impl Into<String>) {
        let issue = Issue { file: file.into(), line: pos.0, col: pos.1, severity, message: message.into() };
        if !self.issues.contains(&issue) {
            self.issues.push(issue);
        }
    }
    fn err(&mut self, file: &str, pos: (u32, u32), message: impl Into<String>) {
        self.push(file, pos, Severity::Error, message);
    }
    fn warn(&mut self, file: &str, pos: (u32, u32), message: impl Into<String>) {
        self.push(file, pos, Severity::Warning, message);
    }
}

/// Template key for a file path: `pages/home.tv.html` -> `pages/home.tv`.
pub fn template_key(path: &str) -> Option<String> {
    let stem = path.strip_suffix(".html")?;
    if stem == "shell" || stem.starts_with("pages/") || stem.starts_with("partials/") {
        Some(stem.to_string())
    } else {
        None
    }
}

/// `pages/item.series.tv` -> ("item", Some("series"), Some("tv")). None for unknown names.
pub fn parse_page_key(key: &str) -> Option<(String, Option<String>, Option<String>)> {
    let r = rules();
    let rest = key.strip_prefix("pages/").or(if key == "shell" { Some("shell") } else { None })?;
    let mut parts = rest.split('.');
    let page = parts.next()?.to_string();
    let rule = r.pages.get(&page)?;
    let mut variant = None;
    let mut device = None;
    for p in parts {
        if variant.is_none() && device.is_none() && rule.variants.iter().any(|v| v == p) {
            variant = Some(p.to_string());
        } else if device.is_none() && r.devices.iter().any(|d| d == p) {
            device = Some(p.to_string());
        } else {
            return None;
        }
    }
    Some((page, variant, device))
}

/// Run every check on a skin folder.
pub fn check_skin(src: &SkinSource) -> SkinReport {
    let r = rules();
    let mut c = Collector { issues: Vec::new() };
    let mut report = SkinReport::default();

    // --- limits ---
    let text_bytes: u64 = src.texts.values().map(|t| t.len() as u64).sum();
    let asset_bytes: u64 = src.binaries.values().sum();
    let file_count = src.texts.len() + src.binaries.len();
    if text_bytes > r.limits.text_bytes {
        c.err(
            "",
            (0, 0),
            format!("templates and CSS are {} KB; the limit is {} KB", text_bytes / 1024, r.limits.text_bytes / 1024),
        );
    }
    if asset_bytes > r.limits.asset_bytes {
        c.err(
            "assets/",
            (0, 0),
            format!("assets are {} KB; the limit is {} KB", asset_bytes / 1024, r.limits.asset_bytes / 1024),
        );
    }
    if file_count > r.limits.files {
        c.err("", (0, 0), format!("{file_count} files; the limit is {}", r.limits.files));
    }

    // --- manifest ---
    let manifest = match src.texts.get("skin.json") {
        None => {
            c.err("skin.json", (0, 0), "missing skin.json");
            None
        }
        Some(text) => match serde_json::from_str::<AppSkinManifest>(text) {
            Ok(m) => Some(m),
            Err(e) => {
                c.err("skin.json", (e.line() as u32, e.column() as u32), format!("invalid skin.json: {e}"));
                None
            }
        },
    };
    if let Some(m) = &manifest {
        check_manifest(m, src, &mut c);
    }

    // --- assets ---
    for path in src.binaries.keys() {
        if let Some(name) = path.strip_prefix("assets/") {
            if !valid_asset_name(name) {
                c.warn(
                    path,
                    (0, 0),
                    "asset names may only use letters, digits, dot, dash and underscore; this file is ignored",
                );
            } else if asset_type(name).is_none() {
                c.warn(
                    path,
                    (0, 0),
                    "this file type isn't served (fonts: woff2/woff; images: webp, avif, png, jpg, svg, gif)",
                );
            }
        } else if path != "preview.png" {
            c.warn(path, (0, 0), "binary files belong in assets/; this file is ignored");
        }
    }

    // --- templates ---
    let partials: BTreeSet<String> = src
        .texts
        .keys()
        .filter_map(|p| template_key(p))
        .filter_map(|k| k.strip_prefix("partials/").map(String::from))
        .collect();
    for (path, text) in &src.texts {
        let Some(key) = template_key(path) else {
            if path.ends_with(".html") && !path.starts_with("player/") {
                c.warn(
                    path,
                    (0, 0),
                    "not a known template name (shell.html, pages/<page>.html, partials/<name>.html); ignored",
                );
            }
            continue;
        };
        let before = c.issues.len();
        if key.starts_with("partials/") {
            let name = key.trim_start_matches("partials/");
            if !valid_partial_name(name) {
                c.warn(path, (0, 0), "partial names may only use lowercase letters, digits and dashes; ignored");
                continue;
            }
            // Checked here without a scope; data paths are checked where it's used.
            lint_template(path, text, None, &partials, src, &mut c, 0);
            if name.starts_with("card") {
                let n = count_elements(text);
                if n > r.limits.card_elements {
                    c.warn(
                        path,
                        (0, 0),
                        format!(
                            "{n} elements in a card; keep cards under {} for smooth scrolling",
                            r.limits.card_elements
                        ),
                    );
                }
            }
        } else {
            let Some((page, _, _)) = parse_page_key(&key) else {
                c.warn(path, (0, 0), "not a known page or variant name; ignored (see docs/app-skins.md)");
                continue;
            };
            let scope = r.pages[&page].scope.clone();
            lint_template(
                path,
                text,
                Some(Scope { stack: vec![scope.clone()], page: Some(scope), in_loop: false }),
                &partials,
                src,
                &mut c,
                0,
            );
            if page == "shell" {
                let outlets = count_attr_value(text, "data-kn-slot", "outlet");
                if outlets != 1 {
                    c.err(
                        path,
                        (0, 0),
                        format!("the shell needs exactly one data-kn-slot=\"outlet\" (found {outlets})"),
                    );
                }
            }
        }
        let has_error = c.issues[before..].iter().any(|i| i.severity == Severity::Error && i.file == *path);
        if has_error {
            report.disabled.push(key);
        } else {
            report.templates.push(key);
        }
    }
    if !report.templates.iter().any(|k| k == "shell" || k.starts_with("pages/")) {
        c.warn("", (0, 0), "no shell or page templates: the skin changes nothing");
    }

    // --- CSS ---
    let styles = manifest.as_ref().map(|m| m.styles.clone()).unwrap_or_else(default_styles);
    for path in &styles {
        if let Some(css) = src.texts.get(path) {
            lint_css(path, css, src, &mut c);
        }
    }
    for path in src.texts.keys().filter(|p| p.ends_with(".css") && !p.starts_with("player/")) {
        if !styles.contains(path) {
            c.warn(path, (0, 0), "not listed in skin.json \"styles\"; not loaded");
        }
    }

    // --- bundled player and themes ---
    if let Some(m) = &manifest {
        if m.player.as_deref() == Some("./player") {
            let html = src.texts.get("player/skin.html").cloned().unwrap_or_default();
            let css = src.texts.get("player/skin.css").cloned().unwrap_or_default();
            let skin =
                crate::PlayerSkin { id: format!("{}-player", m.id), name: m.name.clone(), author: None, html, css };
            if let Err(e) = skin.validate() {
                c.warn("player/skin.html", (0, 0), format!("bundled player skipped: {e}"));
            }
        }
    }
    for (path, text) in src.texts.iter().filter(|(p, _)| p.starts_with("themes/") && p.ends_with(".json")) {
        match serde_json::from_str::<crate::Theme>(text).map_err(|e| e.to_string()).and_then(|t| t.validate()) {
            Ok(()) => {}
            Err(e) => c.warn(path, (0, 0), format!("bundled theme skipped: {e}")),
        }
    }

    report.valid = manifest.is_some()
        && !c.issues.iter().any(|i| {
            i.severity == Severity::Error && (i.file == "skin.json" || i.file.is_empty() || i.file == "assets/")
        });
    report.manifest = manifest;
    c.issues.sort_by(|a, b| (a.severity, &a.file, a.line, a.col).cmp(&(b.severity, &b.file, b.line, b.col)));
    report.issues = c.issues;
    report.templates.sort();
    report.disabled.sort();
    report
}

fn check_manifest(m: &AppSkinManifest, src: &SkinSource, c: &mut Collector) {
    let r = rules();
    let f = "skin.json";
    if !valid_skin_id(&m.id) {
        c.err(f, (0, 0), format!("invalid id {:?} (lowercase letters, digits and dashes)", m.id));
    } else if !src.folder.is_empty() && m.id != src.folder {
        c.err(f, (0, 0), format!("id {:?} must match the folder name {:?}", m.id, src.folder));
    }
    if m.name.trim().is_empty() || m.name.len() > 80 {
        c.err(f, (0, 0), "name must be 1-80 characters");
    }
    if m.version.len() > 32
        || !m.version.split('.').all(|p| !p.is_empty() && p.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '-'))
    {
        c.err(f, (0, 0), format!("version {:?} should look like 1.2.0", m.version));
    }
    for (label, v) in [("author", &m.author), ("description", &m.description)] {
        if v.as_ref().is_some_and(|s| s.len() > 300) {
            c.err(f, (0, 0), format!("{label} is too long (300 characters max)"));
        }
    }
    if let Some(h) = &m.homepage {
        if !(h.starts_with("https://") || h.starts_with("http://")) || h.len() > 300 || h.contains(['<', '>', '"', ' '])
        {
            c.err(f, (0, 0), "homepage must be an http(s) address");
        }
    }
    if !r.supported.contains(&m.skin_api) {
        let msg = if m.skin_api > r.skin_api {
            format!(
                "written for skinApi {}; this Kinora supports {:?}. Update Kinora to use it.",
                m.skin_api, r.supported
            )
        } else {
            format!(
                "written for skinApi {}, which this Kinora no longer supports ({:?}). Ask the author for an update.",
                m.skin_api, r.supported
            )
        };
        c.err(f, (0, 0), msg);
    }
    if m.styles.len() > 8 {
        c.err(f, (0, 0), "at most 8 stylesheets");
    }
    for s in &m.styles {
        if !s.ends_with(".css") || s.contains("..") || s.starts_with('/') {
            c.err(f, (0, 0), format!("style {s:?} must be a .css file inside the skin folder"));
        } else if !src.texts.contains_key(s) {
            c.warn(f, (0, 0), format!("style {s:?} not found"));
        }
    }
    for d in &m.devices {
        if !r.devices.contains(d) {
            c.err(f, (0, 0), format!("unknown device {d:?} (phone, tablet, desktop, tv)"));
        }
    }
    if let Some(p) = &m.player {
        if p == "./player" {
            if !src.texts.contains_key("player/skin.html") {
                c.warn(f, (0, 0), "\"player\": \"./player\" but player/skin.html is missing");
            }
        } else if !BUILTIN_PLAYER_STYLES.contains(&p.as_str()) {
            c.err(f, (0, 0), format!("player must be \"./player\" or one of {}", BUILTIN_PLAYER_STYLES.join(", ")));
        }
    }
    if let Some(t) = &m.theme {
        if !valid_skin_id(t) {
            c.err(f, (0, 0), format!("invalid theme id {t:?}"));
        }
    }
    if let Some(s) = &m.scheme {
        if s != "dark" && s != "light" {
            c.err(f, (0, 0), "scheme must be \"dark\" or \"light\"");
        }
    }
    if m.options.len() > 24 {
        c.err(f, (0, 0), "at most 24 options");
    }
    for (key, opt) in &m.options {
        if !valid_skin_id(key) || key.len() > 32 {
            c.err(f, (0, 0), format!("option name {key:?} must be lowercase letters, digits and dashes"));
        }
        if !["bool", "enum", "number", "color"].contains(&opt.kind.as_str()) {
            c.err(f, (0, 0), format!("option {key:?}: type must be bool, enum, number or color"));
            continue;
        }
        if opt.label.trim().is_empty() || opt.label.len() > 80 {
            c.err(f, (0, 0), format!("option {key:?}: label must be 1-80 characters"));
        }
        if opt.kind == "enum" && (opt.values.is_empty() || opt.values.iter().any(|v| !valid_skin_id(v))) {
            c.err(f, (0, 0), format!("option {key:?}: enum values must be lowercase ids"));
        }
        if let Err(e) = opt.check_value(&opt.default_string()) {
            c.err(f, (0, 0), format!("option {key:?}: default {e}"));
        }
    }
}

pub fn valid_asset_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 100
        && !name.starts_with('.')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
}

/// Content type of an asset by extension.
pub fn asset_type(name: &str) -> Option<&'static str> {
    let ext = name.rsplit_once('.')?.1.to_ascii_lowercase();
    rules().asset_types.get(&ext).map(|s| s.as_str())
}

fn valid_partial_name(name: &str) -> bool {
    valid_skin_id(name)
}

// ---------------------------------------------------------------------------
// A small HTML tokenizer (enough for linting; the browser does the real parsing)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Attr {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone)]
enum Token {
    Start { name: String, attrs: Vec<Attr>, self_closing: bool, offset: usize },
    End { name: String },
    Text { text: String },
}

fn tokenize(src: &str) -> Vec<Token> {
    let b = src.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    let mut text_start = 0;
    let flush = |out: &mut Vec<Token>, from: usize, to: usize| {
        if to > from {
            out.push(Token::Text { text: src[from..to].to_string() });
        }
    };
    while i < b.len() {
        if b[i] != b'<' {
            i += 1;
            continue;
        }
        if src[i..].starts_with("<!--") {
            flush(&mut out, text_start, i);
            i = src[i + 4..].find("-->").map(|p| i + 4 + p + 3).unwrap_or(b.len());
            text_start = i;
            continue;
        }
        if src[i..].starts_with("<!") || src[i..].starts_with("<?") {
            flush(&mut out, text_start, i);
            i = src[i..].find('>').map(|p| i + p + 1).unwrap_or(b.len());
            text_start = i;
            continue;
        }
        let closing = b.get(i + 1) == Some(&b'/');
        let name_start = if closing { i + 2 } else { i + 1 };
        if !b.get(name_start).is_some_and(|c| c.is_ascii_alphabetic()) {
            i += 1;
            continue;
        }
        flush(&mut out, text_start, i);
        let mut j = name_start;
        while j < b.len() && !b[j].is_ascii_whitespace() && b[j] != b'>' && b[j] != b'/' {
            j += 1;
        }
        let name = src[name_start..j].to_ascii_lowercase();
        if closing {
            let end = src[j..].find('>').map(|p| j + p + 1).unwrap_or(b.len());
            out.push(Token::End { name });
            i = end;
            text_start = i;
            continue;
        }
        // attributes
        let mut attrs = Vec::new();
        let mut self_closing = false;
        loop {
            while j < b.len() && b[j].is_ascii_whitespace() {
                j += 1;
            }
            if j >= b.len() {
                break;
            }
            if b[j] == b'>' {
                j += 1;
                break;
            }
            if b[j] == b'/' {
                if b.get(j + 1) == Some(&b'>') {
                    self_closing = true;
                    j += 2;
                    break;
                }
                j += 1;
                continue;
            }
            let an_start = j;
            while j < b.len()
                && !b[j].is_ascii_whitespace()
                && b[j] != b'='
                && b[j] != b'>'
                && !(b[j] == b'/' && b.get(j + 1) == Some(&b'>'))
            {
                j += 1;
            }
            let an = src[an_start..j].to_ascii_lowercase();
            while j < b.len() && b[j].is_ascii_whitespace() {
                j += 1;
            }
            let mut value = String::new();
            if b.get(j) == Some(&b'=') {
                j += 1;
                while j < b.len() && b[j].is_ascii_whitespace() {
                    j += 1;
                }
                if let Some(&q) = b.get(j).filter(|c| **c == b'"' || **c == b'\'') {
                    let vs = j + 1;
                    let ve = src[vs..].find(q as char).map(|p| vs + p).unwrap_or(b.len());
                    value = decode_entities(&src[vs..ve]);
                    j = (ve + 1).min(b.len());
                } else {
                    let vs = j;
                    while j < b.len() && !b[j].is_ascii_whitespace() && b[j] != b'>' {
                        j += 1;
                    }
                    value = decode_entities(&src[vs..j]);
                }
            }
            if !an.is_empty() {
                attrs.push(Attr { name: an, value });
            }
        }
        out.push(Token::Start { name: name.clone(), attrs, self_closing, offset: i });
        i = j;
        text_start = i;
        // Raw text elements: skip to their end tag.
        if matches!(name.as_str(), "script" | "style" | "textarea" | "title" | "xmp") && !self_closing {
            let close = format!("</{name}");
            let lower = src[i..].to_ascii_lowercase();
            if name != "title" {
                let end = lower.find(&close).map(|p| i + p).unwrap_or(b.len());
                i = end;
                text_start = i;
            }
        }
    }
    flush(&mut out, text_start, b.len());
    out
}

fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    s.replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

fn line_col(src: &str, offset: usize) -> (u32, u32) {
    let before = &src[..offset.min(src.len())];
    let line = before.matches('\n').count() as u32 + 1;
    let col = before.rsplit('\n').next().map(|l| l.chars().count()).unwrap_or(0) as u32 + 1;
    (line, col)
}

fn count_elements(html: &str) -> usize {
    tokenize(html).iter().filter(|t| matches!(t, Token::Start { .. })).count()
}

fn count_attr_value(html: &str, attr: &str, value: &str) -> usize {
    tokenize(html)
        .iter()
        .filter(|t| matches!(t, Token::Start { attrs, .. } if attrs.iter().any(|a| a.name == attr && a.value.trim() == value)))
        .count()
}

// ---------------------------------------------------------------------------
// Data paths
// ---------------------------------------------------------------------------

/// Fields of a type, following `$extends`.
fn type_fields(name: &str) -> Option<BTreeMap<String, String>> {
    let r = rules();
    let t = r.types.get(name)?;
    let mut out = BTreeMap::new();
    if let Some(parent) = t.get("$extends") {
        out.extend(type_fields(parent).unwrap_or_default());
    }
    for (k, v) in t {
        if k != "$extends" {
            out.insert(k.clone(), v.clone());
        }
    }
    Some(out)
}

#[derive(Debug, Clone)]
struct Scope {
    /// Type names, innermost last. "any" accepts every path.
    stack: Vec<String>,
    page: Option<String>,
    in_loop: bool,
}

impl Scope {
    /// The type a path resolves to, or an error message.
    fn resolve(&self, path: &str) -> Result<String, String> {
        let r = rules();
        let path = path.trim();
        if path.is_empty() {
            return Err("empty path".into());
        }
        if path == "." {
            return Ok(self.stack.last().cloned().unwrap_or_else(|| "any".into()));
        }
        if path.starts_with('@') {
            if !r.loop_helpers.iter().any(|h| h == path) {
                return Err(format!("unknown loop helper {path:?} ({})", r.loop_helpers.join(", ")));
            }
            if !self.in_loop {
                return Err(format!("{path} only works inside data-kn-each"));
            }
            return Ok(if path == "@first" || path == "@last" { "bool" } else { "number" }.into());
        }
        let mut segs = path.split('.');
        let first = segs.next().unwrap_or_default();
        let mut cur: String = match first {
            "app" => "App".into(),
            "page" => match &self.page {
                Some(p) => p.clone(),
                None => "any".into(),
            },
            _ => {
                let mut found = None;
                for t in self.stack.iter().rev() {
                    if t == "any" {
                        found = Some("any".to_string());
                        break;
                    }
                    if let Some(f) = type_fields(t).and_then(|f| f.get(first).cloned()) {
                        found = Some(f);
                        break;
                    }
                }
                match found {
                    Some(t) => t,
                    None => {
                        return Err(unknown_field(first, self.stack.last().map(|s| s.as_str()).unwrap_or("this page")))
                    }
                }
            }
        };
        for seg in segs {
            if cur == "any" {
                return Ok("any".into());
            }
            let Some(fields) = type_fields(&cur) else {
                return Err(format!("{path:?}: {cur} has no field {seg:?}"));
            };
            match fields.get(seg) {
                Some(t) => cur = t.clone(),
                None => return Err(format!("{path:?}: {} has no field {seg:?}", cur)),
            }
        }
        Ok(cur)
    }

    fn has_type(&self, wanted: &[&str]) -> bool {
        self.stack.iter().any(|t| t == "any" || wanted.contains(&t.as_str()))
    }
}

fn unknown_field(field: &str, within: &str) -> String {
    let r = rules();
    if let Some(new) = r.renamed.fields.get(field) {
        return format!("{field:?} was renamed to {new:?}");
    }
    format!("unknown field {field:?} in {within} (see the view models in docs/app-skins.md)")
}

/// Parse `!a`, `a=b`, `a!=b`, `a`. Returns the path.
fn condition_path(cond: &str) -> &str {
    let c = cond.trim().trim_start_matches('!');
    if let Some((p, _)) = c.split_once("!=") {
        return p.trim();
    }
    if let Some((p, _)) = c.split_once('=') {
        return p.trim();
    }
    c
}

// ---------------------------------------------------------------------------
// Template lint
// ---------------------------------------------------------------------------

struct Frame {
    tag: String,
    pushed: bool,
    was_loop: bool,
}

const URL_ATTRS: &[&str] =
    &["href", "src", "xlink:href", "srcset", "action", "formaction", "poster", "data", "background"];

fn lint_template(
    file: &str,
    src_text: &str,
    start: Option<Scope>,
    partials: &BTreeSet<String>,
    skin: &SkinSource,
    c: &mut Collector,
    depth: usize,
) {
    let r = rules();
    let check_paths = start.is_some();
    let mut scope = start.unwrap_or(Scope { stack: vec!["any".into()], page: None, in_loop: true });
    let mut frames: Vec<Frame> = Vec::new();
    let tokens = tokenize(src_text);
    for tok in &tokens {
        match tok {
            Token::Text { .. } => {}
            Token::End { name } => {
                if let Some(pos) = frames.iter().rposition(|f| &f.tag == name) {
                    while frames.len() > pos {
                        let f = frames.pop().unwrap();
                        if f.pushed {
                            scope.stack.pop();
                            scope.in_loop = f.was_loop;
                        }
                    }
                }
            }
            Token::Start { name, attrs, self_closing, offset } => {
                let pos = line_col(src_text, *offset);
                let get = |n: &str| attrs.iter().find(|a| a.name == n).map(|a| a.value.as_str());
                // Tags
                if !r.tags.contains(name) {
                    c.warn(file, pos, format!("<{name}> isn't allowed in skins and is removed"));
                }
                if let Some(binding) = r.tags_need_binding.get(name) {
                    if get(binding).is_none() {
                        c.warn(file, pos, format!("<{name}> is only kept with {binding}"));
                    }
                }
                if name == "input" && get("data-kn-search").is_some() && get("type").is_some_and(|t| t != "search") {
                    c.warn(file, pos, "data-kn-search needs <input type=\"search\">");
                }
                // Attributes
                for a in attrs {
                    let n = a.name.as_str();
                    if n.starts_with("on") {
                        c.warn(file, pos, format!("event attribute {n} is removed (skins can't run code)"));
                        continue;
                    }
                    if n.starts_with("data-kn-") {
                        continue;
                    }
                    if URL_ATTRS.contains(&n) {
                        let ok =
                            (n == "src" && name == "img" && (a.value.starts_with("asset:") || is_data_image(&a.value)))
                                || ((n == "href" || n == "xlink:href") && name == "use" && a.value.starts_with('#'));
                        if !ok {
                            let hint = if name == "a" && n == "href" {
                                " (use data-kn-link)"
                            } else if name == "img" {
                                " (use data-kn-img or asset:name)"
                            } else {
                                ""
                            };
                            c.warn(file, pos, format!("{n}=\"…\" is removed{hint}"));
                        } else if n == "src" && a.value.starts_with("asset:") {
                            check_asset_ref(file, pos, &a.value[6..], skin, c);
                        }
                        continue;
                    }
                    let allowed = r.attributes.global.iter().any(|g| g == n)
                        || r.attributes.prefixes.iter().any(|p| n.starts_with(p.as_str()))
                        || r.attributes.per_tag.get(name).is_some_and(|l| l.iter().any(|x| x == n))
                        || (is_svg_tag(name) && r.attributes.svg.iter().any(|x| x == n));
                    if !allowed {
                        c.warn(file, pos, format!("attribute {n} on <{name}> isn't allowed and is removed"));
                    }
                    if n == "tabindex" && a.value.trim() != "0" && a.value.trim() != "-1" {
                        c.warn(file, pos, "tabindex may only be 0 or -1");
                    }
                    if n == "style" {
                        let lower = a.value.to_ascii_lowercase();
                        if lower.contains("url(") || lower.contains("expression") || lower.contains("@import") {
                            c.warn(file, pos, "style attributes may not contain url(), expression or @import; removed");
                        }
                    }
                }
                // Bindings
                let mut pushed = false;
                let was_loop = scope.in_loop;
                for a in attrs.iter().filter(|a| a.name.starts_with("data-kn-")) {
                    let n = a.name.as_str();
                    let rule = match r.bindings.get(n) {
                        Some(rule) => rule,
                        None => {
                            if let Some(new) = r.renamed.bindings.get(n) {
                                c.warn(file, pos, format!("{n} was renamed to {new}"));
                            } else {
                                c.warn(file, pos, format!("unknown binding {n}; ignored"));
                            }
                            continue;
                        }
                    };
                    let v = a.value.trim();
                    match rule.value.as_str() {
                        "none" => {}
                        "literal" => {
                            if v.is_empty() && n != "data-kn-case" {
                                c.warn(file, pos, format!("{n} needs a value"));
                            }
                        }
                        "enum" => {
                            if !rule.values.iter().any(|x| x == v) {
                                c.err(file, pos, format!("{n}=\"{v}\": expected one of {}", rule.values.join(", ")));
                            }
                        }
                        "duration" => {
                            if !is_duration(v) {
                                c.err(file, pos, format!("{n}=\"{v}\": expected a duration like 8s or 500ms"));
                            }
                        }
                        "images" => {
                            if v.is_empty() {
                                c.err(file, pos, format!("{n} needs at least one image kind"));
                            }
                            for k in v.split_whitespace() {
                                if !r.image_kinds.iter().any(|x| x == k) {
                                    c.err(
                                        file,
                                        pos,
                                        format!("{n}: unknown image kind {k:?} ({})", r.image_kinds.join(", ")),
                                    );
                                }
                            }
                        }
                        "partial" => {
                            if !partials.contains(v) {
                                c.warn(
                                    file,
                                    pos,
                                    format!("partial {v:?} not found (partials/{v}.html); nothing is rendered"),
                                );
                            } else if depth + 1 >= r.limits.partial_depth {
                                c.err(file, pos, "partials nest too deeply (a partial probably includes itself)");
                            } else if check_paths {
                                // Check the partial against this scope.
                                let pfile = format!("partials/{v}.html");
                                if let Some(text) = skin.texts.get(&pfile) {
                                    let mut sub = Collector { issues: Vec::new() };
                                    lint_template(
                                        &pfile,
                                        text,
                                        Some(scope.clone()),
                                        partials,
                                        skin,
                                        &mut sub,
                                        depth + 1,
                                    );
                                    for mut i in sub.issues {
                                        // Data errors in a partial show up as warnings at the use site's file too.
                                        i.severity = Severity::Warning;
                                        if !c.issues.contains(&i) {
                                            c.issues.push(i);
                                        }
                                    }
                                }
                            }
                        }
                        "path" | "list" | "condition" | "attrmap" => {}
                        _ => {}
                    }
                    if !check_paths {
                        // Partial checked on its own: only shape checks.
                        if n == "data-kn-each" {
                            // Unknown element type inside: accept anything.
                            if !pushed {
                                scope.stack.push("any".into());
                                scope.in_loop = true;
                                pushed = true;
                            }
                        }
                        continue;
                    }
                    match rule.value.as_str() {
                        "path" => {
                            if let Err(e) = scope.resolve(v) {
                                c.warn(file, pos, format!("{n}: {e}"));
                            }
                        }
                        "condition" => {
                            if let Err(e) = scope.resolve(condition_path(v)) {
                                c.warn(file, pos, format!("{n}: {e}"));
                            }
                        }
                        "list" => match scope.resolve(v) {
                            Ok(t) if t == "any" => {
                                if n == "data-kn-each" && !pushed {
                                    scope.stack.push("any".into());
                                    scope.in_loop = true;
                                    pushed = true;
                                }
                            }
                            Ok(t) if t.ends_with("[]") => {
                                if n == "data-kn-each" && !pushed {
                                    scope.stack.push(t.trim_end_matches("[]").to_string());
                                    scope.in_loop = true;
                                    pushed = true;
                                }
                            }
                            Ok(t) => c.err(file, pos, format!("{n}=\"{v}\" is a {t}, not a list")),
                            Err(e) => c.err(file, pos, format!("{n}: {e}")),
                        },
                        "attrmap" => {
                            for pair in v.split_whitespace() {
                                let Some((an, p)) = pair.split_once(':') else {
                                    c.warn(file, pos, format!("{n}: {pair:?} should be name:path"));
                                    continue;
                                };
                                let ok = matches!(an, "title" | "alt")
                                    || an.starts_with("aria-")
                                    || (an.starts_with("data-") && !an.starts_with("data-kn-"));
                                if !ok {
                                    c.warn(
                                        file,
                                        pos,
                                        format!("{n}: attribute {an:?} can't be set (title, alt, aria-*, data-*)"),
                                    );
                                }
                                if let Err(e) = scope.resolve(p) {
                                    c.warn(file, pos, format!("{n}: {e}"));
                                }
                            }
                        }
                        _ => {}
                    }
                    // Scope requirements for links and actions.
                    if n == "data-kn-link" || n == "data-kn-action" {
                        let needs: Option<(&[&str], &str)> = match v {
                            "item" | "play" | "resume" | "parent" | "series" | "restart" | "toggle-played" => {
                                Some((&["Card", "Item"], "an item (inside a list of cards or on the item page)"))
                            }
                            "library" => Some((&["LibraryLink", "Library"], "a library (inside app.libraries)")),
                            "season" => Some((&["SeasonTab"], "a season tab (inside page.seasons)")),
                            "next-season" | "prev-season" => Some((&["Item"], "the item page")),
                            _ => None,
                        };
                        if let Some((types, what)) = needs {
                            if !scope.has_type(types) {
                                c.warn(file, pos, format!("{n}=\"{v}\" needs {what}"));
                            }
                        }
                    }
                }
                // Element-specific checks
                if get("data-kn-img").is_some() && name != "img" {
                    c.warn(file, pos, "data-kn-img only works on <img>; use data-kn-bg for backgrounds");
                }
                if let Some(sz) = get("data-kn-size") {
                    if !sz.trim().parse::<u32>().is_ok_and(|w| r.image_widths.contains(&w)) {
                        c.warn(file, pos, format!("data-kn-size should be one of {:?}", r.image_widths));
                    }
                }
                if get("data-kn-select").is_some() && name != "select" {
                    c.warn(file, pos, "data-kn-select only works on <select>");
                }
                if get("data-kn-search").is_some() && name != "input" {
                    c.warn(file, pos, "data-kn-search only works on <input type=\"search\">");
                }
                if get("data-kn-slot") == Some("outlet") && !file.starts_with("shell") {
                    c.warn(file, pos, "data-kn-slot=\"outlet\" only works in shell.html");
                }
                if name == "img"
                    && get("alt").is_none()
                    && get("data-kn-img").is_none()
                    && get("data-kn-attr").is_none_or(|v| !v.contains("alt:"))
                {
                    c.warn(file, pos, "<img> without alt; add alt=\"\" for decoration");
                }
                if name == "button" {
                    let labelled = get("aria-label").is_some()
                        || get("data-kn-action").is_some()
                        || get("data-kn-scroll").is_some()
                        || get("title").is_some()
                        || get("data-kn-attr").is_some_and(|v| v.contains("aria-label:"));
                    if !labelled && !element_has_text(&tokens, tok) {
                        c.warn(file, pos, "button without text or aria-label");
                    }
                }
                let is_void = r.void_tags.contains(name) || *self_closing;
                if is_void {
                    if pushed {
                        scope.stack.pop();
                        scope.in_loop = was_loop;
                    }
                } else {
                    frames.push(Frame { tag: name.clone(), pushed, was_loop });
                }
            }
        }
    }
}

fn is_svg_tag(tag: &str) -> bool {
    matches!(
        tag,
        "svg"
            | "g"
            | "path"
            | "circle"
            | "rect"
            | "line"
            | "polyline"
            | "polygon"
            | "ellipse"
            | "defs"
            | "lineargradient"
            | "radialgradient"
            | "stop"
            | "clippath"
            | "mask"
            | "text"
            | "tspan"
            | "symbol"
            | "use"
            | "title"
            | "desc"
    )
}

fn element_has_text(tokens: &[Token], start: &Token) -> bool {
    let Some(idx) = tokens.iter().position(|t| std::ptr::eq(t, start)) else { return true };
    let Token::Start { name, .. } = start else { return true };
    let mut depth = 0;
    for t in &tokens[idx + 1..] {
        match t {
            Token::Text { text, .. } if !text.trim().is_empty() => return true,
            Token::Start { attrs, name: n, .. } => {
                if attrs.iter().any(|a| a.name == "data-kn-text" || a.name == "aria-label") {
                    return true;
                }
                if n == name {
                    depth += 1;
                }
            }
            Token::End { name: n } if n == name => {
                if depth == 0 {
                    return false;
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    false
}

fn is_duration(v: &str) -> bool {
    let num = v.strip_suffix("ms").or_else(|| v.strip_suffix('s'));
    num.is_some_and(|n| !n.is_empty() && n.parse::<f64>().is_ok_and(|x| x > 0.0))
}

fn is_data_image(v: &str) -> bool {
    let l = v.trim().to_ascii_lowercase();
    ["png", "jpeg", "jpg", "gif", "webp", "svg+xml", "avif"]
        .iter()
        .any(|t| l.starts_with(&format!("data:image/{t};")) || l.starts_with(&format!("data:image/{t},")))
}

fn check_asset_ref(file: &str, pos: (u32, u32), name: &str, skin: &SkinSource, c: &mut Collector) {
    if !valid_asset_name(name) || asset_type(name).is_none() {
        c.warn(file, pos, format!("asset:{name} isn't a valid asset name"));
    } else if !skin.binaries.contains_key(&format!("assets/{name}"))
        && !skin.texts.contains_key(&format!("assets/{name}"))
    {
        c.warn(file, pos, format!("asset:{name} not found in assets/"));
    }
}

// ---------------------------------------------------------------------------
// CSS lint
// ---------------------------------------------------------------------------

fn lint_css(file: &str, css: &str, skin: &SkinSource, c: &mut Collector) {
    let lower = css.to_ascii_lowercase();
    let at = |needle: &str| lower.match_indices(needle).map(|(i, _)| line_col(css, i)).collect::<Vec<_>>();
    for pos in at("@import") {
        c.warn(file, pos, "@import is removed; list extra files in skin.json \"styles\" instead");
    }
    for pos in at("expression(").into_iter().chain(at("-moz-binding")).chain(at("behavior:")) {
        c.warn(file, pos, "legacy script hooks are removed");
    }
    for pos in at("</") {
        c.warn(file, pos, "markup isn't allowed in CSS; removed");
    }
    // url(...) references
    for (i, _) in lower.match_indices("url(") {
        let pos = line_col(css, i);
        let rest = &css[i + 4..];
        let end = rest.find(')').unwrap_or(rest.len());
        let inner = rest[..end].trim().trim_matches(['"', '\'']).trim();
        if let Some(name) = inner.strip_prefix("asset:") {
            check_asset_ref(file, pos, name, skin, c);
        } else if !is_data_image(inner) {
            c.warn(file, pos, format!("url({inner}) is removed: only url(asset:<file>) and data:image URLs work"));
        }
    }
    // Focus rings: warn when an outline is removed in a focus rule without a replacement.
    let mut search = 0;
    while let Some(p) = lower[search..].find(":focus") {
        let start = search + p;
        let Some(open) = lower[start..].find('{').map(|o| start + o) else { break };
        let close = lower[open..].find('}').map(|o| open + o).unwrap_or(lower.len());
        let body = &lower[open + 1..close];
        let removes = body.contains("outline: none")
            || body.contains("outline:none")
            || body.contains("outline: 0")
            || body.contains("outline:0");
        let replaces = body.contains("box-shadow")
            || body.contains("border")
            || body.contains("background")
            || body.contains("outline-color")
            || body.contains("transform")
            || body.contains("scale");
        // The selector of this rule; a rule for something inside the focused
        // element (`.card:focus-visible .art { outline: … }`) counts as a replacement.
        let sel_start = lower[..open].rfind(['}', ';']).map(|i| i + 1).unwrap_or(0);
        let selector = lower[sel_start..open].trim().to_string();
        let replaced_inside = selector.split(',').all(|sel| {
            let sel = sel.trim();
            let needle = format!("{sel} ");
            lower.match_indices(&needle).any(|(i, _)| {
                let after = lower[i + needle.len()..].trim_start();
                if after.starts_with('{') || after.starts_with(',') {
                    return false;
                }
                let body_start = lower[i..].find('{').map(|o| i + o + 1).unwrap_or(lower.len());
                let body_end = lower[body_start..].find('}').map(|o| body_start + o).unwrap_or(lower.len());
                let b = &lower[body_start..body_end];
                b.contains("outline") || b.contains("box-shadow") || b.contains("border")
            })
        });
        if removes && !replaces && !replaced_inside {
            c.warn(
                file,
                line_col(css, start),
                "focus outline removed without a replacement; remote and keyboard users won't see focus",
            );
        }
        search = close.max(start + 6);
    }
    let opens = css.matches('{').count();
    let closes = css.matches('}').count();
    if opens != closes {
        c.warn(file, (0, 0), format!("unbalanced braces ({opens} '{{' vs {closes} '}}')"));
    }
}

// ---------------------------------------------------------------------------
// Editor support: VS Code custom HTML data and a JSON Schema for skin.json
// ---------------------------------------------------------------------------

/// VS Code `html.customData` file describing every `data-kn-*` attribute.
pub fn html_custom_data() -> String {
    let r = rules();
    let attrs: Vec<Value> = r
        .bindings
        .iter()
        .map(|(name, b)| {
            let mut v = serde_json::json!({ "name": name, "description": b.help });
            if !b.values.is_empty() {
                v["values"] = Value::Array(b.values.iter().map(|x| serde_json::json!({ "name": x })).collect());
            }
            if b.value == "images" {
                v["values"] = Value::Array(r.image_kinds.iter().map(|x| serde_json::json!({ "name": x })).collect());
            }
            v
        })
        .collect();
    serde_json::to_string_pretty(&serde_json::json!({ "version": 1.1, "globalAttributes": attrs })).unwrap_or_default()
}

/// JSON Schema for `skin.json`.
pub fn manifest_schema() -> String {
    let r = rules();
    let schema = serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "Kinora app skin manifest",
        "type": "object",
        "required": ["id", "name", "skinApi"],
        "properties": {
            "id": { "type": "string", "pattern": "^[a-z0-9][a-z0-9-]{0,63}$", "description": "Must match the folder name." },
            "name": { "type": "string", "maxLength": 80 },
            "version": { "type": "string", "description": "Your skin's version, e.g. 1.2.0." },
            "author": { "type": "string" },
            "description": { "type": "string" },
            "homepage": { "type": "string", "format": "uri" },
            "skinApi": { "type": "integer", "enum": r.supported, "description": "The binding contract version this skin targets." },
            "styles": { "type": "array", "items": { "type": "string", "pattern": "\\.css$" }, "maxItems": 8 },
            "devices": { "type": "array", "items": { "enum": r.devices } },
            "player": { "type": "string", "description": "\"./player\" (bundled) or a built-in style: minimal, cinematic, floating, pro." },
            "theme": { "type": "string", "description": "Suggested theme id; may be bundled in themes/." },
            "scheme": { "enum": ["dark", "light"] },
            "options": {
                "type": "object",
                "additionalProperties": {
                    "type": "object",
                    "required": ["type", "label"],
                    "properties": {
                        "type": { "enum": ["bool", "enum", "number", "color"] },
                        "label": { "type": "string" },
                        "default": {},
                        "values": { "type": "array", "items": { "type": "string" } },
                        "min": { "type": "number" },
                        "max": { "type": "number" }
                    }
                }
            }
        }
    });
    serde_json::to_string_pretty(&schema).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(files: &[(&str, &str)]) -> SkinSource {
        SkinSource {
            folder: "test".into(),
            texts: files.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
            binaries: BTreeMap::new(),
        }
    }

    const MANIFEST: &str = r#"{ "id": "test", "name": "Test", "version": "1.0.0", "skinApi": 1 }"#;

    fn messages(r: &SkinReport) -> Vec<String> {
        r.issues.iter().map(|i| i.to_string()).collect()
    }

    #[test]
    fn rules_parse() {
        let r = rules();
        assert_eq!(r.skin_api, 1);
        assert!(r.types.contains_key("Card"));
        assert!(type_fields("Item").unwrap().contains_key("title"), "Item extends Card");
    }

    #[test]
    fn minimal_skin_is_valid() {
        let r = check_skin(&source(&[
            ("skin.json", MANIFEST),
            ("shell.html", r#"<header><a data-kn-link="home">Home</a></header><main data-kn-slot="outlet"></main>"#),
            (
                "pages/home.html",
                r#"<section data-kn-each="rows"><h2 data-kn-text="title"></h2><a data-kn-each="items" data-kn-link="item" data-kn-use="card"></a></section>"#,
            ),
            ("partials/card.html", r#"<img data-kn-img="poster"><span data-kn-text="title"></span>"#),
            ("skin.css", "header { display: flex } .card:focus-visible { outline: 2px solid var(--kn-ring) }"),
        ]));
        assert!(r.valid, "{:?}", messages(&r));
        assert_eq!(r.errors(), 0, "{:?}", messages(&r));
        assert_eq!(r.warnings(), 0, "{:?}", messages(&r));
        assert_eq!(r.templates, vec!["pages/home", "partials/card", "shell"]);
    }

    #[test]
    fn manifest_errors_invalidate() {
        let r = check_skin(&source(&[("skin.json", r#"{ "id": "other", "name": "X", "skinApi": 1 }"#)]));
        assert!(!r.valid);
        assert!(messages(&r).iter().any(|m| m.contains("must match the folder")));
        let r = check_skin(&source(&[("skin.json", r#"{ "id": "test", "name": "X", "skinApi": 99 }"#)]));
        assert!(!r.valid);
        assert!(messages(&r).iter().any(|m| m.contains("Update Kinora")));
        let r = check_skin(&source(&[("skin.json", "{ nope")]));
        assert!(!r.valid);
    }

    #[test]
    fn shell_needs_one_outlet() {
        let r = check_skin(&source(&[("skin.json", MANIFEST), ("shell.html", "<div></div>")]));
        assert!(r.valid, "a broken shell doesn't break the skin");
        assert_eq!(r.disabled, vec!["shell"]);
    }

    #[test]
    fn data_paths_are_checked_against_the_page() {
        let r = check_skin(&source(&[
            ("skin.json", MANIFEST),
            (
                "pages/home.html",
                r#"<p data-kn-text="nope"></p><div data-kn-each="featured"><span data-kn-text="overview"></span></div>"#,
            ),
            ("pages/library.html", r#"<div data-kn-each="count"></div>"#),
        ]));
        let m = messages(&r);
        assert!(m.iter().any(|x| x.contains("pages/home.html:1:1") && x.contains("unknown field \"nope\"")), "{m:?}");
        assert!(!m.iter().any(|x| x.contains("overview")), "{m:?}");
        assert!(m.iter().any(|x| x.contains("is a number, not a list")), "{m:?}");
        assert_eq!(r.disabled, vec!["pages/library"]);
        assert!(r.templates.contains(&"pages/home".to_string()));
    }

    #[test]
    fn scripts_links_and_handlers_are_reported() {
        let r = check_skin(&source(&[
            ("skin.json", MANIFEST),
            ("pages/home.html", "<script>alert(1)</script>\n<a href=\"https://evil\" onclick=\"x()\">x</a><img src=\"https://x/y.png\" alt=\"\">"),
        ]));
        let m = messages(&r).join("\n");
        assert!(m.contains("<script> isn't allowed"), "{m}");
        assert!(m.contains("pages/home.html:2:1"), "{m}");
        assert!(m.contains("event attribute onclick"), "{m}");
        assert!(m.contains("use data-kn-link"), "{m}");
        assert!(m.contains("use data-kn-img"), "{m}");
    }

    #[test]
    fn partials_are_checked_where_used_and_links_need_items() {
        let r = check_skin(&source(&[
            ("skin.json", MANIFEST),
            (
                "pages/home.html",
                r#"<div data-kn-each="rows"><div data-kn-use="card"></div></div><a data-kn-link="item">x</a><div data-kn-use="missing"></div>"#,
            ),
            ("partials/card.html", r#"<span data-kn-text="title"></span>"#),
        ]));
        let m = messages(&r).join("\n");
        // Row has a title, so the partial is fine there.
        assert!(!m.contains("unknown field \"title\""), "{m}");
        assert!(m.contains("data-kn-link=\"item\" needs an item"), "{m}");
        assert!(m.contains("partial \"missing\" not found"), "{m}");
    }

    #[test]
    fn loop_helpers_only_in_loops() {
        let r = check_skin(&source(&[
            ("skin.json", MANIFEST),
            (
                "pages/home.html",
                r#"<b data-kn-text="@number"></b><div data-kn-each="rows"><b data-kn-text="@number"></b><i data-kn-each="items"><span data-kn-text="genresLabel"></span><s data-kn-each="genres" data-kn-text="."></s></i></div>"#,
            ),
        ]));
        let m = messages(&r);
        assert_eq!(m.iter().filter(|x| x.contains("@number only works inside")).count(), 1, "{m:?}");
        assert_eq!(r.errors(), 0, "{m:?}");
    }

    #[test]
    fn css_checks() {
        let mut s = source(&[
            ("skin.json", MANIFEST),
            ("pages/home.html", "<p></p>"),
            ("skin.css", "@import 'x.css';\n.a { background: url(https://x/y.png) }\n.b { background: url(asset:noise.webp) }\n.c { font: url(asset:nope.woff2) }\n.d:focus { outline: none }"),
        ]);
        s.binaries.insert("assets/noise.webp".into(), 10);
        let m = messages(&check_skin(&s)).join("\n");
        assert!(m.contains("@import is removed"), "{m}");
        assert!(m.contains("url(https://x/y.png) is removed"), "{m}");
        assert!(!m.contains("noise.webp not found"), "{m}");
        assert!(m.contains("asset:nope.woff2 not found"), "{m}");
        assert!(m.contains("focus outline removed"), "{m}");
    }

    #[test]
    fn page_keys() {
        assert_eq!(
            parse_page_key("pages/item.series.tv"),
            Some(("item".into(), Some("series".into()), Some("tv".into())))
        );
        assert_eq!(parse_page_key("pages/home.tv"), Some(("home".into(), None, Some("tv".into()))));
        assert_eq!(parse_page_key("shell"), Some(("shell".into(), None, None)));
        assert_eq!(parse_page_key("pages/home.series"), None);
        assert_eq!(parse_page_key("pages/nope"), None);
    }

    #[test]
    fn options() {
        let opt = SkinOption {
            kind: "enum".into(),
            label: "Size".into(),
            default: Some(Value::String("m".into())),
            values: vec!["s".into(), "m".into()],
            min: None,
            max: None,
        };
        assert_eq!(opt.check_value("s").unwrap(), "s");
        assert!(opt.check_value("xl").is_err());
        let color =
            SkinOption { kind: "color".into(), label: "C".into(), default: None, values: vec![], min: None, max: None };
        assert!(color.check_value("#ff0000").is_ok());
        assert!(color.check_value("oklch(0.7 0.1 200)").is_ok());
        assert!(color.check_value("red; } body { x").is_err());
        assert!(color.check_value("url(http://x)").is_err());
        assert!(color.check_value("not a colour").is_err());
        assert!(color.check_value("").is_ok());
        assert!(color.check_value("rebeccapurple").is_ok());
    }

    #[test]
    fn editor_data_is_json() {
        let d: Value = serde_json::from_str(&html_custom_data()).unwrap();
        assert!(d["globalAttributes"].as_array().unwrap().iter().any(|a| a["name"] == "data-kn-each"));
        let s: Value = serde_json::from_str(&manifest_schema()).unwrap();
        assert_eq!(s["required"][0], "id");
    }
}
