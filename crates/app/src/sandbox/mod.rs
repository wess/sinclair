//! The shared project sandbox: one container that the human and every agent on
//! a team work inside together.
//!
//! Sinclair creates and owns it using nothing but `docker` or `podman` — no
//! VS Code, no `devcontainer` CLI, no registry to pull a Sinclair image from.
//! When a project happens to have a `.devcontainer/devcontainer.json`, or an
//! editor already has a container up for it, those are used; when it does not,
//! nothing degrades.
//!
//! - [`spec`] resolves settings into a container description (pure).
//! - [`ensure`] brings that description up (blocking engine calls).
//!
//! The host layer that counts attached panes and decides when to retire the
//! container lives in `root::containers`.

pub mod ensure;
pub mod spec;

pub use ensure::{ensure, Ready, Stage};
pub use spec::{build, Env, Spec};

/// Read the project's `devcontainer.json`, if it has one Sinclair can parse.
///
/// A malformed file is not an error worth blocking on — the sandbox works
/// without it — so it comes back as `None` plus a note for the user.
pub fn read_devcontainer(project: &str) -> (Option<container::DevContainer>, Option<String>) {
    for path in container::devcontainer::config_paths(project) {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        return match container::devcontainer::parse(&text, project) {
            Ok(dc) => (Some(dc), None),
            Err(e) => (None, Some(format!("{path}: {e}"))),
        };
    }
    (None, None)
}

/// The git repository `dir` belongs to, walking up until a `.git` is found.
///
/// A sandbox serves a *project*, so it has to resolve to the same directory
/// from any pane inside it. Keying on the pane's own cwd instead would give a
/// pane three levels down its own container, with only that subtree mounted.
pub fn repo_root(dir: &std::path::Path) -> Option<std::path::PathBuf> {
    let mut cur = Some(dir);
    while let Some(d) = cur {
        if d.join(".git").exists() {
            return Some(d.to_path_buf());
        }
        cur = d.parent();
    }
    None
}

/// Paths too broad to identity-mount into a sandbox.
///
/// The sandbox mounts what it is given. Mounting `$HOME` or `/` would hand an
/// agent running with its permission prompts bypassed the whole machine, which
/// is the exact thing the sandbox exists to prevent — so a pane that resolves
/// to one of these gets no sandbox and an explanation instead.
pub fn too_broad_to_mount(path: &str) -> bool {
    let path = path.trim_end_matches('/');
    if path.is_empty() || path == "/" {
        return true;
    }
    // Anything at depth 1 (`/Users`, `/home`, `/opt`) is a shared parent, not a
    // project. A home directory is depth 2 on macOS and Linux alike.
    if path.matches('/').count() < 2 {
        return true;
    }
    home_dir().is_some_and(|home| path == home.to_string_lossy().trim_end_matches('/'))
}

fn home_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME").map(std::path::PathBuf::from)
}

/// `uid:gid` owning `project`, for `sandbox-user: host`.
pub fn owner_of(project: &str) -> Option<String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let meta = std::fs::metadata(project).ok()?;
        Some(format!("{}:{}", meta.uid(), meta.gid()))
    }
    #[cfg(not(unix))]
    {
        let _ = project;
        None
    }
}
