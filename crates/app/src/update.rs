//! Self-update: check GitHub for a newer release (on launch + hourly) and, when
//! one is found, offer to update. Modeled on Zed's `auto_update`.
//!
//! The install method decides how we update: a package-managed install
//! (Homebrew cask, `.deb`) defers to the package manager (we run its upgrade
//! command in a pane), while a manual `.dmg`/AppImage install is swapped in
//! place and relaunched via gpui's [`gpui::App::restart`]. Pure checking and
//! detection live here; the UI (prompt window, menu, About) lives in
//! `updateui.rs`.

use std::path::PathBuf;
use std::process::Command;

/// The GitHub repo releases are published to.
const REPO: &str = "wess/prompt";

/// How often to re-check while running (Zed's stable channel cadence).
pub const POLL: std::time::Duration = std::time::Duration::from_secs(60 * 60);

/// This build's version.
pub fn current() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// A release newer than the running build.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    /// Semver without the leading `v` (e.g. `1.21.0`).
    pub version: String,
    /// The release page URL.
    pub url: String,
    /// `(asset name, download url)` for each uploaded asset.
    pub assets: Vec<(String, String)>,
}

impl Release {
    /// The download URL of the first asset whose name contains `needle`.
    pub fn asset(&self, needle: &str) -> Option<&str> {
        self.assets
            .iter()
            .find(|(n, _)| n.contains(needle))
            .map(|(_, u)| u.as_str())
    }
}

/// Parse `major.minor.patch` (tolerating a leading `v` and extra fields).
fn parse(v: &str) -> Option<(u64, u64, u64)> {
    let v = v.trim().trim_start_matches('v');
    let mut it = v.split(['.', '-', '+']).map(|p| p.parse::<u64>().ok());
    Some((it.next()??, it.next()??, it.next()??))
}

/// Whether `latest` is a strictly newer version than `current`.
pub fn is_newer(latest: &str, current: &str) -> bool {
    match (parse(latest), parse(current)) {
        (Some(a), Some(b)) => a > b,
        _ => false,
    }
}

