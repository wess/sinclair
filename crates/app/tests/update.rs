use super::*;

#[test]
fn newer_versions_are_detected() {
    assert!(is_newer("1.21.0", "1.20.0"));
    assert!(is_newer("2.0.0", "1.99.99"));
    assert!(is_newer("1.20.1", "1.20.0"));
    assert!(is_newer("v1.21.0", "1.20.0")); // tolerates leading v
}

#[test]
fn same_or_older_is_not_newer() {
    assert!(!is_newer("1.20.0", "1.20.0"));
    assert!(!is_newer("1.19.0", "1.20.0"));
    assert!(!is_newer("nonsense", "1.20.0"));
    assert!(!is_newer("1.20.0", "garbage"));
}

#[test]
fn release_asset_lookup() {
    let r = Release {
        version: "1.21.0".into(),
        url: "https://x".into(),
        assets: vec![
            ("Prompt.dmg".into(), "https://d/Prompt.dmg".into()),
            ("prompt_1.21.0_arm64.deb".into(), "https://d/deb".into()),
            ("Prompt-1.21.0-aarch64.AppImage".into(), "https://d/img".into()),
        ],
    };
    assert_eq!(r.asset(".dmg"), Some("https://d/Prompt.dmg"));
    assert_eq!(r.asset(".AppImage"), Some("https://d/img"));
    assert_eq!(r.asset(".exe"), None);
}

#[test]
fn managed_command_per_install() {
    assert_eq!(managed_command(&Install::BrewCask).as_deref(), Some("brew upgrade --cask prompt"));
    assert!(managed_command(&Install::LinuxPackage).is_some());
    assert!(managed_command(&Install::MacApp(std::path::PathBuf::from("/x"))).is_none());
}
