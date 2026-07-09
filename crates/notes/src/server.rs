//! The persistent Notes server: serves the embedded web app and the vault API
//! over HTTP and pushes external-change events over a WebSocket. Ported from
//! the original Bun server.
//!
//! Lifecycle depends on the mode (see `main.rs`): host-managed leaves teardown
//! to the host (plus a parent watch for a host that died uncleanly); standalone
//! publishes `server.json` and reaps itself when idle.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, Request, State};
use axum::http::{header, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, MethodRouter};
use axum::{Json, Router};
use include_dir::{include_dir, Dir};
use notify::{RecursiveMode, Watcher};
use serde_json::{json, Value};

use crate::vault::Vault;

/// The web app (index.html, app.js, style.css, dist/editor.js), embedded so the
/// binary is self-contained.
static WEB: Dir = include_dir!("$CARGO_MANIFEST_DIR/web");

pub struct AppState {
    vault: Mutex<Vault>,
    tx: tokio::sync::broadcast::Sender<String>,
    self_writes: Arc<Mutex<HashMap<String, Instant>>>,
    last_active: Mutex<Instant>,
    clients: AtomicUsize,
    /// Native folder dialogs currently in flight; holds off the idle reaper
    /// while the user is still browsing for a folder.
    picks: AtomicUsize,
    watcher: Mutex<Option<notify::RecommendedWatcher>>,
    port: u16,
    token: String,
}

/// RAII marker for an in-flight folder dialog. Drop-based so a cancelled
/// request (webview closed mid-pick) still releases the reaper hold.
struct PickGuard(Arc<AppState>);

impl PickGuard {
    fn new(s: &Arc<AppState>) -> Self {
        s.picks.fetch_add(1, Ordering::Relaxed);
        Self(s.clone())
    }
}

impl Drop for PickGuard {
    fn drop(&mut self) {
        self.0.picks.fetch_sub(1, Ordering::Relaxed);
        touch(&self.0);
    }
}

pub async fn run(port: u16, token: String, hosted: bool) {
    let (tx, _) = tokio::sync::broadcast::channel(64);
    let state = Arc::new(AppState {
        vault: Mutex::new(Vault::new()),
        tx,
        self_writes: Arc::new(Mutex::new(HashMap::new())),
        last_active: Mutex::new(Instant::now()),
        clients: AtomicUsize::new(0),
        picks: AtomicUsize::new(0),
        watcher: Mutex::new(None),
        port,
        token,
    });
    arm_watch(&state);

    // Everything that touches the vault (all `/api/*` plus the change-push
    // socket) sits behind the bearer-token gate; only `/health` and the static
    // web assets are public.
    let guarded = Router::new()
        .route("/ws", get(ws_upgrade))
        .route("/api/vault", get(get_vault))
        .route("/api/vault/open", post(vault_open))
        .route("/api/vault/create", post(vault_create))
        .route("/api/vault/pick", post(vault_pick))
        .route("/api/vault/forget", post(vault_forget))
        .route("/api/vaults/recents", get(recents))
        .route("/api/tree", get(tree))
        .route(
            "/api/file",
            get(file_get).put(file_put).post(file_post).delete(file_delete),
        )
        .route("/api/file/rename", post(file_rename))
        .route("/api/file/move", post(file_move))
        .route("/api/resolve", get(resolve))
        .route_layer(middleware::from_fn_with_state(state.clone(), auth));

    let app = Router::new()
        .route("/health", get(health))
        .merge(guarded)
        .fallback(static_asset)
        .with_state(state.clone());

    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(listener) => listener,
        Err(e) if hosted => {
            // The host reserved this exact port for us; not getting it is
            // fatal and must be visible as an exit, not a silent return.
            eprintln!("notes: bind 127.0.0.1:{port}: {e}");
            std::process::exit(1);
        }
        // Port busy — another server is already up; leave its token file intact.
        Err(_) => return,
    };
    if hosted {
        // The host owns the address (it passed it to us) and the teardown (it
        // SIGTERMs us on close/quit): no descriptor files, no idle reaper.
        spawn_parent_watch();
    } else {
        // Port 0 means the OS assigned a free port — publish the actual one.
        let port = listener.local_addr().map(|a| a.port()).unwrap_or(port);
        // We own the port: publish the token (0600) so a client can read it.
        crate::token::write_info(port, &state.token);
        spawn_reaper(state.clone());
    }
    let _ = axum::serve(listener, app).await;
}

