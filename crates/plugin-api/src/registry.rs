//! The package registry format (format 1): what a registry publishes and what
//! the server checks before it installs anything. Pure data and rules; the
//! downloading lives in the server.
//!
//! A registry is a folder of static files:
//!
//! ```text
//! index.json                               every package and its versions
//! index.json.sig                           Ed25519 signature of index.json (optional)
//! packages/<id>/<version>/package.json     the manifest: what it is, its files and their SHA-256
//! packages/<id>/<version>/files/...        the files
//! ```
//!
//! Trust runs downwards: the signature covers the index, the index holds the
//! hash of every manifest, and a manifest holds the hash of every file.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{ExtensionPoint, SettingField};

/// The registry layout this server understands.
pub const FORMAT: u32 = 1;
/// Module ABIs this server can run (see docs/modules.md).
pub const MODULE_ABIS: &[u32] = &[1];
/// A module's code inside its package.
pub const MODULE_ENTRY: &str = "module.wasm";

pub const MAX_INDEX_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_MANIFEST_BYTES: usize = 512 * 1024;
pub const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024;
pub const MAX_PACKAGE_BYTES: u64 = 20 * 1024 * 1024;
pub const MAX_FILES: usize = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PackageKind {
    /// Server logic: a sandboxed WebAssembly module.
    Module,
    /// One or more theme files (design tokens).
    Theme,
    /// A video player skin (markup and CSS).
    PlayerSkin,
    /// A whole-app skin (templates, CSS and assets).
    AppSkin,
}

impl PackageKind {
    pub fn as_str(self) -> &'static str {
        match self {
            PackageKind::Module => "module",
            PackageKind::Theme => "theme",
            PackageKind::PlayerSkin => "player-skin",
            PackageKind::AppSkin => "app-skin",
        }
    }
}

/// What a module may do outside its sandbox. Shown to the admin before installing.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Permissions {
    /// Hosts the module may send HTTPS requests to.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub http: Vec<String>,
    /// Keep its own data in the server's database.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub storage: bool,
    /// Write lines to Admin > Activity.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub activity: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Index {
    pub format: u32,
    #[serde(default)]
    pub packages: Vec<IndexPackage>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IndexPackage {
    pub id: String,
    pub kind: PackageKind,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub recommended: bool,
    #[serde(default)]
    pub provides: Vec<String>,
    #[serde(default)]
    pub permissions: Permissions,
    /// Newest first.
    #[serde(default)]
    pub versions: Vec<IndexVersion>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IndexVersion {
    pub version: String,
    /// The oldest Kinora this version works with.
    #[serde(default)]
    pub min_kinora: Option<String>,
    /// Modules only: the module ABI it was built for.
    #[serde(default)]
    pub abi: Option<u32>,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub published_at: i64,
    /// Path of the manifest, relative to the registry's base URL.
    pub manifest: String,
    /// SHA-256 of the manifest file (hex).
    pub sha256: String,
}

/// `packages/<id>/<version>/package.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageManifest {
    pub format: u32,
    pub id: String,
    pub kind: PackageKind,
    pub name: String,
    pub description: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_kinora: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module: Option<ModuleSpec>,
    pub files: Vec<PackageFile>,
}

/// The `module` block of a module's manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleSpec {
    pub abi: u32,
    pub entry: String,
    pub provides: Vec<ExtensionPoint>,
    #[serde(default)]
    pub permissions: Permissions,
    #[serde(default)]
    pub settings: Vec<SettingField>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageFile {
    pub path: String,
    pub size: u64,
    pub sha256: String,
}

/// `index.json.sig`.
#[derive(Debug, Clone, Deserialize)]
pub struct IndexSignature {
    pub algorithm: String,
    /// First 16 hex characters of the SHA-256 of the public key.
    pub key_id: String,
    /// Base64.
    pub signature: String,
}

/// Lowercase letters, digits and dashes, starting with a letter or digit.
pub fn valid_package_id(id: &str) -> bool {
    (1..=64).contains(&id.len())
        && !id.starts_with('-')
        && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// `1.2.3` as numbers. Nothing else is a version here.
pub fn parse_version(v: &str) -> Option<(u64, u64, u64)> {
    let mut parts = v.split('.');
    let mut next = || {
        let p = parts.next()?;
        (!p.is_empty() && p.len() <= 9 && p.chars().all(|c| c.is_ascii_digit())).then(|| p.parse().ok())?
    };
    let version = (next()?, next()?, next()?);
    parts.next().is_none().then_some(version)
}

/// Is `a` a newer version than `b`? False when either isn't a version.
pub fn is_newer(a: &str, b: &str) -> bool {
    matches!((parse_version(a), parse_version(b)), (Some(a), Some(b)) if a > b)
}

/// A file path inside a package: relative, plain names, nothing hidden, no `..`.
pub fn safe_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 200
        && path.split('/').all(|part| {
            !part.is_empty()
                && !part.starts_with(['.', '-'])
                && !part.ends_with('.')
                && part.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        })
}

