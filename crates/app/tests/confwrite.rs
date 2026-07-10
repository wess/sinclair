use super::*;

fn tmpdir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("sinclairconfwrite{tag}{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn persist_replaces_atomically_and_leaves_no_temp() {
    let dir = tmpdir("a");
    let path = dir.join("config");
    std::fs::write(&path, "font-size = 12\n").unwrap();
    persist(&path, "font-size = 13\n");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "font-size = 13\n");
    let leftovers: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
        .collect();
    assert!(leftovers.is_empty(), "temp file left behind");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn current_treats_missing_as_empty() {
    let dir = tmpdir("b");
    assert_eq!(current(&dir.join("config")), Some(String::new()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn current_refuses_an_unreadable_existing_file() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tmpdir("c");
    let path = dir.join("config");
    std::fs::write(&path, "shell = /bin/zsh\n").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
    // Root can read anything; the check is meaningless in that case.
    if unsafe { libc::geteuid() } != 0 {
        assert_eq!(current(&path), None, "unreadable config must refuse the write");
    }
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}
