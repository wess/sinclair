//! Single-instance IPC over a per-user unix socket.
//!
//! Two clients use it: `prompt --toggle-quick` summons the quick terminal
//! (the Wayland global-summon path, since a Wayland client cannot grab a
//! global hotkey itself), and `prompt mcp` bridges Model Context Protocol
//! tool calls into the running instance.
//!
//! The wire protocol is one newline-terminated JSON request per connection,
//! answered with one newline-terminated JSON response, then the connection
//! closes:
//!
//! ```text
//! request:  {"op":"run_command","args":{"text":"ls"}}
//! response: {"ok":true,"result":{ ... }}   |   {"ok":false,"error":"..."}
//! ```

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

use gpui::App;
use serde_json::{json, Value};

const POLL: Duration = Duration::from_millis(60);
/// How long the server waits for a client to send its request line.
const READ_TIMEOUT: Duration = Duration::from_millis(500);

/// Directory that holds the socket: `$XDG_RUNTIME_DIR` (already a private
/// per-user dir on Linux) when set, otherwise a per-user subdirectory of the
/// temp dir, never the shared temp root itself, which any local user can write.
fn socket_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("XDG_RUNTIME_DIR") {
        return PathBuf::from(dir);
    }
    let uid = unsafe { libc::getuid() };
    std::env::temp_dir().join(format!("prompt-{uid}"))
}

/// Per-user socket path under [`socket_dir`], keyed by app identity so a dev
/// build (`promptdev`) and an installed `prompt` own separate sockets and run as
/// fully independent instances. This is intentionally derived, not read from
/// `PROMPT_SOCKET`: the running app injects that var into child shells (so
/// external tooling can reach it), and honoring it here would make a dev build
/// launched from inside a Prompt session bind the parent's socket instead.
fn socket_path() -> PathBuf {
    socket_dir().join(format!("{}-quick.sock", crate::appid::id()))
}

/// Make sure the socket's directory exists and is private to us: a real
/// directory (not a symlink), owned by this uid, with no group/other access.
/// Returns false if it can't be made safe, we then refuse to listen rather
/// than expose a world-reachable control socket.
fn ensure_private_dir(dir: &Path) -> bool {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
    if !dir.exists() {
        let _ = std::fs::DirBuilder::new().mode(0o700).create(dir);
    }
    let Ok(meta) = std::fs::symlink_metadata(dir) else {
        return false;
    };
    if !meta.is_dir() || meta.uid() != unsafe { libc::getuid() } {
        return false;
    }
    if meta.permissions().mode() & 0o077 != 0 {
        let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
    }
    true
}

/// The connecting peer's uid, via the platform's socket-credential call.
/// `None` if the kernel won't report it.
fn peer_uid(stream: &UnixStream) -> Option<u32> {
    use std::os::unix::io::AsRawFd;
    let fd = stream.as_raw_fd();
    #[cfg(target_os = "macos")]
    {
        let mut uid: libc::uid_t = 0;
        let mut gid: libc::gid_t = 0;
        (unsafe { libc::getpeereid(fd, &mut uid, &mut gid) } == 0).then_some(uid)
    }
    #[cfg(target_os = "linux")]
    {
        let mut cred = libc::ucred {
            pid: 0,
            uid: 0,
            gid: 0,
        };
        let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
        let rc = unsafe {
            libc::getsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_PEERCRED,
                (&mut cred as *mut libc::ucred).cast(),
                &mut len,
            )
        };
        (rc == 0).then_some(cred.uid)
    }
}

/// The socket path as a string, for injecting into a spawned session's
/// environment (`PROMPT_SOCKET`) so an agent hook can reach the running instance
/// directly. Reporting via `prompt agent-status` derives the same path itself;
/// this is for external tooling that speaks the wire protocol.
pub fn socket_env() -> String {
    socket_path().to_string_lossy().into_owned()
}

/// Client: ask a running instance to toggle the quick terminal. Returns
/// whether one answered.
pub fn send_toggle() -> bool {
    match request("toggle_quick", &json!({})) {
        Ok(_) => true,
        Err(_) => {
            eprintln!("prompt: no running instance to toggle the quick terminal");
            false
        }
    }
}

