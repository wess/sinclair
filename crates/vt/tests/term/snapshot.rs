use crate::mode::Modes;
use crate::Terminal;

/// Feed `bytes` into a terminal, snapshot it, replay the snapshot into a
/// fresh one of the same size, and hand both back.
fn roundtrip(cols: usize, rows: usize, bytes: &[u8]) -> (Terminal, Terminal) {
  let mut a = Terminal::new(cols, rows, 1000);
  a.feed(bytes);
  let snap = a.snapshot();
  let mut b = Terminal::new(cols, rows, 1000);
  b.feed(&snap);
  (a, b)
}

fn screen_rows(t: &Terminal) -> Vec<String> {
  (0..t.rows()).map(|r| t.grid().row(r).text()).collect()
}

fn cursor(t: &Terminal) -> (usize, usize) {
  (t.screen().cursor.row, t.screen().cursor.col)
}

#[test]
fn text_scrollback_and_cursor_survive() {
  let mut input = Vec::new();
  for i in 0..30 {
    input.extend_from_slice(format!("line {i}\r\n").as_bytes());
  }
  input.extend_from_slice(b"\x1b[31mred\x1b[0m $ ");
  let (mut a, mut b) = roundtrip(20, 5, &input);
  assert_eq!(screen_rows(&a), screen_rows(&b));
  assert_eq!(a.buffer_text(), b.buffer_text());
  assert_eq!(cursor(&a), cursor(&b));
}

#[test]
fn soft_wrapped_rows_land_where_they_were() {
  let (a, b) = roundtrip(10, 4, b"abcdefghij   klm\r\nnext");
  assert_eq!(screen_rows(&a), screen_rows(&b));
  assert_eq!(cursor(&a), cursor(&b));
}

#[test]
fn alt_screen_comes_back_over_the_primary() {
  let (mut a, mut b) = roundtrip(
    20,
    5,
    b"shell history\r\n$ vim\x1b[?1049h\x1b[2J\x1b[Hediting\x1b[3;4H",
  );
  assert!(b.is_alt_screen());
  assert_eq!(screen_rows(&a), screen_rows(&b));
  assert_eq!(cursor(&a), cursor(&b));
  a.feed(b"\x1b[?1049l");
  b.feed(b"\x1b[?1049l");
  assert_eq!(screen_rows(&a), screen_rows(&b));
}

#[test]
fn modes_title_and_region_survive() {
  let (a, b) = roundtrip(
    20,
    6,
    b"\x1b[?1h\x1b=\x1b[?2004h\x1b[?1002h\x1b[?1006h\x1b[?25l\x1b]2;my title\x07\x1b[2;5r\x1b[4;3H\x1b[>5u\x1b[5 q",
  );
  let m = Modes::APP_CURSOR
    | Modes::APP_KEYPAD
    | Modes::BRACKETED_PASTE
    | Modes::MOUSE_DRAG
    | Modes::MOUSE_SGR;
  assert!(b.modes().contains(m));
  assert!(!b.modes().contains(Modes::CURSOR_VISIBLE));
  assert_eq!(b.title(), "my title");
  assert_eq!(b.screen().scroll_top, 1);
  assert_eq!(b.screen().scroll_bottom, 4);
  assert_eq!(cursor(&a), cursor(&b));
  assert_eq!(b.kitty_keyboard_flags(), 5);
  assert_eq!(b.cursor_style(), a.cursor_style());
}

#[test]
fn status_records_survive() {
  let (_, b) = roundtrip(
    20,
    4,
    b"\x1b]7501;state=blocked:kind=question:app=claude:msg=V2hpY2g/\x1b\\\x1b]7501;state=working:id=a/b:progress=40\x1b\\",
  );
  let recs = b.status().records();
  assert_eq!(recs.len(), 2);
  assert_eq!(recs[0].msg.as_deref(), Some("Which?"));
  assert_eq!(recs[1].id, "a/b");
  assert_eq!(recs[1].progress, Some(40));
}

#[test]
fn pen_is_restored() {
  let (a, b) = roundtrip(20, 4, b"\x1b[1;32mbold green ");
  assert_eq!(a.screen().cursor.pen, b.screen().cursor.pen);
}

#[test]
fn snapshot_writes_nothing_back() {
  let (_, mut b) = roundtrip(20, 4, b"hello");
  assert!(b.take_output().is_empty());
}
