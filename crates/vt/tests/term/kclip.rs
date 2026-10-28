use super::*;
use crate::term::Terminal;

fn osc(meta: &str, payload: Option<&[u8]>) -> Vec<u8> {
  let mut out = format!("\x1b]5522;{meta}").into_bytes();
  if let Some(p) = payload {
    out.push(b';');
    out.extend_from_slice(encode(p).as_bytes());
  }
  out.extend_from_slice(b"\x1b\\");
  out
}

fn b64(s: &str) -> String {
  encode(s.as_bytes())
}

#[test]
fn base64_round_trips_and_is_strict() {
  for input in [&b""[..], b"f", b"fo", b"foo", b"foob", b"fooba", b"foobar"] {
    assert_eq!(decode(encode(input).as_bytes()).as_deref(), Some(input));
  }
  assert_eq!(encode(b"foobar"), "Zm9vYmFy");
  assert_eq!(encode(b"fo"), "Zm8=");
  // Whitespace, missing padding, padding mid-stream: all refused.
  assert_eq!(decode(b"Zm9v YmFy"), None);
  assert_eq!(decode(b"Zm8"), None);
  assert_eq!(decode(b"Zm8=Zm8="), None);
  assert_eq!(decode(b"Z==="), None);
  assert_eq!(decode(b"Zg==Zg=="), None);
}

#[test]
fn write_chunks_may_be_split_raw_or_padded_per_chunk() {
  let mut t = Terminal::new(10, 3, 0);
  let data = b"abcdefgh".to_vec();
  let whole = encode(&data); // YWJjZGVmZ2g=
  t.feed(&osc("type=write", None));
  for part in [&whole[..5], &whole[5..]] {
    t.feed(
      format!(
        "\x1b]5522;type=wdata:mime={};{part}\x1b\\",
        b64("text/plain")
      )
      .as_bytes(),
    );
  }
  t.feed(&osc("type=wdata", None));
  let reqs = t.take_clipboard_requests();
  let ClipboardRequest::Write { items, .. } = &reqs[0] else {
    panic!("{reqs:?}");
  };
  assert_eq!(items[0].1, data);
}

#[test]
fn a_read_is_queued_for_the_host() {
  let mut t = Terminal::new(10, 3, 0);
  t.feed(&osc("type=read:id=a/b!c", Some(b"text/plain image/png")));
  let reqs = t.take_clipboard_requests();
  assert_eq!(
    reqs,
    vec![ClipboardRequest::Read {
      primary: false,
      id: Some("abc".into()),
      mimes: vec!["text/plain".into(), "image/png".into()],
      auth: None,
    }]
  );
  assert!(t.take_clipboard_requests().is_empty());
}

#[test]
fn a_password_needs_a_name() {
  let mut t = Terminal::new(10, 3, 0);
  t.feed(&osc(
    &format!("type=read:loc=primary:pw={}", b64("s3")),
    Some(b"."),
  ));
  t.feed(&osc(
    &format!("type=read:pw={}:name={}", b64("s3"), b64("vim")),
    Some(b"."),
  ));
  let reqs = t.take_clipboard_requests();
  let auths: Vec<_> = reqs
    .iter()
    .map(|r| match r {
      ClipboardRequest::Read { primary, auth, .. } => (*primary, auth.clone()),
      _ => unreachable!(),
    })
    .collect();
  assert_eq!(auths[0], (true, None));
  assert_eq!(
    auths[1],
    (
      false,
      Some(Auth {
        password: "s3".into(),
        name: "vim".into()
      })
    )
  );
}

