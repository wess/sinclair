use super::*;

#[test]
fn only_a_quick_exit_lingers() {
  assert!(lingers(Duration::ZERO));
  assert!(lingers(Duration::from_millis(249)));
  assert!(!lingers(Duration::from_millis(250)));
  assert!(!lingers(Duration::from_secs(3600)));
}

#[test]
fn the_message_names_the_status() {
  assert_eq!(
    message(Some(127)),
    "Process exited with code 127 — press any key to close"
  );
  assert_eq!(
    message(Some(0)),
    "Process exited immediately — press any key to close"
  );
  assert!(message(None).contains("signal"));
}
