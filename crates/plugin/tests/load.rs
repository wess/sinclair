use super::*;

fn tempdir(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("promptplugintest{}{}", std::process::id(), name));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn explicit_path_can_be_directory_or_manifest() {
    let dir = tempdir("explicit");
    let manifest = dir.join(MANIFEST);
    std::fs::write(&manifest, "id = tools\n[[command]]\nid = top\nrun = top\n").unwrap();
    let (plugins, diags) = loadmanifests(vec![dir.clone(), manifest]);
    assert!(diags.is_empty(), "{diags:?}");
    assert_eq!(plugins.len(), 1);
    assert_eq!(plugins[0].id, "tools");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn unreadable_manifest_reports_diagnostic() {
    let (plugins, diags) =
        loadmanifests(vec![PathBuf::from("/definitely/missing/plugin.toml")]);
    assert!(plugins.is_empty());
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("failed to read"));
}
