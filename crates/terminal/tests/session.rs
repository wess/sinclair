use super::*;
use std::time::Instant;

fn command(args: &[&str]) -> SessionOptions {
    SessionOptions::command(args.iter().map(|s| s.to_string()).collect())
}

/// Collect events until `Exit`; returns the others plus the exit code.
fn drain_until_exit(rx: &Receiver<Event>) -> (Vec<Event>, Option<i32>) {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut seen = Vec::new();
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match rx.recv_timeout(remaining) {
            Ok(Event::Exit(code)) => return (seen, code),
            Ok(event) => seen.push(event),
            Err(e) => panic!("no Exit before timeout; saw {seen:?}: {e}"),
        }
    }
}

#[test]
fn exit_event_carries_exit_code() {
    let (_session, rx) = Session::spawn(command(&["/bin/sh", "-c", "exit 7"])).expect("spawn");
    let (_, code) = drain_until_exit(&rx);
    assert_eq!(code, Some(7));
}

#[test]
fn write_reaches_child() {
    let (session, rx) =
        Session::spawn(command(&["/bin/sh", "-c", "read line; exit 5"])).expect("spawn");
    session.write(b"go\n").expect("write");
    let (_, code) = drain_until_exit(&rx);
    assert_eq!(code, Some(5));
}

#[test]
fn wakeup_is_coalesced_until_term_access() {
    // Two output bursts, but the embedder never observes the terminal,
    // so exactly one Wakeup may be queued.
    let (_session, rx) =
        Session::spawn(command(&["/bin/sh", "-c", "printf a; sleep 1; printf b"]))
            .expect("spawn");
    let (seen, _) = drain_until_exit(&rx);
    let wakeups = seen.iter().filter(|e| **e == Event::Wakeup).count();
    assert_eq!(wakeups, 1, "events: {seen:?}");
}

#[test]
fn with_term_rearms_wakeup() {
    let (session, rx) = Session::spawn(command(&[
        "/bin/sh",
        "-c",
        "printf a; sleep 1; printf b; sleep 1",
    ]))
    .expect("spawn");
    assert_eq!(
        rx.recv_timeout(Duration::from_secs(10))
            .expect("first wakeup"),
        Event::Wakeup
    );
    session.with_term(|_| ()); // re-arms the pending flag
    assert_eq!(
        rx.recv_timeout(Duration::from_secs(10))
            .expect("second wakeup"),
        Event::Wakeup
    );
}

#[test]
fn title_and_bell_events() {
    let (_session, rx) = Session::spawn(command(&[
        "/bin/sh",
        "-c",
        "printf '\\033]2;mytitle\\007'; printf '\\007'",
    ]))
    .expect("spawn");
    let (seen, _) = drain_until_exit(&rx);
    assert!(
        seen.contains(&Event::TitleChanged("mytitle".to_string())),
        "events: {seen:?}"
    );
    assert!(seen.contains(&Event::Bell), "events: {seen:?}");
}

#[test]
fn osc52_surfaces_clipboard_event() {
    // base64("hi") = "aGk="
    let (_session, rx) =
        Session::spawn(command(&["/bin/sh", "-c", "printf '\\033]52;c;aGk=\\007'"]))
            .expect("spawn");
    let (seen, _) = drain_until_exit(&rx);
    assert!(
        seen.contains(&Event::Clipboard {
            kind: "c".to_string(),
            data: b"hi".to_vec(),
        }),
        "events: {seen:?}"
    );
}

#[test]
fn resize_updates_grid_immediately() {
    let (session, _rx) =
        Session::spawn(command(&["/bin/sh", "-c", "sleep 30"])).expect("spawn");
    session.resize(90, 28).expect("resize");
    let size = session.with_term(|t| (t.cols(), t.rows()));
    assert_eq!(size, (90, 28));
}

#[test]
fn shutdown_kills_and_reports_exit() {
    let (session, rx) = Session::spawn(command(&["/bin/sh", "-c", "sleep 30"])).expect("spawn");
    session.shutdown();
    let (_, code) = drain_until_exit(&rx);
    assert_eq!(code, None); // killed by signal, no exit code
}