/// Reject any guarded request whose bearer token doesn't match this session's.
/// Accepts `Authorization: Bearer <t>` (fetch) or `?token=<t>` (WebSocket).
async fn auth(State(s): State<Arc<AppState>>, req: Request, next: Next) -> Response {
    let presented: Option<String> = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::to_string)
        .or_else(|| {
            req.uri()
                .query()
                .and_then(crate::token::token_from_query)
                .map(str::to_string)
        });
    match presented {
        Some(t) if crate::token::constant_time_eq(&t, &s.token) => next.run(req).await,
        _ => StatusCode::UNAUTHORIZED.into_response(),
    }
}

// --- helpers ---------------------------------------------------------------

/// A shorthand for a POST-only route (axum 0.8 has no `post()` free fn import
/// clash with our module fn names, so alias it here).
fn post<H, T, S>(handler: H) -> MethodRouter<S>
where
    H: axum::handler::Handler<T, S>,
    T: 'static,
    S: Clone + Send + Sync + 'static,
{
    axum::routing::post(handler)
}

fn touch(state: &AppState) {
    if let Ok(mut t) = state.last_active.lock() {
        *t = Instant::now();
    }
}

fn ok(v: Value) -> Response {
    Json(v).into_response()
}

fn err(e: impl std::fmt::Display) -> Response {
    (StatusCode::BAD_REQUEST, Json(json!({ "error": e.to_string() }))).into_response()
}

fn broadcast_changed(state: &AppState, path: &str) {
    let _ = state.tx.send(json!({ "type": "changed", "path": path }).to_string());
}

fn mark_self(state: &AppState, rel: &str) {
    if let Ok(mut m) = state.self_writes.lock() {
        m.insert(rel.to_string(), Instant::now());
    }
}

/// (Re)establish the recursive file watcher on the current vault root; its
/// events become WebSocket `changed` broadcasts (skipping our own writes).
fn arm_watch(state: &Arc<AppState>) {
    let root = match state.vault.lock() {
        Ok(mut v) => v.current().map(|c| c.root),
        Err(_) => None,
    };
    let Some(root) = root else {
        if let Ok(mut w) = state.watcher.lock() {
            *w = None;
        }
        return;
    };
    let root_path = std::path::PathBuf::from(&root);
    let tx = state.tx.clone();
    let selfw = state.self_writes.clone();
    let base = root_path.clone();
    let handler = move |res: notify::Result<notify::Event>| {
        let Ok(event) = res else { return };
        for p in event.paths {
            let rel = p
                .strip_prefix(&base)
                .map(|r| r.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            if rel.is_empty() {
                continue;
            }
            if let Ok(mut m) = selfw.lock() {
                if let Some(t) = m.get(&rel) {
                    if t.elapsed() < Duration::from_millis(1500) {
                        continue;
                    }
                }
                m.retain(|_, t| t.elapsed() < Duration::from_secs(5));
            }
            let _ = tx.send(json!({ "type": "changed", "path": rel }).to_string());
        }
    };
    if let Ok(mut watcher) = notify::recommended_watcher(handler) {
        let _ = watcher.watch(&root_path, RecursiveMode::Recursive);
        if let Ok(mut w) = state.watcher.lock() {
            *w = Some(watcher);
        }
    }
}

/// Host-managed backstop: the host normally SIGTERMs us, but if it dies
/// without cleanup (crash, SIGKILL) we get reparented — notice that the parent
/// changed and exit rather than linger as an orphan.
fn spawn_parent_watch() {
    #[cfg(unix)]
    {
        let parent = std::os::unix::process::parent_id();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(2)).await;
                if std::os::unix::process::parent_id() != parent {
                    std::process::exit(0);
                }
            }
        });
    }
}

