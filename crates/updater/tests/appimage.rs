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
    let target = dir.join("Prompt.AppImage");
    let staged = dir.join(".Prompt.AppImage.update");
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
