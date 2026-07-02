//! The `notes` sidecar: a self-contained vault server for Prompt's Notes
//! surface. Serves an embedded markdown-notes web app plus a vault API over
//! HTTP, with a WebSocket for external-change push. No runtime dependencies
//! (replaces the old Bun server).
//!
//! Usage: `notes serve [PORT]` (or `notes [PORT]`); default port 4319. The app
//! spawns it detached and reuses a live server; it reaps itself when idle.

mod server;
mod vault;

/// The fixed default port; the app health-checks and reuses a live server here.
const DEFAULT_PORT: u16 = 4319;

fn main() {
    // Accept `notes serve [port]` and a bare `notes [port]`.
    let mut args = std::env::args().skip(1).peekable();
    if args.peek().map(String::as_str) == Some("serve") {
        args.next();
    }
    let port = args.next().and_then(|s| s.parse().ok()).unwrap_or(DEFAULT_PORT);

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
