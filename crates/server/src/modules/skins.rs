//! Player skins: custom video player markup and CSS, installed as folders in
//! `<config>/player-skins/<name>/` (module "player-skins"). See docs/player-skins.md.
//!
//! A skin folder holds `skin.json` (id, name, author), `skin.html` and
//! optionally `skin.css`. The web app sanitises and scopes the files again
//! before using them, so a skin can restyle the player but not run code.

use std::path::Path;

use axum::extract::State;
use axum::Json;
use kinora_plugin_api::{ExtensionPoint, ModuleInfo, PlayerSkin};
use serde::{Deserialize, Serialize};

use crate::auth::{AdminUser, AuthUser};
use crate::db::{get_setting, set_setting};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// Settings key for the server-wide player style.
const STYLE_KEY: &str = "player_style";
/// "Follow the theme's `player.layout`", the default.
pub const THEME_STYLE: &str = "theme";

pub const EXAMPLE_JSON: &str = include_str!("skin_example/skin.json");
pub const EXAMPLE_HTML: &str = include_str!("skin_example/skin.html");
pub const EXAMPLE_CSS: &str = include_str!("skin_example/skin.css");

pub fn info() -> ModuleInfo {
    ModuleInfo {
        id: "player-skins".into(),
        name: "Custom player skins".into(),
        description: "Shows the video player skins you install from the registry, and your own (markup and CSS) \
                      from the player-skins folder in your config directory."
            .into(),
        version: env!("CARGO_PKG_VERSION").into(),
        provides: vec![ExtensionPoint::PlayerSkin],
        enabled_by_default: true,
        settings: vec![],
    }
}

/// Write the example skin so people have something to copy.
pub fn write_example(dir: &Path) {
    let example = dir.join("example-neon.sample");
    if example.exists() {
        return;
    }
    if std::fs::create_dir_all(&example).is_ok() {
        let _ = std::fs::write(example.join("skin.json"), EXAMPLE_JSON);
        let _ = std::fs::write(example.join("skin.html"), EXAMPLE_HTML);
        let _ = std::fs::write(example.join("skin.css"), EXAMPLE_CSS);
    }
}

pub fn load_one(dir: &Path) -> Result<PlayerSkin, String> {
    let json = std::fs::read_to_string(dir.join("skin.json")).map_err(|e| format!("skin.json: {e}"))?;
    let mut skin: PlayerSkin = serde_json::from_str(&json).map_err(|e| format!("skin.json: {e}"))?;
    skin.html = std::fs::read_to_string(dir.join("skin.html")).map_err(|e| format!("skin.html: {e}"))?;
    skin.css = std::fs::read_to_string(dir.join("skin.css")).unwrap_or_default();
    skin.validate()?;
    Ok(skin)
}

/// Every valid skin folder. Folders ending in `.sample` are ignored; bad
/// skins are skipped with a warning instead of breaking the player.
pub fn load_custom(dir: &Path) -> Vec<PlayerSkin> {
    let Ok(entries) = std::fs::read_dir(dir) else { return vec![] };
    let mut out: Vec<PlayerSkin> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if !path.is_dir() || name.ends_with(".sample") || name.starts_with('.') {
            continue;
        }
        match load_one(&path) {
            Ok(s) if out.iter().any(|o| o.id == s.id) => {
                tracing::warn!(folder = %path.display(), id = %s.id, "skipping skin with a duplicate id")
            }
            Ok(s) => out.push(s),
            Err(e) => tracing::warn!(folder = %path.display(), error = %e, "skipping invalid player skin"),
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Skins the viewer can pick from. Empty when the module is turned off.
pub async fn list(State(state): State<AppState>, _user: AuthUser) -> Json<Vec<PlayerSkin>> {
    if !super::is_enabled(&state.db, "player-skins").await {
        return Json(vec![]);
    }
    let mut skins = load_custom(&state.config.skins_dir());
    // Players that come with installed app skins.
    for p in super::app_skins::bundled_players(&state).await {
        if !skins.iter().any(|s| s.id == p.id) {
            skins.push(p);
        }
    }
    Json(skins)
}

/// The player every client uses: "theme", a built-in layout id or a skin id.
#[derive(Serialize, Deserialize)]
pub struct PlayerStyle {
    pub style: String,
}

/// Ids are checked for shape only. A skin that is missing (removed, or the
/// module turned off) makes clients fall back to the default layout.
fn valid_style(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

pub async fn get_style(State(state): State<AppState>, _user: AuthUser) -> AppResult<Json<PlayerStyle>> {
    let style = get_setting(&state.db, STYLE_KEY).await?.unwrap_or_else(|| THEME_STYLE.into());
    Ok(Json(PlayerStyle { style }))
}

pub async fn put_style(
    State(state): State<AppState>,
    _admin: AdminUser,
    Json(body): Json<PlayerStyle>,
) -> AppResult<Json<PlayerStyle>> {
    let style = body.style.trim().to_string();
    if !valid_style(&style) {
        return Err(AppError::BadRequest("player style must be a lowercase id (letters, digits, -)".into()));
    }
    set_setting(&state.db, STYLE_KEY, &style).await?;
    Ok(Json(PlayerStyle { style }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn example_skin_is_valid() {
        let dir = tempfile_dir("kinora-skin-test");
        write_example(&dir);
        let skin = load_one(&dir.join("example-neon.sample")).unwrap();
        assert_eq!(skin.id, "example-neon");
        // .sample folders are not loaded until renamed
        assert!(load_custom(&dir).is_empty());
        std::fs::rename(dir.join("example-neon.sample"), dir.join("example-neon")).unwrap();
        assert_eq!(load_custom(&dir).len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn invalid_skins_are_skipped() {
        let dir = tempfile_dir("kinora-skin-bad");
        let bad = dir.join("bad");
        std::fs::create_dir_all(&bad).unwrap();
        std::fs::write(bad.join("skin.json"), r#"{"id":"bad","name":"Bad"}"#).unwrap();
        std::fs::write(bad.join("skin.html"), "<script>alert(1)</script>").unwrap();
        assert!(load_custom(&dir).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn tempfile_dir(prefix: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("{prefix}-{}", crate::db::new_id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
