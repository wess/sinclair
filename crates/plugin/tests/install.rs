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
    installed.record(
        "git",
        "0.2.0",
        "catalog:git",
        vec!["commands".into(), "filesystem".into()],
    );
    assert_eq!(
        installed.granted("git"),
        &["commands".to_string(), "filesystem".to_string()]
    );

    let text = toml::to_string(&installed).unwrap();
    let back: Installed = toml::from_str(&text).unwrap();
    assert_eq!(back.plugins["git"].version, "0.2.0");
    assert_eq!(back.plugins["git"].source, "catalog:git");
    assert!(back.is_enabled("git"));
    assert_eq!(back.granted("git").len(), 2);
}

#[test]
fn effective_capabilities_enforce_consent() {
    let declared = vec![
        "commands".to_string(),
        "network".to_string(),
        "filesystem".to_string(),
    ];
    let mut installed = Installed::default();

    // Untracked (built-in / local): declared set is granted implicitly.
    assert_eq!(installed.effective_capabilities("git", &declared), declared);

    // Tracked with a narrower grant: the plugin can't reach `network`.
    installed.record(
        "git",
        "0.2.0",
        "catalog:git",
        vec!["commands".into(), "filesystem".into()],
    );
    assert_eq!(
        installed.effective_capabilities("git", &declared),
        vec!["commands".to_string(), "filesystem".to_string()]
    );
}

#[test]
fn grants_beyond_the_declared_set_do_not_widen() {
    let declared = vec!["commands".to_string()];
    let mut installed = Installed::default();
    // A grant for something the plugin never declared confers nothing, and an
    // empty grant yields nothing at all: strictly `granted ∩ declared`.
    installed.record(
        "git",
        "0.1.0",
        "catalog:git",
        vec!["network".into(), "commands".into()],
    );
    assert_eq!(installed.effective_capabilities("git", &declared), declared);
    installed.record("git", "0.1.0", "catalog:git", Vec::new());
    assert!(installed
        .effective_capabilities("git", &declared)
        .is_empty());
}

#[test]
fn record_update_keeps_a_disabled_plugin_disabled() {
    let mut installed = Installed::default();
    installed.record("git", "0.1.0", "catalog:git", vec!["commands".into()]);
    installed.set_enabled("git", false);

    installed.record("git", "0.2.0", "catalog:git", vec!["commands".into()]);
    assert!(!installed.is_enabled("git"));
    assert_eq!(installed.plugins["git"].version, "0.2.0");
}

#[test]
fn save_and_load_round_trip_through_a_file() {
    let dir = std::env::temp_dir().join(format!("sinclairinstalled{}", std::process::id()));
    let path = dir.join("state/installed.toml");
    let mut installed = Installed::default();
    installed.record("git", "0.2.0", "catalog:git", vec!["commands".into()]);
    installed.set_enabled("todos", false);

    installed.save_to(&path).unwrap();
    let back = Installed::load_from(&path);
    assert_eq!(back.plugins["git"], installed.plugins["git"]);
    assert!(!back.is_enabled("todos"));
    assert!(back.is_enabled("untracked"));

    // Absent or unparsable files load as empty, never fail.
    assert!(Installed::load_from(&dir.join("absent.toml"))
        .plugins
        .is_empty());
    std::fs::write(dir.join("broken.toml"), "not toml [").unwrap();
    assert!(Installed::load_from(&dir.join("broken.toml"))
        .plugins
        .is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}
