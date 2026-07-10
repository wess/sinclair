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
fn writes_arrive_in_order() {
    let (session, rx) = Session::spawn(command(&[
        "/bin/sh",
        "-c",
        "read a; read b; [ \"$a$b\" = onetwo ] && exit 3; exit 1",
    ]))
    .expect("spawn");
    session.write(b"one\n").expect("write one");
    session.write(b"two\n").expect("write two");
    let (_, code) = drain_until_exit(&rx);
    assert_eq!(code, Some(3));
}

#[test]
fn large_input_is_delivered_completely() {
    // head exits only once it has read exactly this many bytes from the tty.
    const TOTAL: usize = 256 * 1024;
    let (session, rx) = Session::spawn(command(&[
        "/bin/sh",
        "-c",
        &format!("head -c {TOTAL} >/dev/null; exit 9"),
    ]))
    .expect("spawn");
    // Newlines keep each line under the canonical-mode line limit.
    let line = [b"x".repeat(511).as_slice(), b"\n"].concat();
    let big: Vec<u8> = line
        .iter()
        .copied()
        .cycle()
        .take(TOTAL)
        .collect();
    let start = Instant::now();
    session.write(&big).expect("write");
    // The write only queues; it must return long before the child drains it.
    assert!(start.elapsed() < Duration::from_secs(2));
    let (_, code) = drain_until_exit(&rx);
    assert_eq!(code, Some(9));
}

#[test]
fn write_does_not_block_on_a_stalled_child() {
    let (session, _rx) =
        Session::spawn(command(&["/bin/sh", "-c", "sleep 30"])).expect("spawn");
    let big = vec![b'x'; 1 << 20];
    let start = Instant::now();
    session.write(&big).expect("write");
    assert!(
        start.elapsed() < Duration::from_secs(2),
        "write blocked on a child that never reads"
    );
    // Dropping with the queue still full must not hang either.
    let (done_tx, done_rx) = mpsc::channel();
    thread::spawn(move || {
        drop(session);
        let _ = done_tx.send(());
    });
    done_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("drop hung with a full input queue");
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
fn resize_px_updates_grid_and_kernel_winsize() {
    let (session, rx) = Session::spawn(command(&[
        "/bin/sh",
        "-c",
        "sleep 1; stty size",
    ]))
    .expect("spawn");
    session.resize_px(100, 30, 8, 16).expect("resize_px");
    let size = session.with_term(|t| (t.cols(), t.rows()));
    assert_eq!(size, (100, 30));
    let (_, code) = drain_until_exit(&rx);
    assert_eq!(code, Some(0));
    let text = session.with_term(|t| {
        (0..t.rows())
            .map(|r| t.row_text(r))
            .collect::<Vec<_>>()
            .join("\n")
    });
    assert!(text.contains("30 100"), "grid was:\n{text}");
}

#[test]
fn shutdown_kills_and_reports_exit() {
    let (session, rx) = Session::spawn(command(&["/bin/sh", "-c", "sleep 30"])).expect("spawn");
    session.shutdown();
    let (_, code) = drain_until_exit(&rx);
    assert_eq!(code, None); // killed by signal, no exit code
}

#[test]
fn drop_does_not_hang_when_a_background_child_holds_the_pty() {
    // The shell exits immediately, but its background child inherits the pty
    // slave, so the reader never sees EOF. Teardown must still finish.
    let (session, rx) =
        Session::spawn(command(&["/bin/sh", "-c", "sleep 30 & exit 0"])).expect("spawn");
    std::thread::sleep(Duration::from_millis(100)); // let the shell exit
    let (done_tx, done_rx) = mpsc::channel();
    thread::spawn(move || {
        drop(session);
        let _ = done_tx.send(());
    });
    done_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("drop hung on a lingering pty slave");
    drain_until_exit(&rx);
}

#[test]
fn drop_escalates_when_the_child_ignores_hangup() {
    let (session, rx) =
        Session::spawn(command(&["/bin/sh", "-c", "trap '' HUP; sleep 30"])).expect("spawn");
    std::thread::sleep(Duration::from_millis(100)); // let the trap install
    let (done_tx, done_rx) = mpsc::channel();
    thread::spawn(move || {
        drop(session);
        let _ = done_tx.send(());
    });
    done_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("drop hung on a SIGHUP-ignoring child");
    let (_, code) = drain_until_exit(&rx);
    assert_eq!(code, None); // group SIGKILL
}
