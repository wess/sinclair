//! Notes: the markdown-vault editor. It is the first-party **Notes plugin**
//! (`plugins/notes`), bundled with the app and opened through the plugin system
//! like any other `[webview] service` plugin — the manifest is the single source
//! of truth. File → Notes resolves the loaded `notes` plugin and opens it.
//!
//! There is no hand-rolled fallback boot: bundled-plugin discovery covers every
//! layout (the `.app` bundle, a Linux prefix, a portable dir, and — in debug
//! builds — the workspace `plugins/` dir, see `plugin::load`), so a missing
//! plugin is a real installation problem and is reported as one.

use gpui::{Context, Window};

use crate::root::WorkspaceView;

/// One vault the Notes editor has opened before, as recorded in
/// `~/.config/sinclair/notes/vaults.json`.
///
/// Read straight off disk rather than through the `notes` crate: that crate is
/// a binary-only sidecar (tokio + axum + notify), and pulling all of it into
/// the terminal to list a few directory names is not a trade worth making. The
/// file is written by `notes::Vault::remember_recent`; only these two fields
/// are needed here, and serde ignores the rest.
#[derive(serde::Deserialize)]
pub(crate) struct Vault {
    pub path: String,
    pub name: String,
}

/// The vaults file the Notes sidecar maintains. Hard-wired to `~/.config`
/// rather than `XDG_CONFIG_HOME` because that is where the sidecar writes it
/// (`notes::vault::config_dir`); reading somewhere else would silently show an
/// empty list.
fn vaults_file() -> Option<std::path::PathBuf> {
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from)?;
    Some(
        home.join(".config")
            .join("sinclair")
            .join("notes")
            .join("vaults.json"),
    )
}

/// Every remembered vault that still exists on disk. Empty on any failure —
/// a missing or malformed file just means "no vaults yet", never an error.
pub(crate) fn vaults() -> Vec<Vault> {
    let Some(file) = vaults_file() else {
        return Vec::new();
    };
    let Ok(text) = std::fs::read_to_string(file) else {
        return Vec::new();
    };
    serde_json::from_str::<Vec<Vault>>(&text)
        .unwrap_or_default()
        .into_iter()
        .filter(|v| std::path::Path::new(&v.path).exists())
        .collect()
}

impl WorkspaceView {
    /// Open the Notes vault in a new tab via the bundled Notes plugin.
    pub(crate) fn open_notes(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.has_webview_plugin("notes") {
            self.open_webview("notes", window, cx);
        } else {
            eprintln!(
                "sinclair: the Notes plugin isn't loaded — expected a `notes` plugin \
                 bundled with the app or installed in the plugins directory"
            );
        }
    }
}
