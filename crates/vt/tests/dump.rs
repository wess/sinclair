use super::*;
use crate::cell::CellFlags;
use crate::color::Color;
use crate::term::Terminal;

/// Row-capped tests don't care about the byte budget.
const NO_BYTE_CAP: usize = usize::MAX;

/// Feed `input` into a terminal, dump it, then replay the dump into a fresh
/// terminal of the same size. Returns both so a test can assert the second
/// looks like the first.
fn round_trip(cols: usize, rows: usize, input: &str) -> (Terminal, Terminal) {
  let mut source = Terminal::new(cols, rows, 100);
  source.feed(input.as_bytes());
  let dump = source.buffer_dump(1000, NO_BYTE_CAP);
  let mut restored = Terminal::new(cols, rows, 100);
  restored.feed(dump.as_bytes());
  (source, restored)
}

fn visible(term: &mut Terminal) -> Vec<String> {
  (0..term.rows()).map(|r| term.visible_row(r).text()).collect()
}

fn cell_at(term: &mut Terminal, row: usize, col: usize) -> Cell {
  term.visible_row(row).cells[col]
}

#[test]
fn plain_text_round_trips() {
  let (mut source, mut restored) = round_trip(20, 4, "one\r\ntwo\r\nthree");
  assert_eq!(visible(&mut source), visible(&mut restored));
  assert_eq!(visible(&mut restored)[0], "one");
  assert_eq!(visible(&mut restored)[2], "three");
}

#[test]
fn colors_and_attributes_round_trip() {
  let (_, mut restored) = round_trip(
    20,
    2,
    "\x1b[1;31mred\x1b[0m \x1b[38;2;10;20;30;48;5;5mrgb\x1b[0m",
  );
  let red = cell_at(&mut restored, 0, 0);
  assert_eq!(red.fg, Color::Indexed(1));
  assert!(red.flags.contains(CellFlags::BOLD));
  let rgb = cell_at(&mut restored, 0, 4);
  assert_eq!(rgb.fg, Color::Rgb(10, 20, 30));
  assert_eq!(rgb.bg, Color::Indexed(5));
  // The pen is put back at the end of every row, so nothing bleeds past it.
  assert_eq!(cell_at(&mut restored, 1, 0), Cell::default());
}

#[test]
fn a_styled_row_does_not_bleed_into_the_next() {
  // The shell resets its own colors, so the row after a colored prompt is
  // plain. It has to come back plain: a row that ends inside a color and
  // never leaves it tints everything replayed after it.
  let (_, mut restored) = round_trip(20, 3, "\x1b[36mcyan\x1b[0m\r\nplain\r\n");
  assert_eq!(cell_at(&mut restored, 0, 0).fg, Color::Indexed(6));
  assert_eq!(cell_at(&mut restored, 1, 0).fg, Color::Default);
}

#[test]
fn a_run_returning_to_the_default_pen_is_reset() {
  let (_, mut restored) = round_trip(20, 2, "\x1b[41mred\x1b[0m plain");
  assert_eq!(cell_at(&mut restored, 0, 0).bg, Color::Indexed(1));
  for col in 3..9 {
    assert_eq!(cell_at(&mut restored, 0, col).bg, Color::Default, "col {col}");
  }
}

#[test]
fn underline_styles_and_colors_round_trip() {
  let (_, mut restored) = round_trip(20, 2, "\x1b[4:3m\x1b[58;5;9mwavy\x1b[0m");
  let cell = cell_at(&mut restored, 0, 0);
  assert!(cell.flags.contains(CellFlags::CURLY_UNDERLINE));
  assert_eq!(cell.underline_color, Color::Indexed(9));
}

#[test]
fn wide_characters_round_trip() {
  let (_, mut restored) = round_trip(20, 2, "日本語");
  assert_eq!(visible(&mut restored)[0], "日本語");
  assert!(cell_at(&mut restored, 0, 0).is_wide());
  assert!(cell_at(&mut restored, 0, 1).is_wide_spacer());
}

#[test]
fn combining_marks_round_trip() {
  let (_, mut restored) = round_trip(20, 2, "e\u{301}");
  assert_eq!(visible(&mut restored)[0], "e\u{301}");
}

#[test]
fn scrollback_is_included_and_capped() {
  let mut term = Terminal::new(10, 3, 100);
  for i in 0..20 {
    term.feed(format!("line{i}\r\n").as_bytes());
  }
  // Every line still fits in the 100-row scrollback, so a generous cap
  // brings all twenty back.
  let all = term.buffer_dump(1000, NO_BYTE_CAP);
  assert!(all.contains("line0"), "{all:?}");
  assert!(all.contains("line19"), "{all:?}");
  // A tight cap keeps the newest rows and drops the oldest. Blank padding is
  // trimmed before the cap applies, so five lines means five lines of
  // history, not four plus the row the cursor happened to sit on.
  let recent = term.buffer_dump(5, NO_BYTE_CAP);
  assert_eq!(
    recent,
    "line15\r\nline16\r\nline17\r\nline18\r\nline19\r\n"
  );
}

#[test]
fn trailing_blank_rows_and_cells_are_dropped() {
  let mut term = Terminal::new(40, 6, 100);
  term.feed(b"hi\r\n");
  let dump = term.buffer_dump(1000, NO_BYTE_CAP);
  // One row of content, no padding to the full width, no empty rows after.
  assert_eq!(dump, "hi\r\n");
}

