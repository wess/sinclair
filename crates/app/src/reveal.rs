//! Resolving a clicked path candidate and showing it in the desktop file
//! manager.
//!
//! The scanner in `vt::path` reports anything path-shaped, so this is where a
//! candidate becomes a decision: resolved against the pane's working directory,
//! then checked against the filesystem. Text that merely looked like a path
//! fails that check and the click does nothing, which is the only reason a
//! pattern that loose is safe to click on.

use std::path::{Path, PathBuf};

/// Resolve a candidate against `cwd` and return it only if it exists.
///
/// `~` expands from `$HOME`; a relative path is taken against the pane's
/// working directory, which is what makes the paths in build output clickable
/// — they are printed relative to wherever the build ran.
pub fn resolve(candidate: &str, cwd: Option<&Path>) -> Option<PathBuf> {
    let expanded = if candidate == "~" {
        home()?
    } else if let Some(rest) = candidate.strip_prefix("~/") {
        home()?.join(rest)
    } else {
        PathBuf::from(candidate)
    };
    let full = if expanded.is_absolute() {
        expanded
    } else {
        cwd?.join(expanded)
    };
    // Resolves `..` and symlinks, and is the existence check in one call.
    // A path that does not resolve is not a path, whatever it looked like.
    std::fs::canonicalize(full).ok()
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// Show `path` in the desktop file manager, selected where the platform can
/// select it.
pub fn reveal(path: &Path) {
    let path = path.to_path_buf();
    // Spawning a file manager can block on a cold launch, and this runs from a
    // click handler on the render thread.
    std::thread::spawn(move || {
        let _ = command(&path).map(|mut c| c.spawn());
    });
}

#[cfg(target_os = "macos")]
fn command(path: &Path) -> Option<std::process::Command> {
    let mut c = std::process::Command::new("open");
    c.arg("-R").arg(path);
    Some(c)
}

/// Linux has no portable "reveal this file": the freedesktop file-manager
/// interface is the closest thing, and `xdg-open` on the parent directory is
/// what is left when no one answers it. A directory opens as itself either way.
#[cfg(target_os = "linux")]
fn command(path: &Path) -> Option<std::process::Command> {
    let uri = format!("file://{}", path.display());
    // `dbus-send` fails when nothing implements the interface, and whether
    // anything does is not knowable up front — so the fallback is chained in
    // the shell rather than chosen here.
    let dir = if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent()?.to_path_buf()
    };
    let mut sh = std::process::Command::new("sh");
    sh.arg("-c").arg(format!(
        "dbus-send --session --print-reply --dest=org.freedesktop.FileManager1 \
         /org/freedesktop/FileManager1 org.freedesktop.FileManager1.ShowItems \
         array:string:{} string: >/dev/null 2>&1 || xdg-open {}",
        shell_quote(&uri),
        shell_quote(&dir.display().to_string()),
    ));
    Some(sh)
}

#[cfg(target_os = "linux")]
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn command(_path: &Path) -> Option<std::process::Command> {
    None
}

#[cfg(test)]
#[path = "../tests/reveal.rs"]
mod tests;
