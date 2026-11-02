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
    message(Some(127), true),
    "Process exited with code 127 — press any key to close"
  );
  assert_eq!(
    message(Some(0), true),
    "Process exited immediately — press any key to close"
  );
  assert_eq!(message(Some(0), false), "Process exited — press any key to close");
  assert!(message(None, false).contains("signal"));
}

#[test]
fn keep_open_extends_lingering() {
  let long = Duration::from_secs(60);
  assert!(!stays(long, config::KeepOpen::Never, Some(1)));
  assert!(stays(long, config::KeepOpen::Failure, Some(1)));
  assert!(!stays(long, config::KeepOpen::Failure, Some(0)));
  assert!(stays(long, config::KeepOpen::Always, Some(0)));
  assert!(stays(Duration::ZERO, config::KeepOpen::Never, Some(0)));
}
