use super::*;

#[test]
fn matches_and_titles() {
  let t = Triggers::compile(&["error|Build failed".to_string(), "\\bWARN\\b".to_string()]);
  assert_eq!(
    t.check("fatal error here"),
    Some(("Build failed".to_string(), "fatal error here".to_string()))
  );
  // Untitled trigger uses the default title.
  assert_eq!(
    t.check("a WARN line"),
    Some(("Trigger".to_string(), "a WARN line".to_string()))
  );
  assert_eq!(t.check("all good"), None);
}

#[test]
fn empty_and_invalid() {
  assert!(Triggers::compile(&[]).is_empty());
  // Invalid regex is skipped; the valid one still matches.
  let t = Triggers::compile(&["(".to_string(), "ok".to_string()]);
  assert!(t.check("ok").is_some());
}