/// Fetch the latest published release from GitHub and, if it's newer than this
/// build, return it. Blocking (spawns `curl`) — run off the UI thread.
pub fn check() -> Result<Option<Release>, String> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let body = crate::catalog::fetch(&url)?;
    let v: serde_json::Value =
        serde_json::from_slice(&body).map_err(|e| format!("parse release: {e}"))?;
    let tag = v["tag_name"].as_str().ok_or("release has no tag")?;
    let version = tag.trim_start_matches('v').to_string();
    if !is_newer(&version, current()) {
        return Ok(None);
    }
    let assets = v["assets"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| {
                    Some((
                        x["name"].as_str()?.to_string(),
                        x["browser_download_url"].as_str()?.to_string(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default();
    let page = v["html_url"].as_str().unwrap_or("").to_string();
    Ok(Some(Release { version, url: page, assets }))
}

/// How this copy of Prompt was installed, which decides the update path.
/// (Some variants are only constructed on their platform.)
#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Install {
    /// Homebrew cask — defer to `brew upgrade`.
    BrewCask,
    /// A `.deb`/`.rpm`/pacman install — defer to the system package manager.
    LinuxPackage,
    /// A manually-installed macOS `.app` bundle at this path (swap in place).
    MacApp(PathBuf),
    /// A running AppImage at this path (replace the file).
    AppImage(PathBuf),
    /// Unknown / unsupported — fall back to opening the release page.
    Unknown,
}

/// Detect the install method from the running executable and environment.
pub fn detect_install() -> Install {
    // Linux AppImage exports APPIMAGE pointing at the running image.
    if let Some(img) = std::env::var_os("APPIMAGE") {
        return Install::AppImage(PathBuf::from(img));
    }
    let exe = std::env::current_exe().unwrap_or_default();
    #[cfg(target_os = "macos")]
    {
        // .../Prompt.app/Contents/MacOS/prompt -> the .app bundle is 3 up.
        if let Some(app) = exe.ancestors().nth(3).filter(|p| p.extension().is_some_and(|e| e == "app")) {
            if brew_manages_cask() {
                return Install::BrewCask;
            }
            return Install::MacApp(app.to_path_buf());
        }
    }
    #[cfg(target_os = "linux")]
    {
        // Installed under a system prefix => a distro package owns it.
        if exe.starts_with("/usr") || exe.starts_with("/opt") {
            return Install::LinuxPackage;
        }
    }
    let _ = exe;
    Install::Unknown
}

/// Whether Homebrew tracks the `prompt` cask (best-effort; false if brew absent).
#[cfg(target_os = "macos")]
fn brew_manages_cask() -> bool {
    Command::new("brew")
        .args(["list", "--cask", "prompt"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// The shell command that updates a package-managed install, or `None` when the
/// install is swapped in place instead.
pub fn managed_command(install: &Install) -> Option<String> {
    match install {
        Install::BrewCask => Some("brew upgrade --cask prompt".to_string()),
        Install::LinuxPackage => {
            Some("sudo apt-get update && sudo apt-get install --only-upgrade prompt".to_string())
        }
        _ => None,
    }
}

/// Download `url` to `dest` over https (streamed, so large assets are fine).
fn download_to(url: &str, dest: &std::path::Path) -> Result<(), String> {
    if !url.starts_with("https://") {
        return Err("refusing non-https download".to_string());
    }
    let out = Command::new("curl")
        .args(["-sL", "--fail", "--proto", "=https", "--proto-redir", "=https", "-o"])
        .arg(dest)
        .args(["-H", "User-Agent: prompt-terminal", "--", url])
        .output()
        .map_err(|e| format!("curl: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!("download failed ({})", out.status))
    }
}

/// Download the release and swap it into place, returning the binary to restart
/// into. Only for in-place installs ([`Install::MacApp`], [`Install::AppImage`]);
/// managed installs go through [`managed_command`] instead. Blocking.
pub fn stage(release: &Release, install: &Install) -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join(format!("prompt-update-{}", release.version));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    match install {
        Install::MacApp(app) => stage_mac_app(release, app, &dir),
        Install::AppImage(path) => stage_appimage(release, path, &dir),
        _ => Err("this install can't be updated in place".to_string()),
    }
}

/// macOS: download the notarized `.dmg`, mount it, copy the new `.app` over the
/// installed one, unmount, and return the inner binary to relaunch.
#[cfg(target_os = "macos")]
fn stage_mac_app(release: &Release, app: &std::path::Path, dir: &std::path::Path) -> Result<PathBuf, String> {
    let url = release.asset(".dmg").ok_or("release has no .dmg asset")?;
    let dmg = dir.join("Prompt.dmg");
    download_to(url, &dmg)?;
    let mount = dir.join("mnt");
    std::fs::create_dir_all(&mount).map_err(|e| e.to_string())?;
    let attach = Command::new("hdiutil")
        .args(["attach", "-nobrowse", "-quiet", "-mountpoint"])
        .arg(&mount)
        .arg(&dmg)
        .status()
        .map_err(|e| format!("hdiutil attach: {e}"))?;
    if !attach.success() {
        return Err("could not mount the update image".to_string());
    }
    let src = std::fs::read_dir(&mount)
        .ok()
        .and_then(|mut d| d.find_map(|e| e.ok().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "app"))))
        .ok_or("no .app in the update image")?;
    // Replace atomically-ish: ditto into a sibling, then swap.
    let staged = dir.join("Prompt.app");
    let _ = std::fs::remove_dir_all(&staged);
    let copy = Command::new("ditto").arg(&src).arg(&staged).status();
    let _ = Command::new("hdiutil").args(["detach", "-quiet"]).arg(&mount).status();
    copy.map_err(|e| format!("ditto: {e}"))?
        .success()
        .then_some(())
        .ok_or("could not copy the update")?;
    // Swap the installed bundle for the staged one.
    let backup = app.with_extension("app.old");
    let _ = std::fs::remove_dir_all(&backup);
    std::fs::rename(app, &backup).map_err(|e| format!("move old app: {e}"))?;
    if let Err(e) = std::fs::rename(&staged, app) {
        let _ = std::fs::rename(&backup, app); // roll back
        return Err(format!("install update: {e}"));
    }
    let _ = std::fs::remove_dir_all(&backup);
    Ok(app.join("Contents/MacOS/prompt"))
}

#[cfg(not(target_os = "macos"))]
fn stage_mac_app(_: &Release, _: &std::path::Path, _: &std::path::Path) -> Result<PathBuf, String> {
    Err("not macOS".to_string())
}

/// Linux AppImage: download the new image over the running one and mark it
/// executable, returning it to relaunch.
fn stage_appimage(release: &Release, path: &std::path::Path, dir: &std::path::Path) -> Result<PathBuf, String> {
    let url = release.asset(".AppImage").ok_or("release has no AppImage asset")?;
    let tmp = dir.join("Prompt.AppImage");
    download_to(url, &tmp)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755));
    }
    std::fs::rename(&tmp, path).map_err(|e| format!("replace AppImage: {e}"))?;
    Ok(path.to_path_buf())
}

#[cfg(test)]
#[path = "../tests/update.rs"]
mod tests;
