//! Notes: a built-in markdown-vault surface, hosted as a webview tab. The vault
//! server ships as the bundled `notes` sidecar (no runtime dependency); this
//! module opens the surface by handing the existing `boot` webview flow a
//! synthetic plugin whose runtime is that binary. The binary's launcher mode
//! spawns the server on demand and reports its port, which the webview loads.

use crate::root::WorkspaceView;
use gpui::{Context, Window};

/// The bundled `notes` binary: prefer a sibling of the running executable
/// (inside the app bundle / target dir), else fall back to `PATH`.
fn binary() -> String {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let cand = dir.join("notes");
            if cand.exists() {
                return cand.to_string_lossy().into_owned();
            }
        }
    }
    "notes".to_string()
}

/// A synthetic plugin describing the Notes webview: a `boot` surface whose
/// runtime is the bundled `notes` launcher. The launcher replies with the
/// server port, which is substituted into `{port}` and loaded.
fn descriptor() -> plugin::Plugin {
    let path = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    plugin::Plugin {
        id: "notes".into(),
        name: "Notes".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        description: Some("Markdown notes vault".into()),
        path,
        commands: Vec::new(),
        runtime: Some(plugin::Runtime { command: binary() }),
        panel: None,
        webview: Some(plugin::Webview {
            id: "notes".into(),
            title: "Notes".into(),
            icon: "\u{1F4DD}".into(), // 📝
            placement: plugin::Placement::Tab,
            source: plugin::WebviewSource::Url("http://127.0.0.1:{port}/".into()),
            boot: true,
        }),
        triggers: Vec::new(),
        tools: Vec::new(),
    }
}

impl WorkspaceView {
    /// Open the Notes vault in a new tab.
    pub(crate) fn open_notes(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_webview_tab(descriptor(), window, cx);
    }
}
