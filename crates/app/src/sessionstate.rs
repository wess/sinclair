//! Persisted window session: the tabs, their split layouts, per-pane working
//! directories, and titles. Saved on quit and restored on launch when
//! `session-restore` is on. One window's worth of state (the last to save).

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

/// A whole window: its tabs and which one was active.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct SessionState {
    pub tabs: Vec<TabState>,
    #[serde(default)]
    pub active: usize,
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