#[test]
fn blank_history_is_kept_but_screen_padding_is_not() {
  let mut term = Terminal::new(10, 4, 100);
  term.feed(b"top\r\n\r\n\r\nbottom\r\n");
  // The blank lines something printed are history. The unused rows below
  // the cursor are not, and are dropped.
  assert_eq!(term.buffer_dump(1000, NO_BYTE_CAP), "top\r\n\r\n\r\nbottom\r\n");
}

#[test]
fn a_trailing_background_run_is_kept() {
  let mut term = Terminal::new(10, 2, 100);
  term.feed(b"\x1b[41m   \x1b[0m");
  let dump = term.buffer_dump(1000, NO_BYTE_CAP);
  assert!(dump.contains("41") || dump.contains("48;5;1"), "{dump:?}");
}

#[test]
fn empty_terminal_dumps_nothing() {
  let mut term = Terminal::new(10, 4, 100);
  assert_eq!(term.buffer_dump(1000, NO_BYTE_CAP), "");
  term.feed(b"x");
  assert_eq!(term.buffer_dump(0, NO_BYTE_CAP), "");
}

#[test]
fn the_alternate_screen_is_not_dumped() {
  let mut term = Terminal::new(20, 3, 100);
  term.feed(b"shell history\r\n");
  term.feed(b"\x1b[?1049h"); // enter alt screen
  term.feed(b"full screen program");
  let dump = term.buffer_dump(1000, NO_BYTE_CAP);
  assert!(dump.contains("shell history"), "{dump:?}");
  assert!(!dump.contains("full screen program"), "{dump:?}");
}

#[test]
fn a_soft_wrapped_line_rejoins_at_the_new_width() {
  // Twenty-four characters over twenty columns: two rows in the source, one
  // logical line. Replayed into a forty-column terminal it is one row again.
  let mut source = Terminal::new(20, 4, 100);
  source.feed(b"abcdefghijklmnopqrstuvwx\r\n");
  let dump = source.buffer_dump(1000, NO_BYTE_CAP);
  assert_eq!(dump.matches("\r\n").count(), 1, "{dump:?}");
  let mut restored = Terminal::new(40, 4, 100);
  restored.feed(dump.as_bytes());
  assert_eq!(visible(&mut restored)[0], "abcdefghijklmnopqrstuvwx");
}

#[test]
fn a_hard_break_stays_a_hard_break() {
  let mut source = Terminal::new(20, 4, 100);
  source.feed(b"short\r\nlines\r\n");
  let dump = source.buffer_dump(1000, NO_BYTE_CAP);
  let mut restored = Terminal::new(40, 4, 100);
  restored.feed(dump.as_bytes());
  assert_eq!(visible(&mut restored)[0], "short");
  assert_eq!(visible(&mut restored)[1], "lines");
}

#[test]
fn a_dump_replays_into_a_narrower_terminal_without_losing_rows() {
  let mut source = Terminal::new(40, 3, 100);
  source.feed(b"aaaa\r\nbbbb\r\ncccc\r\n");
  let dump = source.buffer_dump(1000, NO_BYTE_CAP);
  let mut restored = Terminal::new(10, 3, 100);
  restored.feed(dump.as_bytes());
  // Same rows, same order, the earliest scrolled into history on both sides.
  assert_eq!(visible(&mut restored), visible(&mut source));
  restored.scroll_display(3);
  assert_eq!(restored.visible_row(0).text(), "aaaa");
}

#[test]
fn the_byte_budget_keeps_the_newest_whole_rows() {
  let mut term = Terminal::new(20, 3, 100);
  for i in 0..50 {
    term.feed(format!("line{i:02}\r\n").as_bytes());
  }
  // Nine bytes a row ("lineNN\r\n" is 8), so a 30-byte budget buys three.
  let dump = term.buffer_dump(1000, 30);
  assert_eq!(dump.matches("\r\n").count(), 3, "{dump:?}");
  assert!(dump.starts_with("line47"), "{dump:?}");
  assert!(dump.ends_with("line49\r\n"), "{dump:?}");
}

#[test]
fn a_budget_smaller_than_one_row_keeps_nothing() {
  let mut term = Terminal::new(20, 3, 100);
  term.feed(b"a line of output\r\n");
  assert_eq!(term.buffer_dump(1000, 4), "");
  assert_eq!(term.buffer_dump(1000, 0), "");
}

#[test]
fn a_budget_never_starts_mid_logical_line() {
  // One logical line wrapped over three rows, then a short one. A budget
  // that can only hold part of the wrapped line drops all of it rather than
  // replaying a headless fragment.
  let mut term = Terminal::new(10, 4, 100);
  term.feed(b"aaaaaaaaaabbbbbbbbbbcccccccccc\r\nshort\r\n");
  let dump = term.buffer_dump(1000, 25);
  assert_eq!(dump, "short\r\n");
}

#[test]
fn the_row_cap_and_the_byte_cap_agree_on_the_newest_rows() {
  let mut term = Terminal::new(20, 3, 100);
  for i in 0..50 {
    term.feed(format!("line{i:02}\r\n").as_bytes());
  }
  let by_rows = term.buffer_dump(3, NO_BYTE_CAP);
  let by_bytes = term.buffer_dump(1000, 24);
  assert_eq!(by_rows, by_bytes);
}
