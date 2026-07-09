//! macOS: mount the release `.dmg` and rsync the new bundle's contents onto
//! the installed `.app` — never replace the bundle directory itself.
//!
//! This is Zed's install strategy, and the "in place" is the point: the
//! installed bundle keeps its path *and* directory inode, so LaunchServices'
//! registration stays valid and the running executable is only ever
//! replaced-by-rename (its open inode lives on, which macOS is fine with — it
//! is in-place *modification* of a running binary that gets a process killed).
//! Swapping the whole bundle out from under LaunchServices is what used to
//! make the relaunch fall back to running the bare Mach-O in Terminal.app.

#[cfg(target_os = "macos")]
pub(crate) fn install(
    release: &crate::Release,
    app: &std::path::Path,
) -> Result<crate::Relaunch, String> {
    use std::process::Command;

    /// Detach the mount on every exit path, success or error.
    struct Unmount(std::path::PathBuf);
    impl Drop for Unmount {
        fn drop(&mut self) {
            let _ = Command::new("hdiutil").args(["detach", "-quiet"]).arg(&self.0).status();
        }
    }

    let url = release.asset(".dmg").ok_or("release has no .dmg asset")?;
    let dir = std::env::temp_dir().join(format!("prompt-update-{}", release.version));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let dmg = dir.join("Prompt.dmg");
    crate::fetch::file(url, &dmg)?;

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
    let unmount = Unmount(mount.clone());

    // rsync the mounted bundle's *contents* (trailing slash) onto the
    // installed bundle; --delete drops files the new version no longer ships.
    // `Icon?` is the dmg's custom-icon file (`Icon\r`), not part of the app.
    let src = app_in(&mount)?;
    let mut contents = std::ffi::OsString::from(src);
    contents.push("/");
    let synced = Command::new("rsync")
        .args(["-a", "--delete", "--exclude", "Icon?"])
        .arg(&contents)
        .arg(app)
        .status()
        .map_err(|e| format!("rsync: {e}"))?;
    if !synced.success() {
        return Err("could not copy the update into place".to_string());
    }

    drop(unmount);
    let _ = std::fs::remove_dir_all(&dir);
    Ok(crate::Relaunch::Current)
}

/// The first `.app` bundle inside `dir` (the mounted update image).
#[cfg(target_os = "macos")]
fn app_in(dir: &std::path::Path) -> Result<std::path::PathBuf, String> {
    std::fs::read_dir(dir)
        .ok()
        .and_then(|mut d| {
            d.find_map(|e| {
                e.ok()
                    .map(|e| e.path())
                    .filter(|p| p.extension().is_some_and(|x| x == "app"))
            })
        })
        .ok_or_else(|| "no .app in the update image".to_string())
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn install(
    _release: &crate::Release,
    _app: &std::path::Path,
) -> Result<crate::Relaunch, String> {
    Err("not macOS".to_string())
}

#[cfg(all(test, target_os = "macos"))]
#[path = "../tests/mac.rs"]
mod tests;
