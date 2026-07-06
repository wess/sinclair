//! Git worktree verbs surfaced as workspace operations: create/open a worktree
//! as a new tab rooted at its path, list them, or remove one. Each mutation
//! fires a `worktree_created` / `worktree_removed` plugin trigger so plugins can
//! react. Reachable from keybind actions (`worktree_create:…`) and the MCP/IPC
//! bridge (`worktree_create`, `worktree_open`, `worktree_list`, `worktree_remove`).

use std::path::{Path, PathBuf};

use super::*;
use crate::view::TriggerEvent;

impl WorkspaceView {
    /// The repository directory to run worktree commands in: the focused pane's
    /// working directory.
    fn repo_dir(&self, cx: &App) -> Result<PathBuf, String> {
        self.focused_cwd_path(cx)
            .ok_or_else(|| "no working directory for the focused pane".to_string())
    }

    /// Create a worktree from a `path[@branch]` spec and open it in a new tab.
    pub(crate) fn worktree_create(
        &mut self,
        spec: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<PathBuf, String> {
        let repo = self.repo_dir(cx)?;
        let (path, branch) = split_spec(spec);
        let abs = crate::worktree::create(&repo, path, branch)?;
        self.open_worktree_tab(&abs, window, cx);
        let ev = TriggerEvent::WorktreeCreated(abs.to_string_lossy().into_owned());
        self.fire_workspace_trigger(&ev, Some(&abs), window, cx);
        Ok(abs)
    }

    /// Open an existing worktree `path` in a new tab.
    pub(crate) fn worktree_open(
        &mut self,
        path: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<PathBuf, String> {
        let repo = self.repo_dir(cx)?;
        let abs = if Path::new(path).is_absolute() {
            PathBuf::from(path)
        } else {
            repo.join(path)
        };
        if !abs.is_dir() {
            return Err(format!("worktree path does not exist: {}", abs.display()));
        }
        self.open_worktree_tab(&abs, window, cx);
        Ok(abs)
    }

    /// Remove the worktree at `path`; fires `worktree_removed`.
    pub(crate) fn worktree_remove(
        &mut self,
        path: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<PathBuf, String> {
        let repo = self.repo_dir(cx)?;
        let abs = crate::worktree::remove(&repo, path)?;
        let ev = TriggerEvent::WorktreeRemoved(abs.to_string_lossy().into_owned());
        self.fire_workspace_trigger(&ev, Some(&repo), window, cx);
        Ok(abs)
    }

    /// The repository's worktrees, as JSON for the `worktree_list` verb.
    pub(crate) fn worktree_list(&self, cx: &App) -> Result<Value, String> {
        let repo = self.repo_dir(cx)?;
        let list = crate::worktree::list(&repo)?;
        let items: Vec<Value> = list
            .into_iter()
            .map(|w| {
                json!({
                    "path": w.path.to_string_lossy(),
                    "branch": w.branch,
                })
            })
            .collect();
        Ok(json!({ "worktrees": items }))
    }

    /// Spawn a new tab rooted at `path`, labeled with its final component.
    fn open_worktree_tab(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.spawn_cwd(Some(path.to_path_buf()), window, cx) else {
            return;
        };
        self.group.update(cx, |g, cx| g.add_to_focused(id, cx));
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            self.rename_item(id, name, cx);
        }
        self.focusactive(window, cx);
        cx.notify();
    }
}

/// Split a `path[@branch]` spec into its path and optional branch name. The
/// branch is everything after the last `@` (so paths containing `@` still work
/// when the branch is omitted and there is no trailing `@`).
fn split_spec(spec: &str) -> (&str, Option<&str>) {
    match spec.rsplit_once('@') {
        Some((path, branch)) if !branch.is_empty() && !path.is_empty() => (path, Some(branch)),
        _ => (spec, None),
    }
}

#[cfg(test)]
#[path = "../../tests/worktrees.rs"]
mod tests;
