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
