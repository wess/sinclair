use super::*;

/// A scratch dir that cleans up after itself.
fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("updater-mac-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn finds_the_app_in_a_mounted_image() {
    let dir = scratch("appin");
    std::fs::create_dir_all(dir.join("Sinclair.app/Contents")).unwrap();
    std::fs::write(dir.join(".background"), b"").unwrap();
    assert_eq!(app_in(&dir).unwrap(), dir.join("Sinclair.app"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn empty_image_is_an_error() {
    let dir = scratch("empty");
    assert!(app_in(&dir).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}
