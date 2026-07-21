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

/// The code requirement an update must satisfy before we install it: signed by
/// Apple's Developer ID chain, with our team as the leaf. Team IDs are stable
/// across certificate renewals, so this doesn't need touching when the signing
/// cert rolls — only if the account itself changes.
///
/// The practical consequence: an ad-hoc signed build (what CI produces when the
/// signing secrets are absent) cannot self-update, and shouldn't be able to.
///
/// The leading `=` is load-bearing and is part of codesign's argument grammar:
/// `-R` treats a bare value as a *path to* a requirement file, so dropping it
/// makes every verification fail with "No such file or directory" — which, on
/// a check that gates installation, means refusing every update there is.
#[cfg(target_os = "macos")]
const TEAM_REQUIREMENT: &str =
    "-R=anchor apple generic and certificate leaf[subject.OU] = XJDC46F35X";

#[cfg(target_os = "macos")]
pub(crate) fn install(
    release: &crate::Release,
    app: &std::path::Path,
    on_stage: &dyn Fn(crate::Stage),
) -> Result<crate::Relaunch, String> {
    use crate::Stage;
    use std::process::Command;

    /// Detach the mount on every exit path, success or error.
    struct Unmount(std::path::PathBuf);
    impl Drop for Unmount {
        fn drop(&mut self) {
            let _ = Command::new("hdiutil").args(["detach", "-quiet"]).arg(&self.0).status();
        }
    }

    let asset = release
        .asset_for(&crate::Install::MacApp(app.to_path_buf()))
        .ok_or("this release hasn't published a macOS build yet")?;
    let dir = std::env::temp_dir().join(format!("sinclair-update-{}", release.version));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let dmg = dir.join("Sinclair.dmg");
    let total = asset.size;
    on_stage(Stage::Downloading { done: 0, total });
    crate::fetch::file(&asset.url, &dmg, total, &|done| {
        on_stage(Stage::Downloading { done, total })
    })?;

    on_stage(Stage::Preparing);
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

    // Verify the payload on the *mounted image*, before a byte of it reaches
    // the installed bundle. Verifying only afterwards (as this used to) means a
    // bad update is already committed by the time we find out — there is no
    // rollback, and the next launch runs it. The requirement pins the signing
    // identity: a bare `--verify` only proves the bundle matches its own seal,
    // so any validly-signed app at all would pass, which is not the question we
    // are asking of something we are about to execute as the user.
    on_stage(Stage::Verifying);
    let src = app_in(&mount)?;
    let trusted = Command::new("codesign")
        .args(["--verify", "--deep", TEAM_REQUIREMENT])
        .arg(&src)
        .status()
        .map_err(|e| format!("codesign: {e}"))?;
    if !trusted.success() {
        return Err("the update isn't signed by Sinclair — refusing to install it".to_string());
    }

    // rsync the mounted bundle's *contents* (trailing slash) onto the
    // installed bundle; --delete drops files the new version no longer ships.
    // --delay-updates stages every changed file inside the bundle (per-dir
    // `.~tmp~` folders) and promotes it by rename only at the end, so a sync
    // that dies partway leaves the old files intact instead of a mixed bundle
    // with a broken signature. `Icon?` is the dmg's custom-icon file
    // (`Icon\r`), not part of the app.
    on_stage(Stage::Installing);
    let mut contents = std::ffi::OsString::from(src);
    contents.push("/");
    let synced = Command::new("rsync")
        .args(["-a", "--delete", "--delay-updates", "--exclude", "Icon?"])
        .arg(&contents)
        .arg(app)
        .status()
        .map_err(|e| format!("rsync: {e}"))?;
    if !synced.success() {
        scrub_staging(app);
        return Err("could not copy the update into place".to_string());
    }

    // Re-verify what actually landed. The pre-flight check above established
    // the payload was genuine; this catches the sync itself having mangled it,
    // which Gatekeeper would otherwise turn into a dead next launch.
    let verified = Command::new("codesign")
        .args(["--verify", "--deep", TEAM_REQUIREMENT])
        .arg(app)
        .status()
        .map_err(|e| format!("codesign: {e}"))?;
    if !verified.success() {
        return Err("the updated app failed signature verification".to_string());
    }

    drop(unmount);
    let _ = std::fs::remove_dir_all(&dir);
    Ok(crate::Relaunch::Current)
}

/// Remove the `.~tmp~` staging folders `rsync --delay-updates` leaves under
/// `dir` when a sync fails partway.
#[cfg(target_os = "macos")]
fn scrub_staging(dir: &std::path::Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if path.file_name().is_some_and(|n| n == ".~tmp~") {
            let _ = std::fs::remove_dir_all(&path);
        } else {
            scrub_staging(&path);
        }
    }
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
    _on_stage: &dyn Fn(crate::Stage),
) -> Result<crate::Relaunch, String> {
    Err("not macOS".to_string())
}

#[cfg(all(test, target_os = "macos"))]
#[path = "../tests/mac.rs"]
mod tests;
