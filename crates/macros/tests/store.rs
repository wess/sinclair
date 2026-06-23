use super::*;

fn tempdir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("promptmacrotest{}{}", std::process::id(), name));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn save_then_load_roundtrips() {
    let dir = tempdir("roundtrip");
    let mac = Macro::new("deploy", vec!["git pull".into(), "bun run build".into()]);
    save(&dir, &mac).unwrap();
    let loaded = load(&dir);
    assert_eq!(loaded, vec![mac]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn load_sorts_and_skips_comments_and_blanks() {
    let dir = tempdir("parse");
    std::fs::write(dir.join("b.macro"), "# note\n\nls\n").unwrap();
    std::fs::write(dir.join("a.macro"), "echo a\n").unwrap();
    std::fs::write(dir.join("ignore.txt"), "echo nope\n").unwrap();
    let loaded = load(&dir);
    assert_eq!(loaded.len(), 2);
    assert_eq!(loaded[0].name, "a");
    assert_eq!(loaded[1], Macro::new("b", vec!["ls".into()]));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn rename_and_delete() {
    let dir = tempdir("rename");
    save(&dir, &Macro::new("old", vec!["echo hi".into()])).unwrap();
    rename(&dir, "old", "new").unwrap();
    assert_eq!(load(&dir)[0].name, "new");
    delete(&dir, "new").unwrap();
    assert!(load(&dir).is_empty());
    // Deleting a missing macro is a no-op.
    delete(&dir, "gone").unwrap();
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn save_rejects_bad_name() {
    let dir = tempdir("badname");
    assert!(save(&dir, &Macro::new("Bad Name", vec!["x".into()])).is_err());
    let _ = std::fs::remove_dir_all(dir);
}
