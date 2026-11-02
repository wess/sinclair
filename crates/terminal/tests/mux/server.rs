use super::*;
use crate::mux::{client, Attach};
use std::time::Instant;

fn socket() -> std::path::PathBuf {
  let dir = std::env::temp_dir().join(format!("sinclair-mux-test-{}-{}", std::process::id(), new_id(&Server::default())));
  std::fs::create_dir_all(&dir).unwrap();
  dir.join("s.sock")
}

fn start(path: &std::path::Path) {
  let p = path.to_path_buf();
  thread::spawn(move || serve(&p, Duration::from_secs(60)));
  let deadline = Instant::now() + Duration::from_secs(5);
  while !client::alive(path) {
    assert!(Instant::now() < deadline, "server did not come up");
    thread::sleep(Duration::from_millis(20));
  }
}

fn until(session: &Session, what: &str) -> String {
  let deadline = Instant::now() + Duration::from_secs(10);
  loop {
    let text = session.with_term(|t| t.buffer_text());
    if text.contains(what) {
      return text;
    }
    assert!(Instant::now() < deadline, "never saw {what:?}; screen was {text:?}");
    thread::sleep(Duration::from_millis(20));
  }
}

fn shell(script: &str) -> Attach {
  Attach::Open {
    options: SessionOptions::command(vec!["/bin/sh".into(), "-c".into(), script.into()]),
    token: 42,
  }
}

#[test]
fn a_session_outlives_its_client() {
  let path = socket();
  start(&path);
  let (s, _events, info) =
    Session::attach(&path, shell("echo first; read x; echo got $x; sleep 30"), 40, 10, 100).unwrap();
  assert!(s.is_remote());
  assert_eq!(info.token, 42);
  until(&s, "first");
  s.detach();

  let listed = client::list(&path).unwrap();
  assert_eq!(listed.len(), 1);
  assert_eq!(listed[0].id, info.id);

  // A new client sees the history from the snapshot, then live output.
  let (s, _events, again) =
    Session::attach(&path, Attach::Existing(info.id.clone()), 40, 10, 100).unwrap();
  assert_eq!(again.token, 42);
  until(&s, "first");
  s.write(b"ping\n").unwrap();
  until(&s, "got ping");

  // Dropping a session outside of quit ends it.
  drop(s);
  let deadline = Instant::now() + Duration::from_secs(5);
  while !client::list(&path).unwrap().is_empty() {
    assert!(Instant::now() < deadline, "session was not killed");
    thread::sleep(Duration::from_millis(20));
  }
}

#[test]
fn the_exit_code_reaches_the_client() {
  let path = socket();
  start(&path);
  let (_s, events, _) = Session::attach(&path, shell("exit 7"), 40, 10, 100).unwrap();
  let deadline = Instant::now() + Duration::from_secs(10);
  loop {
    match events.recv_timeout(Duration::from_millis(100)) {
      Ok(Event::Exit(code)) => {
        assert_eq!(code, Some(7));
        break;
      }
      _ => assert!(Instant::now() < deadline, "no exit"),
    }
  }
}

#[test]
fn attaching_to_a_missing_session_fails_cleanly() {
  let path = socket();
  start(&path);
  let Err(err) = Session::attach(&path, Attach::Existing("nope".into()), 40, 10, 100) else {
    panic!("attached to a session that does not exist");
  };
  assert!(err.to_string().contains("no such session"), "{err}");
}
