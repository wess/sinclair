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
fn extra_fields_are_tolerated() {
    assert!(is_newer("1.21.0-beta.1", "1.20.0"));
    assert!(is_newer("1.21.0+build5", "1.20.9"));
}
