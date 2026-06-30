use crate::grid::damage::Damage;
use crate::selection::{Point, SelectionAdjust, SelectionMode};
use crate::term::Terminal;

fn p(line: isize, col: usize) -> Point {
    Point::new(line, col)
}

#[test]
fn cell_selection_and_text() {
    let mut t = Terminal::new(20, 3, 0);
    t.feed(b"hello world");
    t.start_selection(SelectionMode::Cell, p(0, 0));
    t.update_selection(p(0, 4));
    assert_eq!(t.selection_text().as_deref(), Some("hello"));
}

#[test]
fn selection_normalizes_upward_drag() {
    let mut t = Terminal::new(10, 3, 0);
    t.feed(b"ab\r\ncd");
    t.start_selection(SelectionMode::Cell, p(1, 1));
    t.update_selection(p(0, 0));
    let sel = t.selection().unwrap();
    assert_eq!(sel.start(), p(0, 0));
    assert_eq!(sel.end(), p(1, 1));
    assert_eq!(t.selection_text().as_deref(), Some("ab\ncd"));
}

#[test]
fn word_selection_honors_word_chars() {
    let mut t = Terminal::new(20, 3, 0);
    t.feed(b"foo bar.gz baz");
    t.start_selection(SelectionMode::Word, p(0, 5));
    assert_eq!(t.selection_text().as_deref(), Some("bar.gz"));
    t.set_word_chars("");
    t.start_selection(SelectionMode::Word, p(0, 5));
    assert_eq!(t.selection_text().as_deref(), Some("bar"));
}

#[test]
fn word_selection_crosses_soft_wrap() {
    let mut t = Terminal::new(4, 3, 10);
    t.feed(b"xx abcd"); // wraps: "xx a" + "bcd"
    assert!(t.grid().row(0).wrapped);
    t.start_selection(SelectionMode::Word, p(0, 3));
    assert_eq!(t.selection_text().as_deref(), Some("abcd"));
}

#[test]
fn line_selection_spans_logical_line() {
    let mut t = Terminal::new(4, 3, 10);
    t.feed(b"abcdef\r\nxyz");
    t.start_selection(SelectionMode::Line, p(1, 0));
    let sel = t.selection().unwrap();
    assert_eq!(sel.start(), p(0, 0));
    assert_eq!(sel.end(), p(1, 3));
    assert_eq!(t.selection_text().as_deref(), Some("abcdef"));
}

#[test]
fn text_soft_wrap_joins_hard_break_newlines() {
    let mut t = Terminal::new(4, 3, 10);
    t.feed(b"abcdef\r\nxyz");
    t.start_selection(SelectionMode::Cell, p(0, 0));
    t.update_selection(p(2, 3));
    assert_eq!(t.selection_text().as_deref(), Some("abcdef\nxyz"));
}

#[test]
fn text_handles_wide_chars_and_trailing_spaces() {
    let mut t = Terminal::new(10, 3, 0);
    t.feed(b"\xe6\xbc\xa2\xe5\xad\x97 x  \r\nok"); // "漢字 x  "
    t.start_selection(SelectionMode::Cell, p(0, 0));
    t.update_selection(p(1, 9));
    assert_eq!(t.selection_text().as_deref(), Some("漢字 x\nok"));
}

#[test]
fn update_without_selection_is_noop() {
    let mut t = Terminal::new(10, 3, 0);
    t.update_selection(p(0, 5));
    assert!(t.selection().is_none());
    assert_eq!(t.selection_text(), None);
}

#[test]
fn selection_changes_are_full_damage() {
    let mut t = Terminal::new(10, 3, 0);
    t.feed(b"abc");
    t.take_damage();
    t.start_selection(SelectionMode::Cell, p(0, 0));
    assert_eq!(t.take_damage(), Damage::Full);
    t.update_selection(p(0, 2));
    assert_eq!(t.take_damage(), Damage::Full);
    t.clear_selection();
    assert_eq!(t.take_damage(), Damage::Full);
    // Clearing nothing is not damage.
    t.clear_selection();
    assert_eq!(t.take_damage(), Damage::Rows(vec![]));
}

