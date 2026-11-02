//! Persistent sessions: the GUI's side of the session server.
//!
//! With `persistent-sessions` on, panes are started in a separate
//! `sinclair _server` process (see `terminal::mux`) rather than in the GUI,
//! so a crash, a self-update, or a quit leaves every shell running. The next
//! launch reattaches the panes the saved session names and opens any other
//! session the server is still holding as a tab of its own.
//!
//! The server is started on demand, detached from the GUI (its own session
//! and process group, output to a log file), and exits by itself once it has
//! had nothing to run for a few seconds.

#[cfg(unix)]
use std::path::PathBuf;
#[cfg(unix)]
use std::time::{Duration, Instant};

/// How long the server lingers with no sessions and no clients.
#[cfg(unix)]
const IDLE_EXIT: Duration = Duration::from_secs(10);
/// How long to wait for a just-started server to answer.
#[cfg(unix)]
const STARTUP: Duration = Duration::from_secs(3);

/// Whether new panes should live in the session server.
pub fn enabled(opts: &config::Options) -> bool {
  cfg!(unix) && opts.persistent_sessions
}

/// The server's socket: next to the single-instance socket, keyed by app
/// identity so a dev build and an installed Sinclair keep separate servers.
#[cfg(unix)]
pub fn socket() -> PathBuf {
  crate::ipc::socket_dir().join(format!("{}-sessions.sock", crate::appid::id()))
}

/// `sinclair _server`: run the session server until it goes idle.
#[cfg(unix)]
pub fn run() -> i32 {
  let path = socket();
  if let Some(dir) = path.parent() {
    if !crate::ipc::ensure_private_dir(dir) {
      eprintln!("sinclair: {} is not a private directory", dir.display());
      return 1;
    }
  }
  match terminal::mux::server::serve(&path, IDLE_EXIT) {
    Ok(()) => 0,
    Err(e) => {
      eprintln!("sinclair: session server: {e}");
      1
    }
  }
}

/// Make sure a server is answering, starting one if not. `false` when none
/// could be reached; the caller then runs the pane in-process.
#[cfg(unix)]
pub fn ensure() -> bool {
  use std::os::unix::process::CommandExt;
  let path = socket();
  if terminal::mux::client::alive(&path) {
    return true;
  }
  let Ok(exe) = std::env::current_exe() else {
    return false;
  };
  let log_path = crate::paths::data_dir("logs").join("sessions.log");
  let _ = std::fs::create_dir_all(log_path.parent().unwrap_or(&log_path));
  let log = std::fs::OpenOptions::new()
    .create(true)
    .append(true)
    .open(&log_path);
  let mut cmd = std::process::Command::new(exe);
  cmd.arg("_server").stdin(std::process::Stdio::null());
  if let Ok(log) = log {
    if let Ok(err) = log.try_clone() {
      cmd.stdout(log).stderr(err);
    }
  }
  // Its own session: no controlling terminal, and nothing that happens to
  // the GUI's process group reaches it.
  // SAFETY: setsid is async-signal-safe and touches only the child.
  unsafe {
    cmd.pre_exec(|| {
      libc::setsid();
      Ok(())
    });
  }
  if cmd.spawn().is_err() {
    return false;
  }
  let deadline = Instant::now() + STARTUP;
  while Instant::now() < deadline {
    if terminal::mux::client::alive(&path) {
      return true;
    }
    std::thread::sleep(Duration::from_millis(25));
  }
  false
}