#[test]
fn a_chunked_write_assembles_per_type() {
  let mut t = Terminal::new(10, 3, 0);
  let png = vec![0x89u8; 6000];
  t.feed(&osc("type=write:id=w1", None));
  t.feed(&osc(
    &format!("type=wdata:mime={}", b64("text/plain")),
    Some(b"hello"),
  ));
  // Two chunks of one type, each padded on its own, still concatenate.
  t.feed(&osc(
    &format!("type=wdata:mime={}", b64("image/png")),
    Some(&png[..4096]),
  ));
  t.feed(&osc(
    &format!("type=wdata:mime={}", b64("image/png")),
    Some(&png[4096..]),
  ));
  t.feed(&osc(
    &format!("type=walias:mime={}", b64("text/plain")),
    Some(b"UTF8_STRING TEXT"),
  ));
  t.feed(&osc("type=wdata", None));
  let reqs = t.take_clipboard_requests();
  let ClipboardRequest::Write {
    id, items, aliases, ..
  } = &reqs[0]
  else {
    panic!("{reqs:?}");
  };
  assert_eq!(id.as_deref(), Some("w1"));
  assert_eq!(items[0], ("text/plain".into(), b"hello".to_vec()));
  assert_eq!(items[1].0, "image/png");
  assert_eq!(items[1].1, png);
  assert_eq!(
    aliases,
    &vec![(
      "text/plain".to_string(),
      vec!["UTF8_STRING".to_string(), "TEXT".to_string()]
    )]
  );
  assert!(t.take_output().is_empty(), "the host answers a valid write");
}

#[test]
fn bad_write_data_is_einval_and_the_rest_is_ignored() {
  let mut t = Terminal::new(10, 3, 0);
  t.feed(&osc("type=write:id=x", None));
  t.feed(
    format!(
      "\x1b]5522;type=wdata:mime={};not base64!\x1b\\",
      b64("text/plain")
    )
    .as_bytes(),
  );
  t.feed(&osc("type=wdata", None));
  assert_eq!(
    t.take_output(),
    b"\x1b]5522;type=write:status=EINVAL:id=x\x1b\\"
  );
  assert!(t.take_clipboard_requests().is_empty());
  // Data with no MIME type is invalid too.
  t.feed(&osc("type=write", None));
  t.feed(&osc("type=wdata", Some(b"orphan")));
  assert_eq!(t.take_output(), b"\x1b]5522;type=write:status=EINVAL\x1b\\");
}

#[test]
fn wdata_without_a_write_is_ignored() {
  let mut t = Terminal::new(10, 3, 0);
  t.feed(&osc(
    &format!("type=wdata:mime={}", b64("text/plain")),
    Some(b"x"),
  ));
  t.feed(&osc("type=wdata", None));
  assert!(t.take_clipboard_requests().is_empty());
  assert!(t.take_output().is_empty());
}

#[test]
fn an_unread_queue_answers_ebusy_instead_of_growing() {
  let mut t = Terminal::new(10, 3, 0);
  for _ in 0..20 {
    t.feed(&osc("type=read", Some(b".")));
  }
  assert_eq!(t.take_clipboard_requests().len(), MAX_PENDING);
  let out = String::from_utf8(t.take_output()).unwrap();
  assert_eq!(out.matches("status=EBUSY").count(), 20 - MAX_PENDING);
}

#[test]
fn read_reply_chunks_at_4096_bytes() {
  let data = vec![b'x'; CHUNK + 10];
  let out = String::from_utf8(read_reply(Some("r"), &[("text/plain", &data)])).unwrap();
  assert!(out.starts_with("\x1b]5522;type=read:status=OK:id=r\x1b\\"));
  assert_eq!(out.matches("status=DATA").count(), 2);
  assert!(out.ends_with("\x1b]5522;type=read:status=DONE:id=r\x1b\\"));
}

#[test]
fn paste_event_matches_the_spec_example() {
  let out = paste_event(false, "secret123", &["text/plain\n"]);
  // From the addendum's example (its list carries a trailing newline).
  assert_eq!(
    String::from_utf8(out).unwrap(),
    "\x1b]5522;type=read:status=OK:pw=c2VjcmV0MTIz\x1b\\\
     \x1b]5522;type=read:status=DATA:mime=Lg==:pw=c2VjcmV0MTIz;dGV4dC9wbGFpbgo=\x1b\\\
     \x1b]5522;type=read:status=DONE:pw=c2VjcmV0MTIz\x1b\\"
  );
}

#[test]
fn mode_5522_is_reported_and_toggles() {
  let mut t = Terminal::new(10, 3, 0);
  t.feed(b"\x1b[?5522$p");
  assert_eq!(t.take_output(), b"\x1b[?5522;2$y");
  t.feed(b"\x1b[?5522h");
  assert!(t.clipboard_paste_events());
  t.feed(b"\x1b[?5522$p");
  assert_eq!(t.take_output(), b"\x1b[?5522;1$y");
}
