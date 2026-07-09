//! Notes: the markdown-vault editor. It is now the first-party **Notes plugin**
//! (`plugins/notes`), bundled with the app and opened through the plugin system
//! like any other `[webview] service` plugin — the manifest is the single source
//! of truth. File → Notes resolves the loaded `notes` plugin and opens it.
//!
//! For a bare binary run outside a bundle (`cargo run -p app --release` — no
//! `Contents/Resources/plugins`), the plugin may not be discoverable; the
//! fallback opens the same bundled `notes` sidecar directly, identical behavior
//! minus the manifest indirection.

use gpui::{Context, Window};

use crate::pluginwebview::{Boot, SurfaceContent, WebviewSurface};
use crate::root::WorkspaceView;

/// Working dir for the fallback sidecar, matching the plugin path's data dir.
fn notes_dir() -> std::path::PathBuf {
    crate::paths::data_dir("notes")
}

impl WorkspaceView {
    /// Open the Notes vault in a new tab. Prefers the bundled Notes plugin;
    /// falls back to spawning the sidecar directly when it isn't loaded.
    pub(crate) fn open_notes(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.has_webview_plugin("notes") {
            self.open_webview("notes", window, cx);
            return;
        }
        let surface = WebviewSurface {
            id: "notes".to_string(),
            title: "Notes".to_string(),
            content: SurfaceContent::Boot {
                url_template: "http://127.0.0.1:{port}/?token={token}".to_string(),
                boot: Boot::Command {
                    command: "notes serve".to_string(),
                    dir: notes_dir(),
                },
            },
            runtime: None,
        };
        self.open_webview_tab(surface, window, cx);
    }
}