fn is_sha256(s: &str) -> bool {
    s.len() == 64 && s.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f'))
}

/// A host name a module may ask for: no scheme, port, path, wildcard or bare IP.
pub fn valid_http_host(host: &str) -> bool {
    (1..=253).contains(&host.len())
        && host.contains('.')
        && !host.starts_with(['.', '-'])
        && !host.ends_with(['.', '-'])
        && host.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '-')
        && host.chars().any(|c| c.is_ascii_lowercase())
}

impl IndexVersion {
    /// Can this server run it?
    pub fn compatible(&self, kinora_version: &str) -> bool {
        let new_enough = match &self.min_kinora {
            None => true,
            Some(min) => match (parse_version(min), parse_version(kinora_version)) {
                (Some(min), Some(have)) => have >= min,
                _ => false,
            },
        };
        new_enough && self.abi.is_none_or(|abi| MODULE_ABIS.contains(&abi)) && parse_version(&self.version).is_some()
    }
}

impl IndexPackage {
    /// The newest version this server can run.
    pub fn best_version(&self, kinora_version: &str) -> Option<&IndexVersion> {
        self.versions
            .iter()
            .filter(|v| v.compatible(kinora_version))
            .max_by_key(|v| parse_version(&v.version).unwrap_or_default())
    }
}

impl Index {
    /// Drop entries this server can't use, so one odd package can't break the list.
    pub fn check(mut self) -> Result<Self, String> {
        if self.format != FORMAT {
            return Err(format!("this registry uses format {}, which this version of Kinora can't read", self.format));
        }
        self.packages.retain(|p| valid_package_id(&p.id));
        for p in &mut self.packages {
            p.versions.retain(|v| is_sha256(&v.sha256) && manifest_path_ok(&p.id, v));
            p.permissions.http.retain(|h| valid_http_host(h));
        }
        let mut seen = std::collections::BTreeSet::new();
        self.packages.retain(|p| seen.insert(p.id.clone()));
        Ok(self)
    }
}

/// Manifests live at a fixed place, so an index can't point a download elsewhere.
fn manifest_path_ok(id: &str, v: &IndexVersion) -> bool {
    parse_version(&v.version).is_some() && v.manifest == format!("packages/{id}/{}/package.json", v.version)
}

impl PackageManifest {
    /// Where one of this package's files is, relative to the registry's base URL.
    pub fn file_url_path(&self, file: &PackageFile) -> String {
        format!("packages/{}/{}/files/{}", self.id, self.version, file.path)
    }

    pub fn total_size(&self) -> u64 {
        self.files.iter().map(|f| f.size).sum()
    }

    /// Check a downloaded manifest against what the index promised and the
    /// rules every package follows. Nothing is written to disk before this passes.
    pub fn validate(&self, expected: &IndexPackage, version: &str) -> Result<(), String> {
        if self.format != FORMAT {
            return Err(format!("unknown package format {}", self.format));
        }
        if self.id != expected.id || self.kind != expected.kind || self.version != version {
            return Err("the package is not what the registry's index says it is".into());
        }
        if self.name.trim().is_empty() || self.name.len() > 120 {
            return Err("the package has no usable name".into());
        }
        if self.files.is_empty() || self.files.len() > MAX_FILES {
            return Err(format!("a package holds 1 to {MAX_FILES} files"));
        }
        let mut paths = std::collections::BTreeSet::new();
        for f in &self.files {
            if !safe_path(&f.path) {
                return Err(format!("unsafe file path {:?}", f.path));
            }
            if !is_sha256(&f.sha256) {
                return Err(format!("{}: bad checksum", f.path));
            }
            if f.size > MAX_FILE_BYTES {
                return Err(format!("{} is too large", f.path));
            }
            // Case-insensitive, so two files can't land on one name on Windows or macOS.
            if !paths.insert(f.path.to_lowercase()) {
                return Err(format!("{} is listed twice", f.path));
            }
        }
        if self.total_size() > MAX_PACKAGE_BYTES {
            return Err("the package is too large".into());
        }
        match (&self.kind, &self.module) {
            (PackageKind::Module, Some(m)) => m.validate(self),
            (PackageKind::Module, None) => Err("a module package needs a module block".into()),
            (_, Some(_)) => Err("only module packages have a module block".into()),
            (PackageKind::Theme, None) => {
                if self.files.iter().any(|f| f.path.contains('/') || !f.path.ends_with(".json")) {
                    return Err("a theme package holds only *.json files".into());
                }
                Ok(())
            }
            (_, None) => Ok(()),
        }
    }
}

