//! The plugin API.
//!
//! Every optional feature (TMDB metadata, themes, notifications, ...) is a
//! *module* that implements one or more of the traits below. Built-in modules
//! are compiled in today; community plugins will implement the same
//! interfaces as WebAssembly components later (see docs/plan.md §4), so this
//! crate is the contract both sides share.
//!
//! Versioning: bump [`API_VERSION`] (semver) on any change. Breaking changes
//! bump the major version once 1.0 is declared.

use std::collections::BTreeMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub const API_VERSION: &str = "0.1.0";

/// Describes a module to the registry and the admin UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleInfo {
    /// Stable id, e.g. "tmdb". Lowercase, dashes allowed.
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    /// Extension points this module provides.
    pub provides: Vec<ExtensionPoint>,
    pub enabled_by_default: bool,
    /// Settings shown in the admin UI; the form is generated from this.
    pub settings: Vec<SettingField>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionPoint {
    MetadataProvider,
    Theme,
    PlayerSkin,
    EventSubscriber,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingField {
    pub key: String,
    pub label: String,
    pub kind: SettingKind,
    pub help: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingKind {
    Text,
    /// Never sent back to the browser after saving.
    Secret,
    Bool,
}

/// Module settings as saved by the admin.
pub type Settings = BTreeMap<String, String>;

// ---------------------------------------------------------------------------
// Metadata providers
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Metadata {
    /// The provider's own id for this item (e.g. a TMDB id).
    pub provider_id: String,
    pub title: Option<String>,
    pub original_title: Option<String>,
    pub year: Option<u16>,
    pub overview: Option<String>,
    pub tagline: Option<String>,
    /// 0-10.
    pub rating: Option<f32>,
    pub official_rating: Option<String>,
    pub genres: Vec<String>,
    pub runtime_minutes: Option<u32>,
    /// Absolute image URLs; the server downloads and caches them.
    pub poster_url: Option<String>,
    pub backdrop_url: Option<String>,
    pub thumb_url: Option<String>,
    pub release_date: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    Movie,
    Series,
}

#[async_trait]
pub trait MetadataProvider: Send + Sync {
    fn id(&self) -> &str;

    /// Best match for a movie or series name.
    async fn search(&self, kind: MediaKind, title: &str, year: Option<u16>) -> anyhow::Result<Option<Metadata>>;

    /// Full details for a provider id found by `search`.
    async fn details(&self, kind: MediaKind, provider_id: &str) -> anyhow::Result<Option<Metadata>>;

    /// Details for one episode of a series.
    async fn episode(&self, series_provider_id: &str, season: u32, episode: u32) -> anyhow::Result<Option<Metadata>>;
}

// ---------------------------------------------------------------------------
// Themes
// ---------------------------------------------------------------------------

/// A theme is just data: design tokens (CSS custom properties) the web app
/// applies at runtime. Built-in themes, drop-in `*.json` files in the config
/// `themes/` folder, and future marketplace themes all use this format.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Theme {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub author: Option<String>,
    /// "dark" or "light": hints the browser's built-in controls.
    #[serde(default = "default_scheme")]
    pub scheme: String,
    /// Token name (without the leading `--`) to CSS value, e.g.
    /// `"primary": "oklch(0.7 0.15 250)"`, `"radius": "0.75rem"`.
    pub tokens: BTreeMap<String, String>,
    /// Optional video player preferences: which layout to use and a few
    /// look-and-feel knobs. Data only, like the tokens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub player: Option<PlayerTheme>,
}

/// The `player` block of a theme.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct PlayerTheme {
    /// A built-in layout id ("minimal", "cinematic", "floating", "pro") or the
    /// id of an installed player skin. Viewers can still pick their own.
    #[serde(default)]
    pub layout: Option<String>,
    /// Keys from [`PLAYER_OPTIONS`] to CSS values, e.g. `"seek-height": "8px"`.
    #[serde(default)]
    pub options: BTreeMap<String, String>,
}

/// Player look-and-feel options a theme may set (applied as `--kn-player-*`).
pub const PLAYER_OPTIONS: &[&str] = &["bar-blur", "seek-height", "button-radius", "controls-opacity"];

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn valid_css_value(v: &str) -> bool {
    v.len() <= 200 && !v.contains([';', '{', '}', '<', '>']) && !v.to_lowercase().contains("url(")
}

fn default_scheme() -> String {
    "dark".into()
}

/// Tokens every theme should define. Missing ones fall back to the default theme.
pub const THEME_TOKENS: &[&str] = &[
    "background",
    "foreground",
    "card",
    "card-foreground",
    "muted",
    "muted-foreground",
    "primary",
    "primary-foreground",
    "accent",
    "accent-foreground",
    "border",
    "ring",
    "destructive",
    "radius",
    "font-sans",
];

