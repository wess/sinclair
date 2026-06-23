use super::*;

fn type_str(rec: &mut Recorder, s: &str) {
    for ch in s.chars() {
        let mut buf = [0u8; 4];
        rec.key("", Some(ch.encode_utf8(&mut buf)));
    }
}

#[test]
fn captures_lines_on_enter() {
    let mut rec = Recorder::new();
    rec.start();
    type_str(&mut rec, "echo hi");
    rec.key("enter", None);
    type_str(&mut rec, "ls -la");
    rec.key("enter", None);
    assert_eq!(rec.len(), 2);
    assert_eq!(rec.finish(), vec!["echo hi".to_string(), "ls -la".to_string()]);
    assert!(!rec.is_active());
}

#[test]
fn backspace_edits_current_line() {
    let mut rec = Recorder::new();
    rec.start();
    type_str(&mut rec, "echoo");
    rec.key("backspace", None);
    type_str(&mut rec, " hi");
    rec.key("enter", None);
    assert_eq!(rec.finish(), vec!["echo hi".to_string()]);
}

#[test]
fn trailing_unsubmitted_line_is_kept() {
    let mut rec = Recorder::new();
    rec.start();
    type_str(&mut rec, "pending");
    assert_eq!(rec.finish(), vec!["pending".to_string()]);
}

#[test]
fn blank_lines_and_control_text_are_ignored() {
    let mut rec = Recorder::new();
    rec.start();
    rec.key("enter", None); // blank submission
    rec.key("", Some("\u{1b}[A")); // an arrow escape: dropped
    type_str(&mut rec, "  real  ");
    rec.key("enter", None);
    assert_eq!(rec.finish(), vec!["real".to_string()]);
}

#[test]
fn inactive_recorder_ignores_input() {
    let mut rec = Recorder::new();
    type_str(&mut rec, "ignored");
    rec.key("enter", None);
    assert!(rec.finish().is_empty());
}

#[test]
fn cancel_drops_everything() {
    let mut rec = Recorder::new();
    rec.start();
    type_str(&mut rec, "echo hi");
    rec.key("enter", None);
    rec.cancel();
    assert!(!rec.is_active());
    assert!(rec.finish().is_empty());
}
