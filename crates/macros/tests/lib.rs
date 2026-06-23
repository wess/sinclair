use super::*;

#[test]
fn name_validation() {
    assert!(valid_name("deploy"));
    assert!(valid_name("build.all"));
    assert!(valid_name("step-1"));
    assert!(!valid_name(""));
    assert!(!valid_name("Deploy"));
    assert!(!valid_name("build all"));
}

#[test]
fn sanitize_coerces_free_text() {
    assert_eq!(sanitize_name("Build And Test").as_deref(), Some("build-and-test"));
    assert_eq!(sanitize_name("deploy_prod").as_deref(), Some("deploy-prod"));
    assert_eq!(sanitize_name("  spaced  ").as_deref(), Some("spaced"));
    assert_eq!(sanitize_name("a--b").as_deref(), Some("a-b"));
    assert_eq!(sanitize_name("***"), None);
    assert_eq!(sanitize_name(""), None);
}
