//! Git worktree operations: create, list, and remove worktrees by shelling out
//! to `git`. The workspace turns a create/open into a new tab rooted at the
//! worktree path and fires `worktree_created` / `worktree_removed` plugin
//! triggers so plugins can react (install deps, open an editor, …).

use std::path::{Path, PathBuf};
use std::process::Command;

/// One entry from `git worktree list --porcelain`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Worktree {
    pub path: PathBuf,
    /// The checked-out branch (`refs/heads/…` stripped), if any.
    pub branch: Option<String>,
}

/// Run `git` in `dir` with `args`, returning stdout on success or git's stderr
/// (trimmed) as a friendly error.
fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .map_err(|e| format!("could not run git: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        let msg = err.trim();
        Err(if msg.is_empty() {
            "git command failed".to_string()
        } else {
            msg.to_string()
        })
    }
}

/// Resolve `path` against `base`: absolute paths are used as-is, relative ones
/// are joined onto `base` (the repository directory).
fn resolve(base: &Path, path: &str) -> PathBuf {
    let p = PathBuf::from(path);
    if p.is_absolute() {
        p
    } else {
        base.join(p)
    }
}

/// Create a worktree at `path` (relative to `repo` when not absolute). With a
/// `branch`, a new branch of that name is created from HEAD; without one, git
/// derives a branch from the final path component. Returns the worktree path.
pub fn create(repo: &Path, path: &str, branch: Option<&str>) -> Result<PathBuf, String> {
    let abs = resolve(repo, path);
    let abs_str = abs.to_string_lossy().into_owned();
    match branch {
        Some(b) if !b.is_empty() => {
            git(repo, &["worktree", "add", "-b", b, &abs_str, "HEAD"])?;
        }
        _ => {
            git(repo, &["worktree", "add", &abs_str])?;
        }
    }
    Ok(abs)
}

/// List the repository's worktrees (parsed from `--porcelain` output).
pub fn list(repo: &Path) -> Result<Vec<Worktree>, String> {
    let out = git(repo, &["worktree", "list", "--porcelain"])?;
    Ok(parse_list(&out))
}

/// Remove the worktree at `path` (relative to `repo` when not absolute).
pub fn remove(repo: &Path, path: &str) -> Result<PathBuf, String> {
    let abs = resolve(repo, path);
    git(repo, &["worktree", "remove", &abs.to_string_lossy()])?;
    Ok(abs)
}

/// Parse `git worktree list --porcelain`: blank-line-separated records, each a
/// `worktree <path>` line plus optional `branch refs/heads/<name>`.
fn parse_list(text: &str) -> Vec<Worktree> {
    let mut out = Vec::new();
    let mut path: Option<PathBuf> = None;
    let mut branch: Option<String> = None;
    for line in text.lines() {
        if let Some(p) = line.strip_prefix("worktree ") {
            path = Some(PathBuf::from(p.trim()));
        } else if let Some(b) = line.strip_prefix("branch ") {
            branch = Some(b.trim().strip_prefix("refs/heads/").unwrap_or(b.trim()).to_string());
        } else if line.trim().is_empty() {
            if let Some(p) = path.take() {
                out.push(Worktree { path: p, branch: branch.take() });
            }
            branch = None;
        }
    }
    if let Some(p) = path.take() {
        out.push(Worktree { path: p, branch });
    }
    out
}

#[cfg(test)]
#[path = "../tests/worktree.rs"]
mod tests;
