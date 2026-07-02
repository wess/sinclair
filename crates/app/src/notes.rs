//! Notes: a built-in markdown-vault surface, hosted as a webview tab. The vault
//! server ships as the bundled `notes` sidecar (no runtime dependency); this
//! module manages that sidecar directly — spawning it on demand like the relay
//! binary — and opens the surface via the shared `boot` webview flow. No
//! synthetic plugin, no plugin runtime.

use std::net::{Ipv4Addr, SocketAddrV4, TcpStream};
use std::time::Duration;

use gpui::{Context, Window};

use crate::pluginwebview::{Boot, SurfaceContent, WebviewSurface};
use crate::root::WorkspaceView;

/// The fixed port the vault server binds; the app health-checks and reuses it.
const PORT: u16 = 4319;

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

/// Is something accepting connections on `127.0.0.1:port`?
fn alive(port: u16) -> bool {
    let addr = SocketAddrV4::new(Ipv4Addr::LOCALHOST, port);
    TcpStream::connect_timeout(&addr.into(), Duration::from_millis(200)).is_ok()
}

/// Ensure the vault server is running and return its port, spawning the bundled
/// `notes serve` (detached) if nothing is listening yet. Runs off the UI thread
/// (called from the surface's boot), so the blocking wait is fine.
fn ensure_server() -> Result<u16, String> {
    if alive(PORT) {
        return Ok(PORT);
    }
    use std::os::unix::process::CommandExt;
    std::process::Command::new(binary())
        .args(["serve", &PORT.to_string()])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .process_group(0) // outlive the app that spawned it
        .spawn()
        .map_err(|e| format!("spawn `notes`: {e}. Is it installed beside prompt?"))?;
    for _ in 0..60 {
        if alive(PORT) {
            return Ok(PORT);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Err("the notes server did not start".to_string())
}

impl WorkspaceView {
    /// Open the Notes vault in a new tab.
    pub(crate) fn open_notes(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let surface = WebviewSurface {
            id: "notes".to_string(),
            title: "Notes".to_string(),
            content: SurfaceContent::Boot {
                url_template: "http://127.0.0.1:{port}/".to_string(),
                boot: Boot::Server(ensure_server),
            },
            runtime: None,
        };
        self.open_webview_tab(surface, window, cx);
    }
}
