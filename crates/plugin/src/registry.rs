//! Plugin registry: an index of installable plugins (name → source, version,
//! integrity checksum), and checksum verification. This replaces the "download
//! whatever is in one repo folder right now" catalog with a pinned, verifiable
//! index — the sandbox makes a real registry safe, and the checksum makes an
//! install tamper-evident (the lightweight stand-in for full signing).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// One registry entry.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct RegistryEntry {
    pub version: String,
    /// Where to fetch the plugin from (a base URL / repo path).
    pub url: String,
    /// Lowercase-hex SHA-256 of the plugin's `plugin.wasm` (empty = unpinned).
    #[serde(default)]
    pub sha256: String,
    #[serde(default)]
    pub description: String,
    /// Capabilities the plugin declares — shown for consent before install.
    #[serde(default)]
    pub capabilities: Vec<String>,
}

/// A registry index (an `index.toml`), keyed by plugin id.
#[derive(Serialize, Deserialize, Default, Debug)]
pub struct Registry {
    #[serde(default)]
    pub plugins: BTreeMap<String, RegistryEntry>,
}

impl Registry {
    /// Parse an index from TOML text.
    pub fn parse(text: &str) -> Option<Self> {
        toml::from_str(text).ok()
    }

    /// Sorted plugin ids in the index.
    pub fn names(&self) -> Vec<String> {
        self.plugins.keys().cloned().collect()
    }

    pub fn get(&self, id: &str) -> Option<&RegistryEntry> {
        self.plugins.get(id)
    }
}

/// Lowercase-hex SHA-256 of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut out = String::with_capacity(64);
    for b in Sha256::digest(bytes) {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

/// Whether `bytes` match the `expected` SHA-256 (lowercase hex). An empty
/// `expected` means unpinned (accepted, but not verified).
pub fn verify_sha256(bytes: &[u8], expected: &str) -> bool {
    expected.is_empty() || sha256_hex(bytes).eq_ignore_ascii_case(expected)
}

#[cfg(test)]
#[path = "../tests/registry.rs"]
mod tests;