impl ModuleSpec {
    fn validate(&self, manifest: &PackageManifest) -> Result<(), String> {
        if !MODULE_ABIS.contains(&self.abi) {
            return Err(format!("module ABI {} is not supported by this version of Kinora", self.abi));
        }
        if self.entry != MODULE_ENTRY || manifest.files.len() != 1 || manifest.files[0].path != MODULE_ENTRY {
            return Err(format!("a module package holds exactly one file, {MODULE_ENTRY}"));
        }
        if self.provides.is_empty() {
            return Err("the module provides nothing".into());
        }
        for p in &self.provides {
            if !p.is_module_point() {
                return Err(format!("modules can't provide {p:?}"));
            }
        }
        if let Some(bad) = self.permissions.http.iter().find(|h| !valid_http_host(h)) {
            return Err(format!("invalid host {bad:?} in permissions.http"));
        }
        if self.settings.len() > 40 {
            return Err("too many settings".into());
        }
        let mut keys = std::collections::BTreeSet::new();
        for f in &self.settings {
            let key_ok = (1..=40).contains(&f.key.len())
                && f.key.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
            if !key_ok || !keys.insert(&f.key) {
                return Err(format!("invalid setting key {:?}", f.key));
            }
        }
        Ok(())
    }

    /// The module's saved settings with defaults filled in for the rest.
    pub fn settings_with_defaults(&self, saved: &BTreeMap<String, String>) -> BTreeMap<String, String> {
        let mut out = saved.clone();
        for f in &self.settings {
            if let (false, Some(default)) = (out.contains_key(&f.key), &f.default) {
                out.insert(f.key.clone(), default.clone());
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INDEX: &str = r#"{
      "format": 1,
      "packages": [{
        "id": "tmdb", "kind": "module", "name": "TMDB", "description": "d", "recommended": true,
        "provides": ["metadata_provider"], "permissions": {"http": ["api.themoviedb.org", "bad host"]},
        "versions": [
          {"version": "2.0.0", "abi": 2, "manifest": "packages/tmdb/2.0.0/package.json", "sha256": "SHA"},
          {"version": "1.2.0", "abi": 1, "min_kinora": "9.0.0", "manifest": "packages/tmdb/1.2.0/package.json", "sha256": "SHA"},
          {"version": "1.10.0", "abi": 1, "manifest": "packages/tmdb/1.10.0/package.json", "sha256": "SHA"},
          {"version": "1.9.0", "abi": 1, "manifest": "packages/tmdb/1.9.0/package.json", "sha256": "SHA"},
          {"version": "1.11.0", "abi": 1, "manifest": "https://evil.example/package.json", "sha256": "SHA"}
        ]
      }, {"id": "../x", "kind": "theme", "name": "x", "description": "x"}]
    }"#;

    fn index() -> Index {
        serde_json::from_str::<Index>(&INDEX.replace("SHA", &"a".repeat(64))).unwrap().check().unwrap()
    }

    fn manifest(kind: &str, files: &str, module: &str) -> PackageManifest {
        serde_json::from_str(&format!(
            r#"{{"format":1,"id":"tmdb","kind":"{kind}","name":"TMDB","description":"d","version":"1.10.0",
                "files":[{files}]{module}}}"#
        ))
        .unwrap()
    }

    fn file(path: &str) -> String {
        format!(r#"{{"path":"{path}","size":10,"sha256":"{}"}}"#, "b".repeat(64))
    }

    const MODULE: &str = r#","module":{"abi":1,"entry":"module.wasm","provides":["metadata_provider"],
        "permissions":{"http":["api.themoviedb.org"]},
        "settings":[{"key":"api_key","label":"Key","kind":"secret","help":null},
                    {"key":"lang","label":"Language","kind":"text","help":null,"default":"en-US"}]}"#;

