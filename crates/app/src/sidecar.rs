//! Host-owned plugin sidecar lifecycle.
//!
//! A `[webview] service = "…"` surface asks the host to run its backend. The
//! host reserves a loopback port (bind `127.0.0.1:0`, read the assigned port,
//! drop the listener just before spawning), mints the session token, and hands
//! both to the child via `SINCLAIR_SERVICE_PORT` / `SINCLAIR_SERVICE_TOKEN`.
//! The child binds exactly that port; readiness is the port accepting
//! connections (bounded wait), not a descriptor file.
//!
//! Every spawned child stays a tracked child of the app. Services are keyed by
//! surface id and refcounted per live surface: a second window or tab reuses
//! the running service, and the child is reaped (SIGTERM, then SIGKILL after a
//! grace period) when the last surface using it closes. [`shutdown`] kills
//! everything at app quit.

use std::collections::HashMap;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

/// One live sidecar process and how many surfaces are using it.
struct Service {
    child: Child,
    port: u16,
    token: String,
    refs: usize,
}

static SERVICES: LazyLock<Mutex<HashMap<String, Service>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// How long a freshly spawned service gets to start accepting connections.
const READY: Duration = Duration::from_secs(5);
/// SIGTERM-to-SIGKILL grace when reaping a closed service.
const GRACE: Duration = Duration::from_secs(2);
/// SIGTERM-to-SIGKILL grace at app quit (gpui gives quit observers ~200ms).
const QUIT_GRACE: Duration = Duration::from_millis(120);
const POLL: Duration = Duration::from_millis(50);

/// Get a running service for `id`, starting `command` in `dir` if none is up.
/// Each successful call takes one reference; pair it with a [`release`].
pub fn acquire(id: &str, command: &str, dir: &Path) -> Result<(u16, String), String> {
    let mut services = SERVICES.lock().map_err(|_| "sidecar registry poisoned")?;
    let stale = match services.remove(id) {
        Some(mut s) => {
            let running = s.child.try_wait().ok().flatten().is_none();
            if running && port_alive(s.port) {
                s.refs += 1;
                let out = (s.port, s.token.clone());
                services.insert(id.to_string(), s);
                return Ok(out);
            }
            // Dead (or wedged before it could serve): clear it out, but keep
            // its reference count — surfaces that booted against the previous
            // incarnation still call `release` when they close.
            if running {
                let mut child = s.child;
                std::thread::spawn(move || terminate(&mut child));
            }
            s.refs
        }
        None => 0,
    };
    let (child, port, token) = start(command, dir)?;
    services.insert(
        id.to_string(),
        Service { child, port, token: token.clone(), refs: stale + 1 },
    );
    Ok((port, token))
}

/// Give back one surface's reference on `id`. When the last one goes, the
/// child is reaped off-thread so a closing tab never waits out the grace.
pub fn release(id: &str) {
    let Ok(mut services) = SERVICES.lock() else {
        return;
    };
    let Some(service) = services.get_mut(id) else {
        return;
    };
    service.refs = service.refs.saturating_sub(1);
    if service.refs > 0 {
        return;
    }
    if let Some(service) = services.remove(id) {
        let mut child = service.child;
        std::thread::spawn(move || terminate(&mut child));
    }
}

/// Kill every live sidecar. Called once when the app quits, on a short budget:
/// signal everyone, wait briefly, then SIGKILL stragglers.
pub fn shutdown() {
    let Ok(mut services) = SERVICES.lock() else {
        return;
    };
    let mut children: Vec<Child> = services.drain().map(|(_, s)| s.child).collect();
    drop(services);
    for child in &mut children {
        sigterm(child);
    }
    let deadline = Instant::now() + QUIT_GRACE;
    for mut child in children {
        reap(&mut child, deadline);
    }
}

