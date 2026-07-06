use super::*;

#[test]
fn untracked_defaults_to_enabled_with_no_grants() {
    let installed = Installed::default();
    assert!(installed.is_enabled("git"));
    assert!(installed.granted("git").is_empty());
}

#[test]
fn disable_then_reenable() {
    let mut installed = Installed::default();
    installed.set_enabled("git", false);
    assert!(!installed.is_enabled("git"));
    installed.set_enabled("git", true);
    assert!(installed.is_enabled("git"));
}

#[test]
fn record_captures_grants_and_round_trips() {
    let mut installed = Installed::default();
    installed.record("git", "0.2.0", "catalog:git", vec!["commands".into(), "filesystem".into()]);
    assert_eq!(installed.granted("git"), &["commands".to_string(), "filesystem".to_string()]);

    let text = toml::to_string(&installed).unwrap();
    let back: Installed = toml::from_str(&text).unwrap();
    assert_eq!(back.plugins["git"].version, "0.2.0");
    assert_eq!(back.plugins["git"].source, "catalog:git");
    assert!(back.is_enabled("git"));
    assert_eq!(back.granted("git").len(), 2);
}

#[test]
fn effective_capabilities_enforce_consent() {
    let declared = vec!["commands".to_string(), "network".to_string(), "filesystem".to_string()];
    let mut installed = Installed::default();

    // Untracked (built-in / local): declared set is granted implicitly.
    assert_eq!(installed.effective_capabilities("git", &declared), declared);

    // Tracked with a narrower grant: the plugin can't reach `network`.
    installed.record("git", "0.2.0", "catalog:git", vec!["commands".into(), "filesystem".into()]);
    assert_eq!(
        installed.effective_capabilities("git", &declared),
        vec!["commands".to_string(), "filesystem".to_string()]
    );
}
