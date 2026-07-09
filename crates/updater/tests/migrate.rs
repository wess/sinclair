use super::*;

/// Nothing on disk exists unless a test says so.
fn nothing(_: &Path) -> bool {
    false
}

fn exe(bundle: &str) -> PathBuf {
    PathBuf::from(format!("/Applications/{bundle}.app/Contents/MacOS/sinclair"))
}

#[test]
fn renames_a_pre_rename_bundle() {
    let plan = plan_with(&exe("Prompt"), "Sinclair", nothing).expect("a rename is planned");
    assert_eq!(plan.from, PathBuf::from("/Applications/Prompt.app"));
    assert_eq!(plan.to, PathBuf::from("/Applications/Sinclair.app"));
}

#[test]
fn a_correctly_named_bundle_is_a_noop() {
    assert_eq!(plan_with(&exe("Sinclair"), "Sinclair", nothing), None);
}

#[test]
fn never_clobbers_an_existing_sinclair_app() {
    // A user who installed Sinclair fresh keeps both; we must not overwrite it.
    let taken = |p: &Path| p == Path::new("/Applications/Sinclair.app");
    assert_eq!(plan_with(&exe("Prompt"), "Sinclair", taken), None);
}

#[test]
fn homebrew_managed_installs_are_left_alone() {
    // Renaming would strand the cask receipt, which still names Prompt.app.
    for prefix in BREW_PREFIXES {
        let cask = PathBuf::from(prefix).join("Caskroom").join("prompt");
        let owned = |p: &Path| p == cask;
        assert_eq!(
            plan_with(&exe("Prompt"), "Sinclair", owned),
            None,
            "cask under {prefix} must suppress the rename"
        );
    }
}

#[test]
fn a_cask_receipt_does_not_suppress_a_bundle_homebrew_never_placed() {
    // A machine can hold both a cask copy and a hand-downloaded one. The
    // receipt describes the former; a bundle outside an app directory is the
    // latter and must still migrate.
    let cask = PathBuf::from("/opt/homebrew/Caskroom/prompt");
    let owned = |p: &Path| p == cask;
    let loose = PathBuf::from("/Users/me/Downloads/Prompt.app/Contents/MacOS/sinclair");
    let plan = plan_with(&loose, "Sinclair", owned).expect("a downloaded copy still migrates");
    assert_eq!(plan.to, PathBuf::from("/Users/me/Downloads/Sinclair.app"));
}

#[test]
fn a_dev_build_never_migrates() {
    // `sinclairdev` inside a bundle is not the shipped binary.
    let dev = PathBuf::from("/Applications/Prompt.app/Contents/MacOS/sinclairdev");
    assert_eq!(plan_with(&dev, "Sinclair", nothing), None);
}

#[test]
fn an_unbundled_executable_never_migrates() {
    let loose = PathBuf::from("/usr/local/bin/sinclair");
    assert_eq!(plan_with(&loose, "Sinclair", nothing), None);
}