#[test]
fn print_inside_selection_clears_it() {
    let mut t = Terminal::new(10, 4, 0);
    t.feed(b"abc");
    t.start_selection(SelectionMode::Cell, p(0, 0));
    t.update_selection(p(0, 2));
    // Printing on an unselected row keeps the selection.
    t.feed(b"\x1b[3;1Hx");
    assert!(t.selection().is_some());
    // Printing on the selected row clears it.
    t.feed(b"\x1b[1;2Hy");
    assert!(t.selection().is_none());
}

#[test]
fn selection_stays_anchored_across_scroll_into_history() {
    let mut t = Terminal::new(4, 2, 10);
    t.feed(b"ab\r\ncd");
    t.start_selection(SelectionMode::Cell, p(0, 0));
    t.update_selection(p(0, 1));
    t.feed(b"\r\nef"); // scrolls "ab" into scrollback
    let sel = t.selection().unwrap();
    assert_eq!(sel.start(), p(-1, 0));
    assert_eq!(t.selection_text().as_deref(), Some("ab"));
}

#[test]
fn selection_invalidated_when_scrolled_off_ring() {
    let mut t = Terminal::new(4, 2, 1);
    t.feed(b"ab\r\ncd");
    t.start_selection(SelectionMode::Cell, p(0, 0));
    t.update_selection(p(0, 1));
    t.feed(b"\r\ne"); // "ab" -> ring (len 1), selection at line -1
    assert!(t.selection().is_some());
    t.feed(b"\r\nf"); // "cd" -> ring, "ab" evicted: selection off-ring
    assert!(t.selection().is_none());
}

#[test]
fn region_scroll_clears_selection() {
    let mut t = Terminal::new(10, 4, 10);
    t.feed(b"sel\x1b[2;3r\x1b[3;1H");
    t.start_selection(SelectionMode::Cell, p(0, 0));
    t.update_selection(p(0, 2));
    t.feed(b"\n"); // scrolls rows 1..=2 only (top != 0): conservative clear
    assert!(t.selection().is_none());
}

#[test]
fn reverse_scroll_clears_selection() {
    let mut t = Terminal::new(10, 3, 10);
    t.feed(b"sel\x1b[1;1H");
    t.start_selection(SelectionMode::Cell, p(0, 0));
    t.feed(b"\x1bM"); // RI at the top: scroll down
    assert!(t.selection().is_none());
}

#[test]
fn alt_screen_switch_clears_selection() {
    let mut t = Terminal::new(10, 3, 10);
    t.feed(b"abc");
    t.start_selection(SelectionMode::Cell, p(0, 0));
    t.feed(b"\x1b[?1049h");
    assert!(t.selection().is_none());
}

#[test]
fn erase_display_clears_only_when_intersecting() {
    let mut t = Terminal::new(4, 2, 10);
    t.feed(b"ab\r\ncd\r\nef"); // "ab" now in scrollback
    t.start_selection(SelectionMode::Cell, p(-1, 0));
    t.update_selection(p(-1, 1));
    t.feed(b"\x1b[2J"); // live rows only: scrollback selection survives
    assert!(t.selection().is_some());
    t.feed(b"\x1b[3J"); // history erased: selection dies, offset resets
    assert!(t.selection().is_none());
    assert_eq!(t.display_offset(), 0);
}

#[test]
fn resize_and_ris_clear_selection() {
    let mut t = Terminal::new(10, 3, 10);
    t.feed(b"abc");
    t.start_selection(SelectionMode::Cell, p(0, 0));
    t.resize(8, 4);
    assert!(t.selection().is_none());
    t.feed(b"abc");
    t.start_selection(SelectionMode::Cell, p(0, 0));
    t.feed(b"\x1bc");
    assert!(t.selection().is_none());
}

#[test]
fn adjust_selection_without_selection_is_noop() {
    let mut t = Terminal::new(10, 3, 0);
    t.feed(b"abcde");
    assert!(!t.adjust_selection(SelectionAdjust::Right));
    assert!(t.selection().is_none());
}

