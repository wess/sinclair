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
    std::fs::write(&manifest, "id = \"tools\"\n[[command]]\nid = \"top\"\nrun = \"top\"\n").unwrap();
    let (plugins, diags) = loadmanifests(vec![dir.clone(), manifest]);
    assert!(diags.is_empty(), "{diags:?}");
    assert_eq!(plugins.len(), 1);
    assert_eq!(plugins[0].id, "tools");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn first_manifest_for_an_id_wins() {
    // A user copy (listed first) overrides a bundled copy of the same id, so the
    // plugin loads once with the overriding manifest.
    let user = tempdir("dedupuser");
    let bundled = tempdir("dedupbundled");
    std::fs::write(
        user.join(MANIFEST),
        "id = \"notes\"\nname = \"User Notes\"\n",
    )
    .unwrap();
    std::fs::write(
        bundled.join(MANIFEST),
        "id = \"notes\"\nname = \"Bundled Notes\"\n",
    )
    .unwrap();
    let (plugins, diags) = loadmanifests(vec![user.clone(), bundled.clone()]);
    assert!(diags.is_empty(), "{diags:?}");
    assert_eq!(plugins.len(), 1);
    assert_eq!(plugins[0].name, "User Notes");
    let _ = std::fs::remove_dir_all(user);
    let _ = std::fs::remove_dir_all(bundled);
}

#[test]
fn unreadable_manifest_reports_diagnostic() {
    let (plugins, diags) =
        loadmanifests(vec![PathBuf::from("/definitely/missing/plugin.toml")]);
    assert!(plugins.is_empty());
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("failed to read"));
}
