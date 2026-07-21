//! Linux AppImage: download the new image and rename it over the running one.
//! The running process keeps its open inode; the next launch gets the new file.

use std::path::Path;

use crate::{fetch, Install, Relaunch, Release, Stage};

/// Download the release's AppImage and swap it in, returning the relaunch
/// target (the image path, handed to `gpui::App::set_restart_path`).
pub(crate) fn install(
    release: &Release,
    target: &Path,
    on_stage: &dyn Fn(Stage),
) -> Result<Relaunch, String> {
    // Resolved through `asset_for`, which matches the running architecture:
    // picking the AppImage by extension alone would hand an x86_64 machine the
    // aarch64 image and rename it over a working install.
    let asset = release
        .asset_for(&Install::AppImage(target.to_path_buf()))
        .ok_or("this release hasn't published an AppImage for this architecture yet")?;
    // Stage *next to* the target, not in the temp dir: the final rename must
    // not cross filesystems (`/tmp` is often tmpfs), or it fails with EXDEV.
    let name = target.file_name().and_then(|n| n.to_str()).unwrap_or("Sinclair.AppImage");
    let staged = target.with_file_name(format!(".{name}.update"));
    let total = asset.size;
    on_stage(Stage::Downloading { done: 0, total });
    let fetched = fetch::file(&asset.url, &staged, total, &|done| {
        on_stage(Stage::Downloading { done, total })
    });
    if let Err(e) = fetched {
        // A dead download must not strand a partial image next to the app.
        let _ = std::fs::remove_file(&staged);
        return Err(e);
    }
    on_stage(Stage::Installing);
    promote(&staged, target)
}

/// Mark `staged` executable and rename it over `target`, dropping the staged
/// file if the rename fails.
fn promote(staged: &Path, target: &Path) -> Result<Relaunch, String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(staged, std::fs::Permissions::from_mode(0o755));
    }
    if let Err(e) = std::fs::rename(staged, target) {
        let _ = std::fs::remove_file(staged);
        return Err(format!("replace AppImage: {e}"));
    }
    Ok(Relaunch::Binary(target.to_path_buf()))
}

#[cfg(test)]
#[path = "../tests/appimage.rs"]
mod tests;