/// Dev-only CLI: `prompt ipc <op> [json-args]`. Sends one op to the running
/// instance and prints the JSON reply (or the error). `json-args` defaults to
/// `{}`. Returns the process exit code. Used to script UI testing — e.g.
/// `prompt ipc send_input '{"text":"git st"}'` then
/// `prompt ipc read_suggestion '{}'`.
#[cfg(debug_assertions)]
pub fn run_cli(args: &[String]) -> i32 {
    let Some(op) = args.first() else {
        eprintln!("usage: prompt ipc <op> [json-args]");
        return 2;
    };
    let parsed = match args.get(1) {
        Some(raw) => match serde_json::from_str::<Value>(raw) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("prompt ipc: bad JSON args: {e}");
                return 2;
            }
        },
        None => json!({}),
    };
    match request(op, &parsed) {
        Ok(result) => {
            println!("{}", serde_json::to_string_pretty(&result).unwrap_or_default());
            0
        }
        Err(e) => {
            eprintln!("prompt ipc: {e}");
            1
        }
    }
}

/// Client: send one op to the running instance and return its result body, or
/// an error string (no instance, transport failure, or a server-side error).
pub fn request(op: &str, args: &Value) -> Result<Value, String> {
    let mut stream = UnixStream::connect(socket_path())
        .map_err(|_| "no running prompt instance".to_string())?;
    let line = json!({ "op": op, "args": args }).to_string();
    stream
        .write_all(line.as_bytes())
        .and_then(|()| stream.write_all(b"\n"))
        .and_then(|()| stream.flush())
        .map_err(|e| format!("write request: {e}"))?;
    let mut response = String::new();
    BufReader::new(&stream)
        .read_line(&mut response)
        .map_err(|e| format!("read response: {e}"))?;
    let value: Value =
        serde_json::from_str(response.trim()).map_err(|e| format!("bad response: {e}"))?;
    if value.get("ok").and_then(Value::as_bool) == Some(true) {
        Ok(value.get("result").cloned().unwrap_or(Value::Null))
    } else {
        Err(value
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("unknown error")
            .to_string())
    }
}

/// Server: own the socket (unless another instance already does) and service
/// one request per connection.
pub fn listen(cx: &mut App) {
    let Some(listener) = bind(&socket_path()) else {
        return;
    };
    if listener.set_nonblocking(true).is_err() {
        return;
    }
    let executor = cx.background_executor().clone();
    cx.spawn(async move |cx| loop {
        executor.timer(POLL).await;
        while let Ok((stream, _)) = listener.accept() {
            serve(stream, cx).await;
        }
    })
    .detach();
}

/// Read one request from `stream`, dispatch it against app state, and write
/// the response back.
async fn serve(stream: UnixStream, cx: &gpui::AsyncApp) {
    if peer_uid(&stream) != Some(unsafe { libc::getuid() }) {
        return;
    }
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(READ_TIMEOUT));
    let mut line = String::new();
    if BufReader::new(&stream).read_line(&mut line).is_err() {
        return;
    }
    let response = match serde_json::from_str::<Value>(line.trim()) {
        Ok(req) => {
            let op = req.get("op").and_then(Value::as_str).unwrap_or_default().to_string();
            let args = req.get("args").cloned().unwrap_or(Value::Null);
            match cx.update(|cx| crate::mcpbridge::handle(&op, &args, cx)) {
                Ok(result) => json!({ "ok": true, "result": result }),
                Err(error) => json!({ "ok": false, "error": error }),
            }
        }
        Err(e) => json!({ "ok": false, "error": format!("bad request: {e}") }),
    };
    let mut stream = stream;
    let _ = stream
        .write_all(response.to_string().as_bytes())
        .and_then(|()| stream.write_all(b"\n"))
        .and_then(|()| stream.flush());
}

/// Bind the socket inside a private directory, clearing a stale file left by a
/// crashed instance, and lock the socket to owner-only access.
fn bind(path: &Path) -> Option<UnixListener> {
    if let Some(dir) = path.parent() {
        if !ensure_private_dir(dir) {
            return None;
        }
    }
    let listener = match UnixListener::bind(path) {
        Ok(listener) => listener,
        Err(_) if UnixStream::connect(path).is_err() => {
            let _ = std::fs::remove_file(path);
            UnixListener::bind(path).ok()?
        }
        Err(_) => return None,
    };
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    Some(listener)
}
