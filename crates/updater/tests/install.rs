use super::*;

#[test]
fn only_swappable_installs_update_in_place() {
    // macOS .app and Linux AppImage are rewritten in place; everything else
    // (a root-owned distro package, Windows, a dev build) opens the page.
    assert!(Install::MacApp(PathBuf::from("/Applications/Prompt.app")).is_in_place());
    assert!(Install::AppImage(PathBuf::from("/x/Prompt.AppImage")).is_in_place());
    assert!(!Install::Unknown.is_in_place());
}

#[test]
fn bundle_is_three_levels_above_the_executable() {
    let app = bundle_of(Path::new("/Applications/Prompt.app/Contents/MacOS/prompt"));
    assert_eq!(app, Some(PathBuf::from("/Applications/Prompt.app")));
}

#[test]
fn unbundled_executables_have_no_bundle() {
    // A dev build under target/ must not be mistaken for an installable .app.
    assert_eq!(bundle_of(Path::new("/dev/prompt/target/release/promptdev")), None);
    assert_eq!(bundle_of(Path::new("/usr/local/bin/prompt")), None);
    assert_eq!(bundle_of(Path::new("prompt")), None);
}

#[test]
fn unknown_installs_refuse_in_place_update() {
    let release = Release { version: "9.9.9".into(), url: String::new(), assets: Vec::new() };
    assert!(install(&release, &Install::Unknown).is_err());
}
