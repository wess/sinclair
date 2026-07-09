//! Per-user data directories.
//!
//! Each lives under the app's config directory. When nothing exists at the
//! `sinclair` location but the pre-rename `prompt` one does, the old directory
//! is used, so upgrading keeps a user's notes, plugins, and webview storage
//! where they already are instead of silently starting empty.

use std::path::PathBuf;

/// `<config base>/sinclair/data/<sub>`, falling back to the `prompt` equivalent
/// when that exists and the `sinclair` one does not.
pub fn data_dir(sub: &str) -> PathBuf {
    let current = app_data("sinclair", sub);
    if !current.exists() {
        let legacy = app_data("prompt", sub);
        if legacy.exists() {
            return legacy;
        }
    }
    current
}

fn app_data(app: &str, sub: &str) -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join(app).join("data").join(sub)
}
