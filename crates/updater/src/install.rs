//! Install detection and the install/relaunch entry points.

use std::path::PathBuf;
#[cfg(any(target_os = "macos", test))]
use std::path::Path;

use crate::Release;

/// How this copy of Sinclair was installed, which decides the update path. We
/// self-update where we can rewrite the install ourselves; anything else opens
/// the download page. (Some variants are only constructed on their platform.)
#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Install {
    /// A macOS `.app` bundle at this path — rewrite its contents in place.
    /// Covers every macOS install, Homebrew casks included; how it got there
    /// doesn't matter.
    MacApp(PathBuf),
    /// A running AppImage at this path (replace the file).
    AppImage(PathBuf),
    /// An install we can't rewrite ourselves — a root-owned distro package
    /// (`.deb`/`.rpm`), a Windows install, or a dev build. Fall back to opening
    /// the release page.
    Unknown,
}

impl Install {
    /// Whether this install can be updated in place (vs. opening the page).
    pub fn is_in_place(&self) -> bool {
        matches!(self, Install::MacApp(_) | Install::AppImage(_))
    }
}

/// What the installer is doing, reported as it happens so the UI can show real
/// progress. Without this the whole install is one opaque blocking call, and a
/// failure that arrives in microseconds — a missing asset, say — never renders
/// a single frame of feedback.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Stage {
    /// Fetching the asset: bytes written of the total (0 total = unknown).
    Downloading { done: u64, total: u64 },
    /// Opening what we downloaded (macOS mounts the `.dmg`).
    Preparing,
    /// Writing the new version over the install.
    Installing,
    /// Checking the result before we relaunch into it.
    Verifying,
}

impl Stage {
    /// Short present-tense label for the UI. Lives here so the stages and the
    /// words describing them can't drift apart.
    pub fn label(&self) -> &'static str {
        match self {
            Stage::Downloading { .. } => "Downloading update…",
            Stage::Preparing => "Preparing…",
            Stage::Installing => "Installing…",
            Stage::Verifying => "Verifying…",
        }
    }
}

/// How to relaunch after a successful [`install`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Relaunch {
    /// The install was rewritten in place at its existing path: restart with
    /// **no** explicit binary path, so gpui reopens the running bundle via
    /// `NSBundle`. Never hand the restart an explicit path here — `open` on a
    /// path whose LaunchServices registration is stale can fall back to
    /// running the inner Mach-O inside Terminal.app.
    Current,
    /// Restart by launching this binary (`gpui::App::set_restart_path`).
    Binary(PathBuf),
}

/// The `.app` bundle three levels above a macOS executable
/// (`…/Sinclair.app/Contents/MacOS/sinclair`), if there is one.
#[cfg(any(target_os = "macos", test))]
pub(crate) fn bundle_of(exe: &Path) -> Option<PathBuf> {
    exe.ancestors()
        .nth(3)
        .filter(|p| p.extension().is_some_and(|e| e == "app"))
        .map(|p| p.to_path_buf())
}

/// Detect the install method from the running executable and environment. This
/// only decides *how* to install an update — the check for whether one exists
/// is [`crate::check`], which asks GitHub. No package manager is consulted.
pub fn detect() -> Install {
    // Linux AppImage exports APPIMAGE pointing at the running image.
    if let Some(img) = std::env::var_os("APPIMAGE") {
        return Install::AppImage(PathBuf::from(img));
    }
    #[cfg(target_os = "macos")]
    {
        // Any macOS .app self-updates; we never ask Homebrew whether it owns it.
        let exe = std::env::current_exe().unwrap_or_default();
        if let Some(app) = bundle_of(&exe) {
            return Install::MacApp(app);
        }
    }
    // A Linux distro package under a system prefix is root-owned, and Windows
    // installs update through their package flow; neither can be swapped in
    // place, so both fall through to Unknown (open the download page).
    Install::Unknown
}

/// Download the release and install it in place, returning how to relaunch.
/// Only for in-place installs ([`Install::MacApp`], [`Install::AppImage`]);
/// an [`Install::Unknown`] has no in-place path and opens the release page
/// instead (see the app's `updateui`). Blocking — run off the UI thread.
///
/// `on_stage` is called from this thread as the install moves through
/// [`Stage`]s, including once per download sample.
pub fn install(
    release: &Release,
    install: &Install,
    on_stage: &dyn Fn(Stage),
) -> Result<Relaunch, String> {
    match install {
        Install::MacApp(app) => crate::mac::install(release, app, on_stage),
        Install::AppImage(path) => crate::appimage::install(release, path, on_stage),
        Install::Unknown => Err("this install can't be updated in place".to_string()),
    }
}

#[cfg(test)]
#[path = "../tests/install.rs"]
mod tests;
