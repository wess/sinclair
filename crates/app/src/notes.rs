//! Notes: the markdown-vault editor, a first-party surface backed by the
//! bundled `notes` sidecar binary.
//!
//! It was briefly a plugin, to prove the host-managed sidecar path worked for
//! plugins. With plugin-contributed web views gone, Notes is that path's only
//! user, so carrying a manifest for it bought nothing — File → Notes now builds
//! the surface directly and `crate::sidecar` runs the binary.

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
    /// Open the Notes editor in a new tab, backed by the bundled `notes`
    /// sidecar. The host allocates the sidecar's address and reaps it when the
    /// last tab using it closes — see [`crate::sidecar`].
    pub(crate) fn open_notes(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let surface = crate::webview::WebviewSurface {
            id: "notes".to_string(),
            title: "Notes".to_string(),
            command: "notes serve".to_string(),
            // The sidecar needs somewhere writable for its state; the app's own
            // directory may be read-only (it lives inside the bundle).
            dir: crate::paths::data_dir("notes"),
        };
        self.open_webview_tab(surface, window, cx);
    }
}
