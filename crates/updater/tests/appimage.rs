use super::*;

/// A scratch dir that cleans up after itself.
fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("updater-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn promote_swaps_and_marks_executable() {
    let dir = scratch("promote");
    let target = dir.join("Sinclair.AppImage");
    let staged = dir.join(".Sinclair.AppImage.update");
    std::fs::write(&target, b"old").unwrap();
    std::fs::write(&staged, b"new").unwrap();

    let relaunch = promote(&staged, &target).unwrap();
    assert_eq!(relaunch, Relaunch::Binary(target.clone()));
    assert_eq!(std::fs::read(&target).unwrap(), b"new");
    assert!(!staged.exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&target).unwrap().permissions().mode();
        assert_eq!(mode & 0o755, 0o755);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn promote_fails_cleanly_without_a_staged_file() {
    let dir = scratch("missing");
    let err = promote(&dir.join("absent"), &dir.join("target")).unwrap_err();
    assert!(err.contains("replace AppImage"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn promote_failure_drops_the_staged_file() {
    let dir = scratch("promotefail");
    let staged = dir.join(".Sinclair.AppImage.update");
    std::fs::write(&staged, b"new").unwrap();

    let err = promote(&staged, &dir.join("nosuchdir/Sinclair.AppImage")).unwrap_err();
    assert!(err.contains("replace AppImage"));
    assert!(!staged.exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn failed_download_leaves_no_staged_file() {
    let dir = scratch("download");
    let target = dir.join("Sinclair.AppImage");
    let staged = dir.join(".Sinclair.AppImage.update");
    // Simulate a dead download's partial output: the fetch is refused (non-https)
    // and any staged bytes must be swept up.
    std::fs::write(&staged, b"partial").unwrap();
    let release = Release {
        version: "9.9.9".to_string(),
        url: String::new(),
        assets: vec![crate::Asset {
            name: format!("Sinclair-9.9.9-{}.AppImage", std::env::consts::ARCH),
            url: "http://127.0.0.1/x".to_string(),
            size: 0,
        }],
    };

    assert!(install(&release, &target, &|_| {}).is_err());
    assert!(!staged.exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_appimage_for_another_architecture_is_not_installed() {
    // A release that has only uploaded the other arch's image must be refused
    // outright, never renamed over the running install.
    let dir = scratch("otherarch");
    let target = dir.join("Sinclair.AppImage");
    std::fs::write(&target, b"working").unwrap();
    let other = if std::env::consts::ARCH == "aarch64" { "x86_64" } else { "aarch64" };
    let release = Release {
        version: "9.9.9".to_string(),
        url: String::new(),
        assets: vec![crate::Asset {
            name: format!("Sinclair-9.9.9-{other}.AppImage"),
            url: "https://d/x".to_string(),
            size: 0,
        }],
    };

    assert!(install(&release, &target, &|_| {}).is_err());
    assert_eq!(std::fs::read(&target).unwrap(), b"working");
    let _ = std::fs::remove_dir_all(&dir);
}
