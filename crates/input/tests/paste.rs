use super::*;

#[test]
fn bracketed_wraps_text() {
  assert_eq!(encode_paste("hello", true), b"\x1b[200~hello\x1b[201~");
  assert_eq!(encode_paste("", true), b"\x1b[200~\x1b[201~");
}

#[test]
fn bracketed_preserves_newlines_verbatim() {
  assert_eq!(encode_paste("a\nb", true), b"\x1b[200~a\nb\x1b[201~");
}

#[test]
fn bracketed_strips_embedded_end_marker() {
  assert_eq!(
    encode_paste("evil\x1b[201~rm -rf\x1b[201~ /", true),
    b"\x1b[200~evil[201~rm -rf[201~ /\x1b[201~"
  );
}

#[test]
fn a_split_end_marker_cannot_reassemble() {
  // Removing only the literal marker would leave `ESC[201~` behind here.
  let out = encode_paste("a\x1b[20\x1b[201~1~\nrm -rf ~\n", true);
  let inner = &out[6..out.len() - 6];
  assert!(
    !inner.contains(&0x1b),
    "no ESC may survive inside the bracket"
  );
}

#[test]
fn controls_are_stripped_but_formatting_survives() {
  assert_eq!(
    encode_paste("a\tb\x07c\x18d\x7fe", true),
    b"\x1b[200~a\tbcde\x1b[201~"
  );
  // C1 CSI (U+009B) is dropped too.
  assert_eq!(
    encode_paste("x\u{9b}201~y", true),
    b"\x1b[200~x201~y\x1b[201~"
  );
  assert_eq!(encode_paste("ok\x1b[200~still", false), b"ok[200~still");
}

#[test]
fn unicode_is_untouched() {
  assert_eq!(encode_paste("héllo ✓", false), "héllo ✓".as_bytes());
}

#[test]
fn plain_normalizes_newlines_to_cr() {
  let cases: &[(&str, &[u8])] = &[
    ("hello", b"hello"),
    ("a\nb", b"a\rb"),
    ("a\r\nb", b"a\rb"),
    ("a\rb", b"a\rb"),
    ("\n\n", b"\r\r"),
    ("", b""),
  ];
  for (text, want) in cases {
    assert_eq!(encode_paste(text, false), *want, "{text:?}");
  }
}
