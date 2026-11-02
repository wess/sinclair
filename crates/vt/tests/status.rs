use super::*;
use crate::Terminal;

fn b64(s: &str) -> String {
  const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
  let bytes = s.as_bytes();
  let mut out = String::new();
  for chunk in bytes.chunks(3) {
    let n = chunk.iter().enumerate().fold(0u32, |acc, (i, &b)| acc | (b as u32) << (16 - 8 * i));
    for i in 0..=chunk.len() {
      out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
    }
  }
  out
}

fn set(body: &str) -> Record {
  match parse(body.as_bytes()) {
    Some(Report::Set(r)) => r,
    other => panic!("expected a record, got {other:?}"),
  }
}

fn term() -> Terminal {
  Terminal::new(80, 24, 100)
}

fn osc(t: &mut Terminal, body: &str) {
  t.feed(format!("\x1b]7501;{body}\x1b\\").as_bytes());
}

#[test]
fn parses_a_full_report() {
  let r = set(&format!(
    "state=blocked:kind=permission:app=terraform:progress=40:msg={}:title={}",
    b64("Apply 3 to add?"),
    b64("Plan")
  ));
  assert_eq!(r.id, "");
  assert_eq!(r.state, State::Blocked);
  assert_eq!(r.kind, Some(Kind::Permission));
  assert_eq!(r.progress, Some(40));
  assert_eq!(r.app.as_deref(), Some("terraform"));
  assert_eq!(r.msg.as_deref(), Some("Apply 3 to add?"));
  assert_eq!(r.title.as_deref(), Some("Plan"));
}

#[test]
fn state_is_required_and_must_be_known() {
  assert_eq!(parse(b"app=cargo"), None);
  assert_eq!(parse(b"state=sleeping"), None);
}

#[test]
fn malformed_pairs_are_skipped_and_last_key_wins() {
  let r = set("bogus: =x:state=idle:app=a:app=b:weird=x;y");
  assert_eq!(r.state, State::Idle);
  assert_eq!(r.app.as_deref(), Some("b"));
}

#[test]
fn whitespace_around_pairs_is_removed() {
  let r = set(" state = working : app = cargo ");
  assert_eq!(r.state, State::Working);
  assert_eq!(r.app.as_deref(), Some("cargo"));
}

#[test]
fn kind_and_progress_only_apply_to_their_states() {
  let r = set("state=done:kind=auth:progress=10");
  assert_eq!(r.kind, None);
  assert_eq!(r.progress, None);
  let r = set("state=working:progress=101");
  assert_eq!(r.progress, None);
  let r = set("state=blocked:kind=nope");
  assert_eq!(r.kind, None);
}

#[test]
fn bad_app_is_treated_as_absent() {
  let r = set("state=idle:app=has/slash");
  assert_eq!(r.app, None);
}

#[test]
fn control_characters_discard_the_report() {
  assert_eq!(parse(format!("state=idle:msg={}", b64("a\nb")).as_bytes()), None);
  assert_eq!(parse(b"state=idle:msg=a,b"), None);
}

#[test]
fn limits_discard_the_report() {
  let long = "a".repeat(2733);
  assert_eq!(parse(format!("state=idle:msg={long}").as_bytes()), None);
  let deep = ["a"; 9].join("/");
  assert_eq!(parse(format!("state=idle:id={deep}").as_bytes()), None);
  let seg = "a".repeat(33);
  assert_eq!(parse(format!("state=idle:id={seg}").as_bytes()), None);
  assert_eq!(parse(b"state=idle:abcdefghijklmnopq=1"), None);
}

#[test]
fn bad_id_is_ignored_rather_than_hitting_root() {
  assert_eq!(parse(b"state=idle:id=a//b"), None);
}

#[test]
fn query_is_answered() {
  let mut t = term();
  t.feed(b"\x1b]7501;?\x1b\\");
  let out = String::from_utf8(t.take_output()).unwrap();
  assert_eq!(out, format!("\x1b]{QUERY_REPLY}\x1b\\"));
  t.feed(b"\x1b]7501;?\x07");
  assert!(t.take_output().ends_with(b"\x07"));
}

#[test]
fn reports_replace_their_record() {
  let mut t = term();
  osc(&mut t, "state=working:app=brew");
  osc(&mut t, "state=done");
  let recs = t.status().records();
  assert_eq!(recs.len(), 1);
  assert_eq!(recs[0].state, State::Done);
  assert_eq!(recs[0].app, None);
  assert!(t.take_status_changed());
  assert!(!t.take_status_changed());
}