impl Theme {
    /// Validate a theme from an untrusted file: sane id, known tokens only,
    /// and values that can't break out of a CSS declaration.
    pub fn validate(&self) -> Result<(), String> {
        if !valid_id(&self.id) {
            return Err(format!("invalid theme id {:?}", self.id));
        }
        if self.scheme != "dark" && self.scheme != "light" {
            return Err("scheme must be \"dark\" or \"light\"".into());
        }
        for (k, v) in &self.tokens {
            if !THEME_TOKENS.contains(&k.as_str()) {
                return Err(format!("unknown token {k:?}"));
            }
            if v.len() > 200 || v.contains([';', '{', '}', '<', '>']) {
                return Err(format!("invalid value for {k:?}"));
            }
        }
        if let Some(player) = &self.player {
            if let Some(layout) = &player.layout {
                if !valid_id(layout) {
                    return Err(format!("invalid player layout {layout:?}"));
                }
            }
            for (k, v) in &player.options {
                if !PLAYER_OPTIONS.contains(&k.as_str()) {
                    return Err(format!("unknown player option {k:?}"));
                }
                if !valid_css_value(v) {
                    return Err(format!("invalid value for player option {k:?}"));
                }
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Player skins
// ---------------------------------------------------------------------------

/// A custom video player skin: your own markup and CSS. The web app wires
/// the markup to the player through `data-kn-*` attributes (docs/player-skins.md).
/// Skins are installed by the server admin as folders in `player-skins/`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlayerSkin {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub author: Option<String>,
    /// Markup (from `skin.html`). Filled in by the loader, not by `skin.json`.
    #[serde(default)]
    pub html: String,
    /// Styles (from `skin.css`), scoped to this skin by the web app.
    #[serde(default)]
    pub css: String,
}

const MAX_SKIN_BYTES: usize = 200_000;

impl PlayerSkin {
    /// First line of defence for files from disk. The browser sanitises the
    /// markup and CSS again before using them, so this is not the only check.
    pub fn validate(&self) -> Result<(), String> {
        if !valid_id(&self.id) {
            return Err(format!("invalid skin id {:?}", self.id));
        }
        if self.name.trim().is_empty() || self.name.len() > 80 {
            return Err("skin name must be 1-80 characters".into());
        }
        if self.html.trim().is_empty() {
            return Err("skin.html is empty".into());
        }
        if self.html.len() > MAX_SKIN_BYTES || self.css.len() > MAX_SKIN_BYTES {
            return Err("skin files are too large".into());
        }
        let html = self.html.to_lowercase();
        for bad in ["<script", "<iframe", "<object", "<embed", "<link", "<style", "<base", "<meta", "javascript:"] {
            if html.contains(bad) {
                return Err(format!("skin.html may not contain {bad:?}"));
            }
        }
        let css = self.css.to_lowercase();
        for bad in ["@import", "expression(", "-moz-binding", "behavior:", "</"] {
            if css.contains(bad) {
                return Err(format!("skin.css may not contain {bad:?}"));
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

/// Things that happen on the server that modules may react to.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerEvent {
    LibraryScanStarted { library_id: String },
    LibraryScanFinished { library_id: String, added: u32, removed: u32 },
    ItemAdded { item_id: String, title: String },
    PlaybackStarted { user_id: String, item_id: String, method: String },
    PlaybackStopped { user_id: String, item_id: String, position_ms: u64 },
    UserCreated { user_id: String, username: String },
}

#[async_trait]
pub trait EventSubscriber: Send + Sync {
    async fn on_event(&self, event: &ServerEvent);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theme(id: &str, tokens: &[(&str, &str)]) -> Theme {
        Theme {
            id: id.into(),
            name: "T".into(),
            author: None,
            scheme: "dark".into(),
            tokens: tokens.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
            player: None,
        }
    }

    #[test]
    fn valid_theme() {
        assert!(theme("ocean-blue", &[("primary", "oklch(0.7 0.15 250)")]).validate().is_ok());
    }

    #[test]
    fn rejects_css_injection_and_bad_ids() {
        assert!(theme("x", &[("primary", "red; } body { display:none")]).validate().is_err());
        assert!(theme("x", &[("not-a-token", "red")]).validate().is_err());
        assert!(theme("Bad Id", &[]).validate().is_err());
        assert!(theme("../etc", &[]).validate().is_err());
    }

    #[test]
    fn parses_theme_json() {
        let t: Theme =
            serde_json::from_str(r##"{"id":"sunset","name":"Sunset","tokens":{"primary":"#ff7a59","radius":"1rem"}}"##)
                .unwrap();
        assert_eq!(t.scheme, "dark");
        assert!(t.validate().is_ok());
    }

    #[test]
    fn player_block_is_validated() {
        let ok =
            r##"{"id":"p","name":"P","tokens":{},"player":{"layout":"cinematic","options":{"seek-height":"8px"}}}"##;
        assert!(serde_json::from_str::<Theme>(ok).unwrap().validate().is_ok());
        let bad_opt = r##"{"id":"p","name":"P","tokens":{},"player":{"options":{"nope":"1"}}}"##;
        assert!(serde_json::from_str::<Theme>(bad_opt).unwrap().validate().is_err());
        let bad_val = r##"{"id":"p","name":"P","tokens":{},"player":{"options":{"bar-blur":"url(http://x)"}}}"##;
        assert!(serde_json::from_str::<Theme>(bad_val).unwrap().validate().is_err());
        let bad_layout = r##"{"id":"p","name":"P","tokens":{},"player":{"layout":"../x"}}"##;
        assert!(serde_json::from_str::<Theme>(bad_layout).unwrap().validate().is_err());
    }

    fn skin(html: &str, css: &str) -> PlayerSkin {
        PlayerSkin { id: "neon".into(), name: "Neon".into(), author: None, html: html.into(), css: css.into() }
    }

    #[test]
    fn skin_validation() {
        assert!(skin("<div data-kn-action=\"play\"></div>", ".a { color: red }").validate().is_ok());
        assert!(skin("<script>alert(1)</script>", "").validate().is_err());
        assert!(skin("<a href=\"javascript:x\">x</a>", "").validate().is_err());
        assert!(skin("<div></div>", "@import 'http://x';").validate().is_err());
        assert!(skin("", "").validate().is_err());
    }
}