#[test]
fn adjust_selection_extends_extent() {
    let mut t = Terminal::new(10, 3, 0);
    t.feed(b"abcde");
    t.start_selection(SelectionMode::Cell, p(0, 1));
    // Anchor at (0,1); extend the moving end right twice.
    assert!(t.adjust_selection(SelectionAdjust::Right));
    assert!(t.adjust_selection(SelectionAdjust::Right));
    let sel = t.selection().unwrap();
    assert_eq!(sel.start(), p(0, 1));
    assert_eq!(sel.end(), p(0, 3));
    // Pull it back left past the anchor; start now precedes it.
    assert!(t.adjust_selection(SelectionAdjust::Left));
    assert!(t.adjust_selection(SelectionAdjust::Left));
    assert!(t.adjust_selection(SelectionAdjust::Left));
    let sel = t.selection().unwrap();
    assert_eq!(sel.start(), p(0, 0));
    assert_eq!(sel.end(), p(0, 1));
}

#[test]
fn adjust_selection_left_wraps_to_prior_row() {
    let mut t = Terminal::new(4, 3, 0);
    t.feed(b"abcd\r\nwxyz");
    t.start_selection(SelectionMode::Cell, p(1, 0));
    assert!(t.adjust_selection(SelectionAdjust::Left));
    let sel = t.selection().unwrap();
    // Wrapped from (1,0) to the last column of the row above.
    assert_eq!(sel.start(), p(0, 3));
    assert_eq!(sel.end(), p(1, 0));
}

#[test]
fn adjust_selection_up_reveals_scrollback() {
    let mut t = Terminal::new(4, 2, 10);
    // Push several lines into scrollback, leaving the view at the bottom.
    t.feed(b"l1\r\nl2\r\nl3\r\nl4\r\nl5");
    assert_eq!(t.display_offset(), 0);
    t.start_selection(SelectionMode::Cell, p(1, 0));
    // Walk the moving end up into history; the view scrolls to follow.
    for _ in 0..3 {
        assert!(t.adjust_selection(SelectionAdjust::Up));
    }
    assert!(t.display_offset() > 0);
    assert!(t.selection().unwrap().start().line < 0);
}

#[test]
fn contains_matches_renderer_view() {
    let mut t = Terminal::new(10, 3, 0);
    t.feed(b"ab\r\ncd\r\nef");
    t.start_selection(SelectionMode::Cell, p(0, 1));
    t.update_selection(p(2, 0));
    let sel = t.selection().unwrap();
    assert!(!sel.contains(p(0, 0)));
    assert!(sel.contains(p(0, 1)));
    assert!(sel.contains(p(1, 5))); // full middle row
    assert!(sel.contains(p(2, 0)));
    assert!(!sel.contains(p(2, 1)));
}

#[test]
fn word_right_extends_by_word() {
    let mut t = Terminal::new(20, 3, 0);
    t.feed(b"foo bar baz");
    t.start_selection(SelectionMode::Cell, p(0, 0));
    assert!(t.adjust_selection(SelectionAdjust::WordRight));
    assert_eq!(t.selection_text().as_deref(), Some("foo"));
    assert!(t.adjust_selection(SelectionAdjust::WordRight));
    assert_eq!(t.selection_text().as_deref(), Some("foo bar"));
}

#[test]
fn word_left_extends_by_word() {
    let mut t = Terminal::new(20, 3, 0);
    t.feed(b"foo bar baz");
    t.start_selection(SelectionMode::Cell, p(0, 10));
    assert!(t.adjust_selection(SelectionAdjust::WordLeft));
    assert_eq!(t.selection_text().as_deref(), Some("baz"));
    assert!(t.adjust_selection(SelectionAdjust::WordLeft));
    assert_eq!(t.selection_text().as_deref(), Some("bar baz"));
}

#[test]
fn extend_selection_starts_at_cursor() {
    let mut t = Terminal::new(20, 3, 0);
    t.feed(b"hello world");
    assert!(t.selection().is_none());
    // No selection yet: extend begins one at the cursor, then grows a word.
    t.extend_selection(SelectionAdjust::WordLeft);
    assert_eq!(t.selection_text().as_deref(), Some("world"));
}

#[test]
fn word_motion_crosses_rows() {
    let mut t = Terminal::new(6, 3, 0);
    t.feed(b"foo\r\nbar");
    // From the start of "bar", a word-left step crosses up to "foo".
    t.start_selection(SelectionMode::Cell, p(1, 0));
    assert!(t.adjust_selection(SelectionAdjust::WordLeft));
    let sel = t.selection().unwrap();
    assert_eq!(sel.start(), p(0, 0));
    assert_eq!(sel.end(), p(1, 0));
}