    #[test]
    fn versions() {
        assert_eq!(parse_version("1.2.3"), Some((1, 2, 3)));
        for bad in ["1.2", "1.2.3.4", "1.2.x", "v1.2.3", "1..3", "", "1.2.3-beta"] {
            assert_eq!(parse_version(bad), None, "{bad}");
        }
        assert!(is_newer("1.10.0", "1.9.9"));
        assert!(!is_newer("1.0.0", "1.0.0"));
        assert!(!is_newer("junk", "1.0.0"));
    }

    #[test]
    fn index_keeps_only_what_is_safe_and_picks_the_newest_usable_version() {
        let index = index();
        assert_eq!(index.packages.len(), 1, "the package with a bad id is dropped");
        let tmdb = &index.packages[0];
        assert_eq!(tmdb.permissions.http, vec!["api.themoviedb.org"]);
        // The version whose manifest pointed at another site is gone.
        assert!(tmdb.versions.iter().all(|v| v.version != "1.11.0"));
        // 2.0.0 needs an ABI we don't have, 1.2.0 a newer Kinora.
        assert_eq!(tmdb.best_version("0.1.0").unwrap().version, "1.10.0");
    }

    #[test]
    fn unknown_formats_are_refused() {
        let index: Index = serde_json::from_str(r#"{"format": 2, "packages": []}"#).unwrap();
        assert!(index.check().is_err());
    }

    #[test]
    fn manifests_are_checked_against_the_index() {
        let index = index();
        let tmdb = &index.packages[0];
        let good = manifest("module", &file("module.wasm"), MODULE);
        good.validate(tmdb, "1.10.0").unwrap();
        assert!(good.validate(tmdb, "1.9.0").is_err(), "wrong version");
        assert!(manifest("theme", &file("a.json"), "").validate(tmdb, "1.10.0").is_err(), "wrong kind");
        assert!(manifest("module", &file("module.wasm"), "").validate(tmdb, "1.10.0").is_err(), "no module block");
        let two = format!("{},{}", file("module.wasm"), file("extra.txt"));
        assert!(manifest("module", &two, MODULE).validate(tmdb, "1.10.0").is_err(), "extra files");
        let bad_host = MODULE.replace("api.themoviedb.org", "localhost");
        assert!(manifest("module", &file("module.wasm"), &bad_host).validate(tmdb, "1.10.0").is_err());
        let theme_point = MODULE.replace("metadata_provider", "theme");
        assert!(manifest("module", &file("module.wasm"), &theme_point).validate(tmdb, "1.10.0").is_err());
    }

    #[test]
    fn file_paths_cannot_escape() {
        for ok in ["skin.json", "pages/home.tv.html", "assets/logo_2.png"] {
            assert!(safe_path(ok), "{ok}");
        }
        for bad in ["../x", "/etc/passwd", "a//b", ".hidden", "a/.git/config", "a\\b", "", "a/", "con.", "C:x", "a b"] {
            assert!(!safe_path(bad), "{bad}");
        }
        let mut index = index();
        index.packages[0].kind = PackageKind::Theme;
        let theme = &index.packages[0];
        manifest("theme", &file("midnight.json"), "").validate(theme, "1.10.0").unwrap();
        assert!(manifest("theme", &file("sub/midnight.json"), "").validate(theme, "1.10.0").is_err());
        let twice = format!("{},{}", file("a.json"), file("A.json"));
        assert!(manifest("theme", &twice, "").validate(theme, "1.10.0").is_err());
    }

    #[test]
    fn http_hosts() {
        for ok in ["api.themoviedb.org", "discord.com", "a-b.example.co"] {
            assert!(valid_http_host(ok), "{ok}");
        }
        for bad in ["localhost", "127.0.0.1", "*.example.com", "example.com:8080", "https://x.com", "x.com/a", ""] {
            assert!(!valid_http_host(bad), "{bad}");
        }
    }

    #[test]
    fn settings_get_their_defaults() {
        let m = manifest("module", &file("module.wasm"), MODULE);
        let spec = m.module.unwrap();
        let mut saved = BTreeMap::new();
        assert_eq!(spec.settings_with_defaults(&saved).get("lang").map(String::as_str), Some("en-US"));
        saved.insert("lang".to_string(), "de-DE".to_string());
        let filled = spec.settings_with_defaults(&saved);
        assert_eq!(filled.get("lang").map(String::as_str), Some("de-DE"));
        assert!(!filled.contains_key("api_key"));
    }
}
