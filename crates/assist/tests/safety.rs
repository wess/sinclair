use super::*;

#[test]
fn multiline_is_medium_risk() {
    assert_eq!(analyze("echo a\necho b").level, RiskLevel::Medium);
}

#[test]
fn destructive_is_high_risk() {
    assert_eq!(analyze("rm -rf /tmp/x").level, RiskLevel::High);
}