/// Standalone self-reaping: exit after a minute with no client and no dialog.
fn spawn_reaper(state: Arc<AppState>) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(5)).await;
            let idle = state
                .last_active
                .lock()
                .map(|t| t.elapsed())
                .unwrap_or_default();
            if state.clients.load(Ordering::Relaxed) == 0
                && state.picks.load(Ordering::Relaxed) == 0
                && idle > Duration::from_secs(60)
            {
                std::process::exit(0);
            }
        }
    });
}

// --- handlers --------------------------------------------------------------

async fn health(State(s): State<Arc<AppState>>) -> Response {
    touch(&s);
    ok(json!({ "server": "sinclair-notes", "port": s.port }))
}

async fn get_vault(State(s): State<Arc<AppState>>) -> Response {
    touch(&s);
    let cur = s.vault.lock().ok().and_then(|mut v| v.current());
    ok(serde_json::to_value(cur).unwrap_or(Value::Null))
}

/// Check that `dir` is a usable folder, off-thread and with a deadline.
/// A folder under macOS privacy protection (Documents, Desktop, …) blocks the
/// first `stat` until the user answers the consent prompt — probing on a
/// blocking thread with a timeout keeps the request (and the picker UI behind
/// it) from hanging forever, and turns a denial into an actionable message
/// instead of a bare "not a folder".
async fn probe_dir(dir: String, create: bool) -> Result<(), String> {
    let task = tokio::task::spawn_blocking(move || {
        if create {
            std::fs::create_dir_all(&dir).map_err(|e| match e.kind() {
                std::io::ErrorKind::PermissionDenied => deny_msg(),
                _ => e.to_string(),
            })?;
        }
        match std::fs::read_dir(&dir) {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => Err(deny_msg()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Err(format!("no such folder: {dir}"))
            }
            Err(e) => Err(format!("can't open {dir}: {e}")),
        }
    });
    match tokio::time::timeout(Duration::from_secs(15), task).await {
        Ok(Ok(res)) => res,
        Ok(Err(e)) => Err(e.to_string()),
        Err(_) => Err(
            "still waiting for folder access — if macOS is showing a permission \
             dialog, answer it, then try again"
                .to_string(),
        ),
    }
}

fn deny_msg() -> String {
    "macOS blocked access to that folder. Allow Sinclair under System Settings \
     → Privacy & Security → Files and Folders, then try again."
        .to_string()
}

async fn open_probed(s: &Arc<AppState>, dir: &str, create: bool) -> Response {
    if let Err(e) = probe_dir(dir.to_string(), create).await {
        return err(e);
    }
    let res = s.vault.lock().map_err(|e| e.to_string()).and_then(|mut v| {
        if create {
            v.create(dir)
        } else {
            v.open(dir)
        }
    });
    match res {
        Ok(info) => {
            arm_watch(s);
            ok(serde_json::to_value(info).unwrap_or(Value::Null))
        }
        Err(e) => err(e),
    }
}

async fn vault_open(State(s): State<Arc<AppState>>, Json(body): Json<Value>) -> Response {
    touch(&s);
    let path = body.get("path").and_then(Value::as_str).unwrap_or_default().to_string();
    open_probed(&s, &path, false).await
}

async fn vault_create(State(s): State<Arc<AppState>>, Json(body): Json<Value>) -> Response {
    touch(&s);
    let path = body.get("path").and_then(Value::as_str).unwrap_or_default().to_string();
    open_probed(&s, &path, true).await
}

async fn vault_pick(State(s): State<Arc<AppState>>, Json(body): Json<Value>) -> Response {
    touch(&s);
    let mode = body.get("mode").and_then(Value::as_str).unwrap_or("open");
    let dir = {
        let _guard = PickGuard::new(&s);
        pick_folder().await
    };
    let Some(dir) = dir else {
        let cur = s.vault.lock().ok().and_then(|mut v| v.current());
        return ok(serde_json::to_value(cur).unwrap_or(Value::Null));
    };
    open_probed(&s, &dir, mode == "create").await
}

