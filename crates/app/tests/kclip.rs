use super::*;

fn png() -> Vec<u8> {
  b"\x89PNG\r\n\x1a\nfake".to_vec()
}

#[test]
fn text_mimes_are_recognized_by_any_common_name() {
  for mime in [
    "text/plain",
    "text/plain;charset=utf-8",
    "TEXT/PLAIN",
    "UTF8_STRING",
    "STRING",
  ] {
    assert!(is_text(mime), "{mime}");
  }
  assert!(!is_text("text/html"));
  assert!(!is_text("image/png"));
}

#[test]
fn a_write_becomes_text_and_image_entries() {
  let items = vec![
    ("text/html".to_string(), b"<b>x</b>".to_vec()),
    ("text/plain".to_string(), b"x".to_vec()),
    ("image/png".to_string(), png()),
  ];
  let item = clipboard_item(&items, &[]).expect("representable");
  assert_eq!(item.text().as_deref(), Some("x"));
  assert_eq!(types_of(&item), vec!["text/plain", "image/png"]);
}

#[test]
fn an_alias_can_mark_text() {
  let items = vec![("application/x-mine".to_string(), b"hi".to_vec())];
  let aliases = vec![(
    "application/x-mine".to_string(),
    vec!["text/plain".to_string()],
  )];
  assert_eq!(
    clipboard_item(&items, &aliases).unwrap().text().as_deref(),
    Some("hi")
  );
}

#[test]
fn nothing_representable_is_no_item() {
  let items = vec![("text/html".to_string(), b"<i>".to_vec())];
  assert!(clipboard_item(&items, &[]).is_none());
}

#[test]
fn reads_return_requested_types_in_order_and_skip_missing() {
  let items = vec![
    ("text/plain".to_string(), b"hello".to_vec()),
    ("image/png".to_string(), png()),
  ];
  let item = clipboard_item(&items, &[]).unwrap();
  let want = vec![
    "image/png".to_string(),
    "text/html".to_string(),
    "UTF8_STRING".to_string(),
  ];
  let got = read_items(&item, &want);
  assert_eq!(got.len(), 2);
  assert_eq!(got[0], ("image/png".to_string(), png()));
  assert_eq!(got[1], ("UTF8_STRING".to_string(), b"hello".to_vec()));
}

#[test]
fn one_time_passwords_differ() {
  let a = one_time_password();
  let b = one_time_password();
  assert_eq!(a.len(), 32);
  assert_ne!(a, b);
}

#[test]
fn the_prompt_names_the_client_and_sizes() {
  let request = vt::ClipboardRequest::Write {
    primary: false,
    id: None,
    items: vec![("image/png".to_string(), vec![0; 2048])],
    aliases: vec![],
    auth: Some(Auth {
      password: "p".into(),
      name: "vim".into(),
    }),
  };
  assert_eq!(
    TerminalView::clipboard_summary(&request),
    "“vim” wants to set the clipboard (image/png, 2.0 KB)."
  );
}
