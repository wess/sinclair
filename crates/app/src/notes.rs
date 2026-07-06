//! Notes: the markdown-vault editor, opened as a webview tab backed by the
//! bundled `notes` sidecar via the **host-managed service** webview path — the
//! same `[[webview]] service` mechanism any plugin uses (see `plugins/notes`).
//! The old bespoke server boot (`ensure_server` + `Boot::Server` fn-pointer +
//! the hardcoded port) is retired: `run_service` spawns `notes serve 0`, notes
//! picks a free port and reports `{port, token}` via `.service.json`, and the
//! page loads from that origin.

use gpui::{Context, Window};

use crate::pluginwebview::{Boot, SurfaceContent, WebviewSurface};
use crate::root::WorkspaceView;

/// Where the notes sidecar runs and writes its `.service.json` (its data dir).
fn notes_dir() -> std::path::PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    base.join("prompt").join("notes")
}

impl WorkspaceView {
    /// Open the Notes vault in a new tab via the host-managed sidecar.
    pub(crate) fn open_notes(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let surface = WebviewSurface {
            id: "notes".to_string(),
            title: "Notes".to_string(),
            content: SurfaceContent::Boot {
                url_template: "http://127.0.0.1:{port}/?token={token}".to_string(),
                boot: Boot::Command {
                    command: "notes serve 0".to_string(),
                    dir: notes_dir(),
                },
            },
            runtime: None,
        };
        self.open_webview_tab(surface, window, cx);
    }
}