async fn vault_forget(State(s): State<Arc<AppState>>, Json(body): Json<Value>) -> Response {
    touch(&s);
    let path = body.get("path").and_then(Value::as_str).unwrap_or_default();
    if let Ok(v) = s.vault.lock() {
        v.forget_recent(path);
        return ok(serde_json::to_value(v.recents()).unwrap_or(Value::Null));
    }
    ok(json!([]))
}

async fn recents(State(s): State<Arc<AppState>>) -> Response {
    touch(&s);
    let list = s.vault.lock().map(|v| v.recents()).unwrap_or_default();
    ok(serde_json::to_value(list).unwrap_or(Value::Null))
}

async fn tree(State(s): State<Arc<AppState>>) -> Response {
    touch(&s);
    match s.vault.lock().map_err(|e| e.to_string()).and_then(|v| v.tree()) {
        Ok(nodes) => ok(serde_json::to_value(nodes).unwrap_or(Value::Null)),
        Err(e) => err(e),
    }
}

async fn file_get(State(s): State<Arc<AppState>>, Query(q): Query<HashMap<String, String>>) -> Response {
    touch(&s);
    let path = q.get("path").map(String::as_str).unwrap_or_default();
    match s.vault.lock().map_err(|e| e.to_string()).and_then(|v| v.read(path)) {
        Ok(content) => ok(json!({ "content": content })),
        Err(e) => err(e),
    }
}

async fn file_put(State(s): State<Arc<AppState>>, Json(body): Json<Value>) -> Response {
    touch(&s);
    let path = body.get("path").and_then(Value::as_str).unwrap_or_default();
    let content = body.get("content").and_then(Value::as_str).unwrap_or_default();
    mark_self(&s, path);
    match s.vault.lock().map_err(|e| e.to_string()).and_then(|v| v.write(path, content)) {
        Ok(()) => ok(json!({ "ok": true })),
        Err(e) => err(e),
    }
}

async fn file_post(State(s): State<Arc<AppState>>, Json(body): Json<Value>) -> Response {
    touch(&s);
    let parent = body.get("parent").and_then(Value::as_str).unwrap_or_default();
    let kind = body.get("kind").and_then(Value::as_str).unwrap_or("file");
    let kind = if kind == "dir" { "dir" } else { "file" };
    match s.vault.lock().map_err(|e| e.to_string()).and_then(|v| v.create_file(parent, kind)) {
        Ok(path) => {
            broadcast_changed(&s, &path);
            ok(json!({ "path": path }))
        }
        Err(e) => err(e),
    }
}

async fn file_delete(State(s): State<Arc<AppState>>, Json(body): Json<Value>) -> Response {
    touch(&s);
    let path = body.get("path").and_then(Value::as_str).unwrap_or_default();
    match s.vault.lock().map_err(|e| e.to_string()).and_then(|v| v.remove(path)) {
        Ok(()) => {
            broadcast_changed(&s, path);
            ok(json!({ "ok": true }))
        }
        Err(e) => err(e),
    }
}

async fn file_rename(State(s): State<Arc<AppState>>, Json(body): Json<Value>) -> Response {
    touch(&s);
    let path = body.get("path").and_then(Value::as_str).unwrap_or_default();
    let title = body.get("title").and_then(Value::as_str).unwrap_or_default();
    match s.vault.lock().map_err(|e| e.to_string()).and_then(|v| v.rename(path, title)) {
        Ok(dest) => {
            broadcast_changed(&s, &dest);
            ok(json!({ "path": dest }))
        }
        Err(e) => err(e),
    }
}

async fn file_move(State(s): State<Arc<AppState>>, Json(body): Json<Value>) -> Response {
    touch(&s);
    let from = body.get("from").and_then(Value::as_str).unwrap_or_default();
    let to = body.get("to").and_then(Value::as_str).unwrap_or_default();
    match s.vault.lock().map_err(|e| e.to_string()).and_then(|v| v.move_to(from, to)) {
        Ok(dest) => {
            broadcast_changed(&s, &dest);
            ok(json!({ "path": dest }))
        }
        Err(e) => err(e),
    }
}

