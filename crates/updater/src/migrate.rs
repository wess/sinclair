//! One-time bundle rename for installs that predate the Sinclair name.
//!
//! An installed `Prompt.app` updates to Sinclair on its own: [`crate::mac`]
//! rsyncs the new bundle's *contents* onto the installed one, and the shipped
//! 1.25 installer already matches the release `.dmg` by extension and takes
//! whatever `.app` it finds inside. What it cannot do is rename the bundle
//! directory — that is precisely the swap the in-place design forbids, because
//! handing LaunchServices a new inode for a registered path makes the relaunch
//! fall back to running the bare Mach-O inside Terminal.app.
//!
//! So the rename happens at startup instead, before gpui or the single-instance
//! socket exist: rename the directory, then `open` the *new* path. A path that
//! LaunchServices has never seen has no stale registration to resolve, so it
//! registers the bundle fresh. The old process exits without ever having bound
//! the socket.
//!
//! Renaming a directory that holds the running executable is safe on macOS —
//! the open inode outlives the path.

#[cfg(any(target_os = "macos", test))]
use std::path::{Path, PathBuf};

/// A pending rename of the installed bundle directory.
#[cfg(any(target_os = "macos", test))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    pub from: PathBuf,
    pub to: PathBuf,
}

/// Homebrew's two prefixes. A cask-managed bundle must not be renamed: the
/// cask's receipt still names the old bundle, and moving it strands `brew
/// upgrade` and `brew uninstall` with a path that no longer exists.
#[cfg(any(target_os = "macos", test))]
const BREW_PREFIXES: [&str; 2] = ["/opt/homebrew", "/usr/local"];

/// Whether the bundle holding `exe` should be renamed to `<app>.app`, given a
/// way to ask whether a path exists. Pure, so the policy is testable without
/// touching the filesystem.
///
/// Returns `None` — meaning "boot normally" — when the executable is not the
/// shipped one, is not inside a bundle, the bundle is already named correctly,
/// the destination is taken, or Homebrew owns the install.
#[cfg(any(target_os = "macos", test))]
pub fn plan_with(exe: &Path, app: &str, exists: impl Fn(&Path) -> bool) -> Option<Plan> {
    // Only the shipped binary migrates; a dev build (`sinclairdev`) never does,
    // even if someone drops it inside a bundle.
    if exe.file_stem()? != app.to_lowercase().as_str() {
        return None;
    }
    let from = crate::install::bundle_of(exe)?;
    let old = from.file_stem()?.to_str()?.to_string();
    if old == app {
        return None;
    }
    if brew_owns(&from, &old, &exists) {
        return None;
    }
    let to = from.with_file_name(format!("{app}.app"));
    if exists(&to) {
        return None;
    }
    Some(Plan { from, to })
}

/// Whether Homebrew placed this bundle, in which case renaming it would strand
/// the cask receipt.
///
/// A receipt alone is not enough: a machine can hold a cask-installed copy *and*
/// a hand-downloaded one, and the receipt says nothing about which bundle is
/// which. Homebrew only ever moves a cask app into an app directory, so a bundle
/// living anywhere else is not the cask's, whatever receipts exist.
#[cfg(any(target_os = "macos", test))]
fn brew_owns(from: &Path, old: &str, exists: &impl Fn(&Path) -> bool) -> bool {
    let Some(parent) = from.parent() else {
        return false;
    };
    let user_apps = std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Applications"));
    let in_appdir = parent == Path::new("/Applications")
        || user_apps.as_deref().is_some_and(|p| parent == p);
    if !in_appdir {
        return false;
    }
    let token = old.to_lowercase();
    BREW_PREFIXES
        .iter()
        .any(|p| exists(&Path::new(p).join("Caskroom").join(&token)))
}

/// Rename the running bundle to `Sinclair.app` and relaunch there.
///
/// `true` means a fresh instance was launched and this process should exit
/// immediately, before it opens a window or binds the single-instance socket.
/// Every failure is silent and yields `false`: booting from a directory with
/// the old name works fine, so a migration that cannot happen is never fatal.
#[cfg(target_os = "macos")]
pub fn migrate_bundle() -> bool {
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let Some(plan) = plan_with(&exe, "Sinclair", |p| p.exists()) else {
        return false;
    };
    if std::fs::rename(&plan.from, &plan.to).is_err() {
        // /Applications is admin-writable; a locked-down machine just keeps the
        // old directory name. The app is still Sinclair inside.
        return false;
    }
    std::process::Command::new("open")
        .arg(&plan.to)
        .spawn()
        .is_ok()
}

#[cfg(not(target_os = "macos"))]
pub fn migrate_bundle() -> bool {
    false
}

#[cfg(test)]
#[path = "../tests/migrate.rs"]
mod tests;
