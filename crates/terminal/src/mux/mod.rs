//! Sessions that outlive the app: the session server and its client.
//!
//! A session normally lives inside the GUI process — its pty, its child, its
//! terminal state — so anything that ends the GUI (a crash, a self-update, a
//! quit) ends every shell with it. The session server is a separate process
//! (`sinclair _server`) that owns the ptys instead. The GUI attaches to its
//! sessions over a unix socket: on attach the server sends a snapshot of the
//! session's terminal (`vt::Terminal::snapshot`), then streams raw child
//! output, which the GUI feeds into a terminal of its own. Everything the GUI
//! reads — rendering, search, selection — keeps working on that local copy
//! unchanged; only keystrokes, resizes and kills cross back.
//!
//! When the GUI goes away the server notices the connection drop and keeps
//! the session. The next GUI lists the server's sessions and attaches again.

pub mod frame;

#[cfg(unix)]
pub mod client;
#[cfg(unix)]
pub mod server;

use std::sync::atomic::{AtomicBool, Ordering};

/// What to attach to.
#[derive(Debug, Clone)]
pub enum Attach {
  /// Start a new session from these options, then attach. `token` is the
  /// GUI's pane token, kept with the session so a later GUI can give the
  /// pane the same `SINCLAIR_PANE` its shell already has.
  Open {
    options: crate::SessionOptions,
    token: u64,
  },
  /// Attach to the running session with this id.
  Existing(String),
}

/// The session an attach landed on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attached {
  pub id: String,
  pub token: u64,
  pub cols: usize,
  pub rows: usize,
  pub busy: bool,
}

/// A session the server is running, as `list` reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listed {
  pub id: String,
  pub token: u64,
  pub title: String,
  pub clients: usize,
}

static DETACHING: AtomicBool = AtomicBool::new(false);

/// From now on, dropping a server-owned session lets it go instead of
/// killing it. The app sets this as it quits, so its sessions survive it;
/// closing a pane while running still ends that pane's session.
pub fn detach_on_drop(on: bool) {
  DETACHING.store(on, Ordering::SeqCst);
}

pub(crate) fn detaching() -> bool {
  DETACHING.load(Ordering::SeqCst)
}