async fn resolve(State(s): State<Arc<AppState>>, Query(q): Query<HashMap<String, String>>) -> Response {
    touch(&s);
    let title = q.get("title").map(String::as_str).unwrap_or_default();
    match s.vault.lock().map_err(|e| e.to_string()).and_then(|v| v.resolve(title)) {
        Ok(path) => ok(json!({ "path": path })),
        Err(e) => err(e),
    }
}

// --- websocket -------------------------------------------------------------

async fn ws_upgrade(State(s): State<Arc<AppState>>, upgrade: WebSocketUpgrade) -> Response {
    upgrade.on_upgrade(move |socket| ws_task(socket, s))
}

async fn ws_task(mut socket: WebSocket, state: Arc<AppState>) {
    state.clients.fetch_add(1, Ordering::Relaxed);
    touch(&state);
    let mut rx = state.tx.subscribe();
    loop {
        tokio::select! {
            msg = rx.recv() => match msg {
                Ok(text) => {
                    if socket.send(Message::Text(text.into())).await.is_err() {
                        break;
                    }
                }
                Err(_) => continue,
            },
            incoming = socket.recv() => match incoming {
                Some(Ok(_)) => touch(&state),
                _ => break,
            },
        }
    }
    state.clients.fetch_sub(1, Ordering::Relaxed);
    touch(&state);
}

// --- static assets ---------------------------------------------------------

async fn static_asset(uri: axum::http::Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };
    match WEB.get_file(path) {
        Some(file) => {
            let mime = content_type(path);
            ([(header::CONTENT_TYPE, mime)], file.contents()).into_response()
        }
        None => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

fn content_type(path: &str) -> &'static str {
    match Path::new(path).extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") | Some("mjs") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    }
}

// --- folder picker ---------------------------------------------------------

/// Native "choose folder" dialog. macOS via `osascript`, Linux via
/// `zenity`/`kdialog` when present. `None` if cancelled or unavailable.
async fn pick_folder() -> Option<String> {
    #[cfg(target_os = "macos")]
    let cmd = {
        let mut c = tokio::process::Command::new("osascript");
        c.args([
            "-e",
            "POSIX path of (choose folder with prompt \"Choose a vault folder\")",
        ]);
        c
    };
    #[cfg(target_os = "linux")]
    let cmd = {
        let mut c = tokio::process::Command::new("zenity");
        c.args(["--file-selection", "--directory", "--title=Choose a vault folder"]);
        c
    };
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let cmd: Option<tokio::process::Command> = None::<tokio::process::Command>;

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        let mut cmd = cmd;
        let out = cmd.output().await.ok()?;
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let s = s.trim_end_matches('/').to_string();
        (!s.is_empty()).then_some(s)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = cmd;
        None
    }
}

#[cfg(test)]
mod tests {
    use super::probe_dir;

    #[tokio::test]
    async fn probe_accepts_a_real_folder() {
        let dir = std::env::temp_dir().join("sinclairnotesprobeok");
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(probe_dir(dir.to_string_lossy().into_owned(), false).await, Ok(()));
    }

    #[tokio::test]
    async fn probe_reports_missing_folder() {
        let e = probe_dir("/definitely/not/here".into(), false).await.unwrap_err();
        assert!(e.contains("no such folder"), "{e}");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn probe_maps_permission_denied_to_privacy_hint() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join("sinclairnotesprobedeny");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o000)).unwrap();
        let e = probe_dir(dir.to_string_lossy().into_owned(), false).await.unwrap_err();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(e.contains("Privacy & Security"), "{e}");
    }

    #[tokio::test]
    async fn probe_create_makes_the_folder() {
        let dir = std::env::temp_dir().join("sinclairnotesprobecreate/nested");
        let _ = std::fs::remove_dir_all(std::env::temp_dir().join("sinclairnotesprobecreate"));
        assert_eq!(probe_dir(dir.to_string_lossy().into_owned(), true).await, Ok(()));
        assert!(dir.is_dir());
    }
}
