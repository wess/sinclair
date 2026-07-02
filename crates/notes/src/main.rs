//! The `notes` sidecar: a self-contained vault server for Prompt's Notes
//! surface. Serves an embedded markdown-notes web app plus a vault API over
//! HTTP, with a WebSocket for external-change push. No runtime dependencies
//! (replaces the old Bun server).
//!
//! Two modes:
//! - `notes serve [PORT]` — run the HTTP/WS server (default port 4319).
//! - `notes` / `notes boot` — launcher: read a plugin `boot` request on stdin,
//!   ensure the server is running (spawn it detached if not), and reply
//!   `{"result":{"port":PORT}}` so the app's `boot` webview can navigate to it.

mod server;
mod vault;

use std::io::Read;
use std::net::{Ipv4Addr, SocketAddrV4, TcpStream};
use std::time::Duration;

/// The fixed default port; the app health-checks and reuses a live server here.
const DEFAULT_PORT: u16 = 4319;

fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("serve") => {
            let port = std::env::args()
                .nth(2)
                .and_then(|s| s.parse().ok())
                .unwrap_or(DEFAULT_PORT);
            serve(port);
        }
        // "boot" or no args: act as the plugin-runtime launcher.
        _ => launch(),
    }
}

/// Run the server (blocking).
fn serve(port: u16) {
    write_pidfile(port);
    let rt = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("notes: runtime: {e}");
            std::process::exit(1);
        }
    };
    rt.block_on(server::run(port));
}

/// Launcher: the app invokes this over the plugin bridge (`boot`). Ensure the
/// server is up, then reply with its port on stdout as a plugin response.
fn launch() {
    // Drain the request (its content doesn't matter — any invoke means "boot").
    let mut buf = Vec::new();
    let _ = std::io::stdin().read_to_end(&mut buf);

    ensure_server(DEFAULT_PORT);

    // The pluginhost expects a JSON response `{ result: … }`; the boot webview
    // reads `result.port` and navigates to the templated URL.
    println!("{{\"result\":{{\"port\":{DEFAULT_PORT}}}}}");
}

/// Return once a server is listening on `port`, spawning one (detached) if none
/// is up yet.
fn ensure_server(port: u16) {
    if alive(port) {
        return;
    }
    if let Ok(exe) = std::env::current_exe() {
        use std::os::unix::process::CommandExt;
        let _ = std::process::Command::new(exe)
            .args(["serve", &port.to_string()])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .process_group(0) // survive this launcher's exit
            .spawn();
    }
    // Wait (up to ~3s) for it to bind.
    for _ in 0..60 {
        if alive(port) {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Is something accepting connections on `127.0.0.1:port`?
fn alive(port: u16) -> bool {
    let addr = SocketAddrV4::new(Ipv4Addr::LOCALHOST, port);
    TcpStream::connect_timeout(&addr.into(), Duration::from_millis(200)).is_ok()
}

/// Record `{port, pid}` at `~/.config/prompt/notes/server.json`.
fn write_pidfile(port: u16) {
    let Some(home) = std::env::var_os("HOME") else {
        return;
    };
    let dir = std::path::Path::new(&home).join(".config").join("prompt").join("notes");
    let _ = std::fs::create_dir_all(&dir);
    let body = serde_json::json!({ "port": port, "pid": std::process::id() });
    let _ = std::fs::write(dir.join("server.json"), body.to_string());
}