/// Reserve a port, spawn `command` in `dir` with the port and a fresh token in
/// its environment, and wait for it to accept connections.
fn start(command: &str, dir: &Path) -> Result<(Child, u16, String), String> {
    std::fs::create_dir_all(dir).ok();
    let mut parts = command.split_whitespace();
    let program = resolve_program(parts.next().ok_or("empty service command")?);
    let args: Vec<&str> = parts.collect();
    let port = reserve_port()?;
    let token = mint_token();
    let mut cmd = Command::new(&program);
    cmd.args(&args)
        .current_dir(dir)
        .env("SINCLAIR_SERVICE_PORT", port.to_string())
        .env("SINCLAIR_SERVICE_TOKEN", &token)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // The child inherits the GUI parent's signal state, under which it was
    // observed (macOS) to shrug off the reaper's SIGTERM — every teardown
    // then waits out the SIGKILL grace. Give the child default dispositions
    // and an empty mask so a plain SIGTERM works.
    #[cfg(unix)]
    unsafe {
        use std::os::unix::process::CommandExt;
        cmd.pre_exec(|| {
            libc::signal(libc::SIGTERM, libc::SIG_DFL);
            libc::signal(libc::SIGINT, libc::SIG_DFL);
            libc::signal(libc::SIGHUP, libc::SIG_DFL);
            let mut set: libc::sigset_t = std::mem::zeroed();
            libc::sigemptyset(&mut set);
            libc::sigprocmask(libc::SIG_SETMASK, &set, std::ptr::null_mut());
            Ok(())
        });
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("spawn `{program}`: {e}"))?;
    wait_ready(&mut child, port)?;
    Ok((child, port, token))
}

/// Bind `127.0.0.1:0` and return the port the OS assigned. The listener is
/// dropped here, just before the spawn, so the child can take the port; the
/// tiny window in between is the accepted cost of not passing an fd.
fn reserve_port() -> Result<u16, String> {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0))
        .map_err(|e| format!("reserve a service port: {e}"))?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("read the reserved port: {e}"))?
        .port();
    Ok(port)
}

/// Wait (bounded) for the child to accept connections on its assigned port.
/// An early exit is reported as such; a timeout reaps the half-started child.
fn wait_ready(child: &mut Child, port: u16) -> Result<(), String> {
    let deadline = Instant::now() + READY;
    loop {
        if let Ok(Some(status)) = child.try_wait() {
            return Err(format!("the service exited during startup ({status})"));
        }
        if port_alive(port) {
            return Ok(());
        }
        if Instant::now() >= deadline {
            terminate(child);
            return Err("the service did not start listening in time".into());
        }
        std::thread::sleep(POLL);
    }
}

/// SIGTERM, wait out [`GRACE`], then SIGKILL.
fn terminate(child: &mut Child) {
    sigterm(child);
    reap(child, Instant::now() + GRACE);
}

/// Ask the child to exit. The pid is safe to signal here: the child hasn't
/// been waited on, so even a dead one is still a zombie holding its pid.
fn sigterm(child: &mut Child) {
    #[cfg(unix)]
    unsafe {
        libc::kill(child.id() as libc::pid_t, libc::SIGTERM);
    }
    #[cfg(not(unix))]
    {
        let _ = child.kill();
    }
}

/// Poll for exit until `deadline`, then SIGKILL and collect the child.
fn reap(child: &mut Child, deadline: Instant) {
    while Instant::now() < deadline {
        if child.try_wait().ok().flatten().is_some() {
            return;
        }
        std::thread::sleep(POLL);
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// Whether something is accepting connections on `127.0.0.1:port`.
fn port_alive(port: u16) -> bool {
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    std::net::TcpStream::connect_timeout(&addr, Duration::from_millis(200)).is_ok()
}

/// Resolve a bare program name to a sibling of the current executable if one
/// exists (bundled binaries like `notes` aren't on the user's `$PATH`); else
/// leave it for a normal `$PATH` lookup.
fn resolve_program(program: &str) -> String {
    if program.contains('/') {
        return program.to_string();
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(sibling) = exe.parent().map(|d| d.join(program)) {
            if sibling.is_file() {
                return sibling.to_string_lossy().into_owned();
            }
        }
    }
    program.to_string()
}

/// A fresh 128-bit token as lowercase hex, from the OS CSPRNG (with a
/// time-and-pid fallback so a token is never empty).
fn mint_token() -> String {
    let mut buf = [0u8; 16];
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        use std::io::Read;
        if f.read_exact(&mut buf).is_ok() {
            return hex(&buf);
        }
    }
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
        ^ (std::process::id() as u128);
    hex(&n.to_le_bytes())
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        s.push(DIGITS[(b >> 4) as usize] as char);
        s.push(DIGITS[(b & 0xf) as usize] as char);
    }
    s
}

#[cfg(test)]
#[path = "../tests/sidecar.rs"]
mod tests;
