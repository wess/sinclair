use super::*;

#[test]
fn cursor_returns_only_newer_events() {
  let mut j = Journal::default();
  assert_eq!(j.head(), 0);
  j.push(1, "bell", json!({}), 10);
  let start = j.head();
  j.push(2, "exit", json!({"exit_code": 0}), 11);
  j.push(1, "bell", json!({}), 12);
  let (events, next, lost) = j.since(start, None);
  assert_eq!(events.len(), 2);
  assert_eq!(next, 3);
  assert!(!lost);
  let (events, _, _) = j.since(start, Some(2));
  assert_eq!(events[0].kind, "exit");
  assert_eq!(events[0].to_json()["pane"], "pane:2");
}

#[test]
fn falling_behind_the_buffer_is_reported() {
  let mut j = Journal::default();
  for i in 0..(CAPACITY as u64 + 5) {
    j.push(1, "bell", json!({}), i);
  }
  let (events, _, lost) = j.since(0, None);
  assert!(lost);
  assert_eq!(events.len(), CAPACITY);
  let (_, _, lost) = j.since(j.head(), None);
  assert!(!lost);
}