#[test]
fn clear_removes_a_subtree() {
  let mut t = term();
  osc(&mut t, "state=working:app=deploy");
  osc(&mut t, "state=working:id=us");
  osc(&mut t, "state=working:id=us/a");
  osc(&mut t, "state=working:id=usb");
  osc(&mut t, "state=clear:id=us");
  let ids: Vec<&str> = t.status().records().iter().map(|r| r.id.as_str()).collect();
  assert_eq!(ids, vec!["", "usb"]);
  osc(&mut t, "state=clear");
  assert!(t.status().is_empty());
}

#[test]
fn app_is_inherited_from_the_nearest_ancestor() {
  let mut t = term();
  osc(&mut t, "state=working:app=deploy");
  osc(&mut t, "state=working:id=eu/west");
  let board = t.status();
  let child = board.records().iter().find(|r| r.id == "eu/west").unwrap();
  assert_eq!(board.app_of(child), Some("deploy"));
}

#[test]
fn prompt_and_exit_end_transient_records() {
  let mut t = term();
  osc(&mut t, "state=working:id=a");
  osc(&mut t, "state=blocked:id=b");
  osc(&mut t, "state=done:id=c");
  osc(&mut t, "state=error:id=d");
  osc(&mut t, "state=idle:id=e");
  t.feed(b"\x1b]133;A\x07");
  let ids: Vec<&str> = t.status().records().iter().map(|r| r.id.as_str()).collect();
  assert_eq!(ids, vec!["c", "d", "e"]);
  osc(&mut t, "state=working:id=a");
  t.status_exited();
  assert_eq!(t.status().records().len(), 3);
  t.acknowledge_status();
  assert_eq!(t.status().records().len(), 1);
}

#[test]
fn ris_clears_and_decstr_does_not() {
  let mut t = term();
  osc(&mut t, "state=done");
  t.feed(b"\x1b[!p");
  assert_eq!(t.status().records().len(), 1);
  t.feed(b"\x1bc");
  assert!(t.status().is_empty());
}

#[test]
fn alt_screen_keeps_records() {
  let mut t = term();
  osc(&mut t, "state=working");
  t.feed(b"\x1b[?1049h");
  osc(&mut t, "state=working:id=x");
  t.feed(b"\x1b[?1049l");
  assert_eq!(t.status().records().len(), 2);
}

#[test]
fn conemu_progress_maps_to_root_until_native() {
  let mut t = term();
  t.feed(b"\x1b]9;4;1;40\x07");
  let r = &t.status().records()[0];
  assert_eq!((r.state, r.progress), (State::Working, Some(40)));
  t.feed(b"\x1b]9;4;3\x07");
  assert_eq!(t.status().records()[0].progress, None);
  t.feed(b"\x1b]9;4;2\x07");
  assert_eq!(t.status().records()[0].state, State::Error);
  t.feed(b"\x1b]9;4;0\x07");
  assert!(t.status().is_empty());
  // OSC 9;4 must not post a notification.
  assert!(t.take_notification().is_none());

  osc(&mut t, "state=blocked:kind=question");
  t.feed(b"\x1b]9;4;1;50\x07");
  assert_eq!(t.status().records()[0].kind, Some(Kind::Question));
  t.feed(b"\x1bc");
  t.feed(b"\x1b]9;4;1;50\x07");
  assert_eq!(t.status().records()[0].progress, Some(50));
}

#[test]
fn headline_prefers_urgency_then_recency() {
  let mut t = term();
  osc(&mut t, "state=working:app=deploy");
  osc(&mut t, "state=blocked:id=eu:kind=permission");
  osc(&mut t, "state=done:id=us");
  assert_eq!(t.status().headline().unwrap().id, "eu");
  osc(&mut t, "state=blocked:id=ap");
  assert_eq!(t.status().headline().unwrap().id, "ap");
}

#[test]
fn record_count_is_capped_by_evicting_the_oldest() {
  let mut t = term();
  for i in 0..=MAX_RECORDS {
    osc(&mut t, &format!("state=idle:id=r{i}"));
  }
  let recs = t.status().records();
  assert_eq!(recs.len(), MAX_RECORDS);
  assert!(!recs.iter().any(|r| r.id == "r0"));
}

#[test]
fn disarm_strips_invisible_formatting() {
  assert_eq!(disarm("a\u{202E}b\u{200B}c\u{2066}"), "abc");
}

#[test]
fn depth_counts_segments() {
  let mut t = term();
  osc(&mut t, "state=idle");
  osc(&mut t, "state=idle:id=a/b");
  let depths: Vec<usize> = t.status().records().iter().map(|r| r.depth()).collect();
  assert_eq!(depths, vec![0, 2]);
}
