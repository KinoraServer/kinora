//! Themes: Kinora's own two (dark and light) plus `*.json` files in
//! `<config>/themes/` (module "custom-themes"): your own, and the ones
//! installed from the registry. A theme is only design tokens, so making one
//! is copying a file and changing colours. See docs/themes.md.

use std::collections::BTreeMap;

use axum::extract::State;
use axum::Json;
use kinora_plugin_api::{ExtensionPoint, ModuleInfo, Theme};
use serde::Serialize;

use crate::state::AppState;

pub fn info() -> ModuleInfo {
    ModuleInfo {
        id: "custom-themes".into(),
        name: "Custom themes".into(),
        description:
            "Shows the themes you install from the registry, and your own theme files (*.json) from the themes \
                      folder in your config directory."
                .into(),
        version: env!("CARGO_PKG_VERSION").into(),
        provides: vec![ExtensionPoint::Theme],
        enabled_by_default: true,
        settings: vec![],
    }
}

fn theme(id: &str, name: &str, scheme: &str, tokens: &[(&str, &str)]) -> Theme {
    Theme {
        id: id.into(),
        name: name.into(),
        author: Some("Built-in".into()),
        scheme: scheme.into(),
        pair: None,
        tokens: tokens.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect::<BTreeMap<_, _>>(),
        player: None,
    }
}

const FONT: &str = "ui-sans-serif, system-ui, -apple-system, Segoe UI, Roboto, sans-serif";

pub fn builtin_themes() -> Vec<Theme> {
    let mut themes = vec![
        theme(
            "kinora",
            "Kinora",
            "dark",
            &[
                ("background", "#0B1612"),
                ("foreground", "#ECF7F1"),
                ("card", "#13221C"),
                ("card-foreground", "#ECF7F1"),
                ("muted", "#1C2E26"),
                ("muted-foreground", "#8FA79B"),
                ("primary", "#2BD48F"),
                ("primary-foreground", "#04140D"),
                ("accent", "#1A3328"),
                ("accent-foreground", "#ECF7F1"),
                ("border", "#22382E"),
                ("ring", "#2BD48F"),
                ("destructive", "#F2555A"),
                ("radius", "0.75rem"),
                ("font-sans", FONT),
            ],
        ),
        theme(
            "kinora-light",
            "Kinora Light",
            "light",
            &[
                ("background", "#F5FAF7"),
                ("foreground", "#0B1612"),
                ("card", "#FFFFFF"),
                ("card-foreground", "#0B1612"),
                ("muted", "#E6F0EA"),
                ("muted-foreground", "#4F685C"),
                ("primary", "#0A7F50"),
                ("primary-foreground", "#FFFFFF"),
                ("accent", "#DDEFE5"),
                ("accent-foreground", "#0B1612"),
                ("border", "#D3E3DA"),
                ("ring", "#0A7F50"),
                ("destructive", "#C9353A"),
                ("radius", "0.75rem"),
                ("font-sans", FONT),
            ],
        ),
    ];
    // Paired themes show as one choice that follows light/dark mode.
    if let Some(t) = themes.iter_mut().find(|t| t.id == "kinora") {
        t.pair = Some("kinora-light".into());
    }
    themes
}

/// Read and validate every `*.json` in the themes folder. Bad files are
/// skipped with a warning instead of breaking the app.
pub fn load_custom(dir: &std::path::Path) -> Vec<Theme> {
    let Ok(entries) = std::fs::read_dir(dir) else { return vec![] };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let parsed = std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|s| serde_json::from_str::<Theme>(&s).map_err(|e| e.to_string()))
            .and_then(|t| t.validate().map(|_| t));
        match parsed {
            Ok(t) => out.push(t),
            Err(e) => tracing::warn!(file = %path.display(), error = %e, "skipping invalid theme"),
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

#[derive(Serialize)]
pub struct ThemeList {
    pub default_theme: String,
    pub themes: Vec<Theme>,
}

/// Public (the login page is themed too).
pub async fn list(State(state): State<AppState>) -> Json<ThemeList> {
    let mut themes = builtin_themes();
    if super::is_enabled(&state.db, "custom-themes").await {
        let mut extra = load_custom(&state.config.themes_dir());
        // Themes that come with installed app skins.
        extra.extend(super::app_skins::bundled_themes(&state).await);
        for t in extra {
            // A custom theme can't replace a built-in one.
            if !themes.iter().any(|b| b.id == t.id) {
                themes.push(t);
            }
        }
    }
    let default_theme = crate::db::get_setting(&state.db, "default_theme")
        .await
        .ok()
        .flatten()
        .filter(|id| themes.iter().any(|t| &t.id == id))
        .unwrap_or_else(|| "kinora".into());
    Json(ThemeList { default_theme, themes })
}

/// Written to the themes folder on first start so people have something to copy.
pub const EXAMPLE_THEME: &str = r#"{
  "id": "example-ocean",
  "name": "Ocean (example)",
  "author": "You",
  "scheme": "dark",
  "tokens": {
    "background": "oklch(0.17 0.03 230)",
    "card": "oklch(0.22 0.035 230)",
    "muted": "oklch(0.28 0.03 230)",
    "border": "oklch(0.32 0.03 230)",
    "primary": "oklch(0.78 0.13 195)",
    "primary-foreground": "oklch(0.18 0.03 230)",
    "ring": "oklch(0.78 0.13 195)",
    "radius": "1.25rem"
  },
  "player": {
    "layout": "floating",
    "options": { "seek-height": "8px", "bar-blur": "20px" }
  }
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_are_valid_and_complete() {
        for t in builtin_themes() {
            t.validate().unwrap();
            for token in kinora_plugin_api::THEME_TOKENS {
                assert!(t.tokens.contains_key(*token), "{} is missing {token}", t.id);
            }
        }
    }

    #[test]
    fn example_theme_is_valid() {
        let t: Theme = serde_json::from_str(EXAMPLE_THEME).unwrap();
        t.validate().unwrap();
    }

    #[test]
    fn loads_good_and_skips_bad_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("good.json"), EXAMPLE_THEME).unwrap();
        std::fs::write(dir.path().join("bad.json"), r#"{"id":"x","name":"x","tokens":{"primary":"red;}"}}"#).unwrap();
        std::fs::write(dir.path().join("notes.txt"), "hi").unwrap();
        let themes = load_custom(dir.path());
        assert_eq!(themes.len(), 1);
        assert_eq!(themes[0].id, "example-ocean");
    }
}
