//! Persisted window session: where the window was, how its docks were arranged,
//! and the tabs, split layouts, per-pane working directories, titles and
//! buffers inside it. Saved on quit and restored on launch when
//! `session-restore` is on. One window's worth of state (the last to save).
//!
//! The buffers make this file a copy of what was on your screens, which is as
//! private as the panes it came from: it is written owner-only, through a temp
//! file renamed over the target so a crash mid-write can't leave a half-written
//! session behind.
//!
//! The division of labour with `settings.json`: **config is the default,
//! the session is the memory.** Config says how a fresh window should look —
//! which dock sections exist, in what order, how wide, `window-width` in cells.
//! The session says how *your* window actually was when you closed it. So
//! dragging a dock open never rewrites your settings, and clearing the session
//! returns you to the configured defaults.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::tiles::Layout;

/// One restored tab: its split tree, the working directory of each pane (in
/// pre-order leaf order), and the tab title. For panes that were running a
/// reporting agent, `commands`/`sessions` carry the launch command and the
/// native session id so the agent can be relaunched and resumed on restore.
/// `buffers` carries what each pane had on screen and in scrollback.
#[derive(Clone, Serialize, Deserialize)]
pub struct TabState {
  pub layout: Layout,
  #[serde(default)]
  pub cwds: Vec<Option<String>>,
  #[serde(default)]
  pub title: Option<String>,
  /// Per-pane launch command (pre-order leaf order); `None` for plain shells.
  #[serde(default)]
  pub commands: Vec<Option<String>>,
  /// Per-pane native agent session id (pre-order leaf order); `None` when the
  /// pane wasn't running a session-reporting agent.
  #[serde(default)]
  pub sessions: Vec<Option<String>>,
  /// Per-pane buffer dump (pre-order leaf order): the pane's last lines as
  /// the escape sequences that redraw them, replayed into the restored pane
  /// before its shell starts. `None` when there was nothing to keep or
  /// `session-restore-lines` is `0`. Absent for a session saved before
  /// buffers were kept, which restores as it always did.
  #[serde(default)]
  pub buffers: Vec<Option<String>>,
}

/// Where the window was on screen, in logical pixels. Restored as-is when it
/// still lands on a display that exists — see [`WindowState::usable`].
#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct WindowState {
  pub x: f32,
  pub y: f32,
  pub width: f32,
  pub height: f32,
}

impl WindowState {
  /// Whether these bounds are worth restoring: a positive size, and not so
  /// large it can only have come from a corrupt file. A window saved on a
  /// display that is now unplugged is *not* rejected here — the platform
  /// clamps an off-screen window back onto a real display, which is the
  /// behaviour people expect when they undock a laptop.
  pub fn usable(&self) -> bool {
    self.width >= 200.0 && self.height >= 200.0 && self.width < 30_000.0 && self.height < 30_000.0
  }
}

/// One dock's live state. Sections are keyed by **token**, never by index: the
/// index of a plugin section moves when the plugin set changes, so a stored
/// index would silently reattach to a different section. An unknown token is
/// ignored on restore and a section with no stored entry keeps its configured
/// default.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct DockState {
  pub open: bool,
  pub width: f32,
  /// `(section token, expanded)`.
  #[serde(default)]
  pub sections: Vec<(String, bool)>,
}

/// A whole window: where it was, how its docks stood, its tabs, and which tab
/// was active.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct SessionState {
  pub tabs: Vec<TabState>,
  #[serde(default)]
  pub active: usize,
  /// Absent for a session saved before window geometry was recorded, or when
  /// the window was minimised/full-screen at save time.
  #[serde(default)]
  pub window: Option<WindowState>,
  /// `[left, right]`. Absent for an older session.
  #[serde(default)]
  pub docks: Option<[DockState; 2]>,
}

fn path() -> Option<PathBuf> {
  config::default_path().and_then(|p| p.parent().map(|d| d.join("session.json")))
}

/// A session file larger than this is not one we wrote: the buffers are
/// bounded to a couple of megabytes at the source. Refuse to parse it rather
/// than spend launch time and memory on it.
const MAX_SESSION_FILE_BYTES: u64 = 32 * 1024 * 1024;

/// Write the session to disk (best-effort): owner-only, and atomically, so a
/// crash on quit costs the *new* session rather than the one already saved.
pub fn save(state: &SessionState) {
  let Some(p) = path() else {
    return;
  };
  let Some(dir) = p.parent() else {
    return;
  };
  let _ = std::fs::create_dir_all(dir);
  let Ok(json) = serde_json::to_vec_pretty(state) else {
    return;
  };
  persist(&p, &json);
}

/// Write `json` to `path` through a temp file in the same directory, renamed
/// over the target once it is complete. The rename also replaces the file's
/// permissions with the temp file's, so a session saved by an older version
/// as world-readable is tightened by the next save.
fn persist(path: &std::path::Path, json: &[u8]) {
  let Some(dir) = path.parent() else {
    return;
  };
  let tmp = dir.join(format!(".session.json.{}.tmp", std::process::id()));
  if write_private(&tmp, json).is_err() {
    let _ = std::fs::remove_file(&tmp);
    return;
  }
  if std::fs::rename(&tmp, path).is_err() {
    let _ = std::fs::remove_file(&tmp);
  }
}

/// Create `path` owner-only and write `json` into it. The mode is set as the
/// file is created rather than after: this holds terminal output, and a
/// window where it sits world-readable is a window too many.
fn write_private(path: &std::path::Path, json: &[u8]) -> std::io::Result<()> {
  use std::io::Write;
  let mut options = std::fs::OpenOptions::new();
  options.write(true).create(true).truncate(true);
  #[cfg(unix)]
  {
    use std::os::unix::fs::OpenOptionsExt;
    options.mode(0o600);
  }
  let mut file = options.open(path)?;
  file.write_all(json)?;
  file.flush()
}

/// Read the saved session, or `None` if absent, oversized, or unreadable.
pub fn load() -> Option<SessionState> {
  let path = path()?;
  if std::fs::metadata(&path).ok()?.len() > MAX_SESSION_FILE_BYTES {
    eprintln!("sinclair: ignoring an implausibly large session file");
    return None;
  }
  let bytes = std::fs::read(path).ok()?;
  serde_json::from_slice(&bytes).ok()
}

#[cfg(test)]
#[path = "../tests/sessionstate.rs"]
mod tests;
