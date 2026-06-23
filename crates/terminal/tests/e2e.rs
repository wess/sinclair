//! End-to-end session tests: real shells on a real pty.

#![cfg(unix)]

use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use terminal::{Event, Session, SessionOptions};

const TIMEOUT: Duration = Duration::from_secs(10);

fn shell(args: &[&str]) -> SessionOptions {
    SessionOptions::command(args.iter().map(|s| s.to_string()).collect())
}

/// Flatten the visible grid rows into one newline-joined string.
fn grid_text(session: &Session) -> String {
    session.with_term(|term| {
        term.visible_rows()
            .map(|row| row.text())
            .collect::<Vec<_>>()
            .join("\n")
    })
}

/// Poll the grid until `ok` accepts its text; panic with a dump on timeout.
fn wait_for(session: &Session, what: &str, ok: impl Fn(&str) -> bool) {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let text = grid_text(session);
        if ok(&text) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {what}; grid was:\n{text}"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// Block until an `Exit` event arrives; panic on timeout.
fn wait_for_exit(events: &Receiver<Event>) -> Option<i32> {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match events.recv_timeout(remaining) {
            Ok(Event::Exit(code)) => return code,
            Ok(_) => {}
            Err(e) => panic!("no Exit event before timeout: {e}"),
        }
    }
}

#[test]
fn oneshot_output_lands_in_grid() {
    let (session, _events) =
        Session::spawn(shell(&["/bin/sh", "-c", "printf 'hello terminal'"])).expect("spawn");
    wait_for(&session, "hello terminal", |text| {
        text.contains("hello terminal")
    });
}

#[test]
fn interactive_echo_and_exit() {
    let (session, events) = Session::spawn(shell(&["/bin/sh"])).expect("spawn");
    session.write(b"echo marker42\n").expect("write echo");
    // The command-output line holds "marker42" without the "echo" that the
    // tty-echoed input line carries.
    wait_for(&session, "marker42 output line", |text| {
        text.lines()
            .any(|line| line.contains("marker42") && !line.contains("echo"))
    });
    session.write(b"exit\n").expect("write exit");
    wait_for_exit(&events);
}

#[test]
fn resize_reaches_child_and_grid() {
    let (session, _events) = Session::spawn(shell(&["/bin/sh"])).expect("spawn");
    session.resize(100, 30).expect("resize");
    let size = session.with_term(|t| (t.cols(), t.rows()));
    assert_eq!(size, (100, 30));
    session.write(b"stty size\n").expect("write stty");
    wait_for(&session, "stty to report 30 100", |text| {
        text.contains("30 100")
    });
}
