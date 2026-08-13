//! Persisted window session: where the window was, how its docks were arranged,
//! and the tabs, split layouts, per-pane working directories and titles inside
//! it. Saved on quit and restored on launch when `session-restore` is on. One
//! window's worth of state (the last to save).
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
        self.width >= 200.0
            && self.height >= 200.0
            && self.width < 30_000.0
            && self.height < 30_000.0
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

/// Write the session to disk (best-effort).
pub fn save(state: &SessionState) {
    let Some(p) = path() else {
        return;
    };
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_vec_pretty(state) {
        let _ = std::fs::write(p, json);
    }
}

/// Read the saved session, or `None` if absent/unreadable.
pub fn load() -> Option<SessionState> {
    let bytes = std::fs::read(path()?).ok()?;
    serde_json::from_slice(&bytes).ok()
}

#[cfg(test)]
#[path = "../tests/sessionstate.rs"]
mod tests;
