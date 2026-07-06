//! Install state: which plugins are installed, their version and source, whether
//! they're enabled, and the capabilities the user consented to. This replaces
//! "a folder exists and parses" with an explicit record (`installed.toml`), so a
//! plugin can be disabled without deleting it, updates are visible, and a
//! plugin's capabilities are what the user *granted*, not what it self-declares.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

fn yes() -> bool {
    true
}

/// One installed plugin's record.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct Entry {
    /// The version recorded at install (for pinning / update detection).
    #[serde(default)]
    pub version: String,
    /// Where it came from: `builtin`, `catalog:<name>`, `local:<path>`, …
    #[serde(default)]
    pub source: String,
    /// Enabled plugins load; a disabled one stays installed but inert.
    #[serde(default = "yes")]
    pub enabled: bool,
    /// The capabilities the user consented to at install. A plugin may only reach
    /// the intersection of what it declares and what was granted.
    #[serde(default)]
    pub granted: Vec<String>,
}

/// The `installed.toml` state, keyed by plugin id.
#[derive(Serialize, Deserialize, Default, Debug)]
pub struct Installed {
    #[serde(default)]
    pub plugins: BTreeMap<String, Entry>,
}

impl Installed {
    /// `$XDG_CONFIG_HOME/prompt/installed.toml` (beside the plugins dir).
    pub fn path() -> Option<PathBuf> {
        Some(crate::defaultdir()?.parent()?.join("installed.toml"))
    }

    /// Load the record, or an empty one if absent/unparsable (never fails).
    pub fn load() -> Self {
        Self::path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|text| toml::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// Persist the record.
    pub fn save(&self) -> std::io::Result<()> {
        let Some(path) = Self::path() else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = toml::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(path, text)
    }

    /// Whether `id` should load. Untracked plugins (built-ins, freshly dropped-in
    /// dirs) default to enabled, so the record is opt-in.
    pub fn is_enabled(&self, id: &str) -> bool {
        self.plugins.get(id).map(|e| e.enabled).unwrap_or(true)
    }

    /// Enable or disable `id`, creating a record if needed.
    pub fn set_enabled(&mut self, id: &str, enabled: bool) {
        self.plugins.entry(id.to_string()).or_default().enabled = enabled;
    }

    /// The capabilities granted to `id` (empty if untracked).
    pub fn granted(&self, id: &str) -> &[String] {
        self.plugins.get(id).map(|e| e.granted.as_slice()).unwrap_or(&[])
    }

    /// Record an install (or update): version, source, and the granted caps the
    /// user consented to.
    pub fn record(&mut self, id: &str, version: &str, source: &str, granted: Vec<String>) {
        self.plugins.insert(
            id.to_string(),
            Entry {
                version: version.to_string(),
                source: source.to_string(),
                enabled: true,
                granted,
            },
        );
    }
}

impl Default for Entry {
    fn default() -> Self {
        Entry {
            version: String::new(),
            source: String::new(),
            enabled: true,
            granted: Vec::new(),
        }
    }
}

#[cfg(test)]
#[path = "../tests/install.rs"]
mod tests;
