use crate::term::Terminal;

fn term() -> Terminal {
    Terminal::new(10, 4, 50)
}

#[test]
fn write_char_advances() {
    let mut t = term();
    t.inner.write_char('a');
    t.inner.write_char('b');
    assert_eq!(t.row_text(0), "ab");
    assert_eq!(t.cursor_pos(), (0, 2));
}

#[test]
fn wide_char_takes_two_cells() {
    let mut t = term();
    t.inner.write_char('漢');
    assert!(t.cell(0, 0).is_wide());
    assert!(t.cell(0, 1).is_wide_spacer());
    assert_eq!(t.cursor_pos(), (0, 2));
}

#[test]
fn overwriting_wide_head_clears_spacer() {
    let mut t = term();
    t.inner.write_char('漢');
    t.inner.cursor_to(0, 0);
    t.inner.write_char('x');
    assert_eq!(t.cell(0, 0).ch, 'x');
    assert!(!t.cell(0, 1).is_wide_spacer());
}

#[test]
fn overwriting_spacer_clears_head() {
    let mut t = term();
    t.inner.write_char('漢');
    t.inner.cursor_to(0, 1);
    t.inner.write_char('x');
    assert_eq!(t.cell(0, 1).ch, 'x');
    assert!(!t.cell(0, 0).is_wide());
    assert_eq!(t.cell(0, 0).ch, ' ');
}

#[test]
fn pending_wrap_then_write_wraps() {
    let mut t = term();
    for _ in 0..10 {
        t.inner.write_char('x');
    }
    assert_eq!(t.cursor_pos(), (0, 9));
    assert!(t.screen().cursor.pending_wrap);
    t.inner.write_char('y');
    assert_eq!(t.cursor_pos(), (1, 1));
    assert_eq!(t.cell(1, 0).ch, 'y');
    assert!(t.grid().row(0).wrapped);
}

#[test]
fn cursor_movement_clamps() {
    let mut t = term();
    t.inner.cursor_up(5);
    assert_eq!(t.cursor_pos(), (0, 0));
    t.inner.cursor_down(99);
    assert_eq!(t.cursor_pos(), (3, 0));
    t.inner.cursor_right(99);
    assert_eq!(t.cursor_pos(), (3, 9));
    t.inner.cursor_left(99);
    assert_eq!(t.cursor_pos(), (3, 0));
}

#[test]
fn scroll_region_constrains_linefeed() {
    let mut t = term();
    // 1-based margins 2..3 = rows 1..2 zero-based.
    t.inner.set_scroll_region(2, 3);
    t.inner.cursor_to(1, 0);
    t.inner.write_char('a');
    t.inner.linefeed();
    t.inner.carriage_return();
    t.inner.write_char('b');
    t.inner.linefeed(); // at bottom margin: scrolls region, cursor stays
    assert_eq!(t.row_text(1), "b");
    assert_eq!(t.row_text(0), ""); // row 0 untouched
    assert_eq!(t.row_text(3), ""); // below region untouched
    assert_eq!(t.cursor_pos().0, 2);
}

#[test]
fn delete_chars_shifts_left() {
    let mut t = term();
    t.feed(b"abcdef");
    t.inner.cursor_to(0, 1);
    t.inner.delete_chars(2);
    assert_eq!(t.row_text(0), "adef");
}

#[test]
fn insert_blank_shifts_right() {
    let mut t = term();
    t.feed(b"abc");
    t.inner.cursor_to(0, 1);
    t.inner.insert_blank(2);
    assert_eq!(t.row_text(0), "a  bc");
}

#[test]
fn full_reset_restores_defaults() {
    let mut t = term();
    t.feed(b"\x1b[?25l\x1b[31mhello");
    t.inner.full_reset();
    assert!(t.cursor_visible());
    assert_eq!(t.row_text(0), "");
    assert_eq!(t.cursor_pos(), (0, 0));
}

#[test]
fn alignment_test_fills_screen() {
    let mut t = term();
    t.inner.screen_alignment_test();
    assert_eq!(t.row_text(0), "EEEEEEEEEE");
    assert_eq!(t.row_text(3), "EEEEEEEEEE");
}

/// 10-col terminal whose row 0 soft-wrapped into row 1.
fn wrapped() -> Terminal {
    let mut t = term();
    t.feed(b"0123456789ab");
    assert!(t.grid().row(0).wrapped);
    t
}

#[test]
fn erase_line_to_end_breaks_wrap() {
    let mut t = wrapped();
    t.feed(b"\x1b[1;5H\x1b[K"); // EL 0 from mid-row
    assert!(!t.grid().row(0).wrapped);
    let mut t = wrapped();
    t.feed(b"\x1b[1;1H\x1b[2K"); // EL 2
    assert!(!t.grid().row(0).wrapped);
    // EL 1 leaves the tail (and the continuation) intact.
    let mut t = wrapped();
    t.feed(b"\x1b[1;5H\x1b[1K");
    assert!(t.grid().row(0).wrapped);
}

#[test]
fn erase_display_below_breaks_wrap_on_cursor_row() {
    let mut t = wrapped();
    t.feed(b"\x1b[1;5H\x1b[J");
    assert!(!t.grid().row(0).wrapped);
}

#[test]
fn char_edits_break_wrap() {
    let mut t = wrapped();
    t.feed(b"\x1b[1;1H\x1b[P"); // DCH shifts the tail left
    assert!(!t.grid().row(0).wrapped);
    let mut t = wrapped();
    t.feed(b"\x1b[1;1H\x1b[@"); // ICH pushes content off the right edge
    assert!(!t.grid().row(0).wrapped);
    let mut t = wrapped();
    t.feed(b"\x1b[1;10H\x1b[X"); // ECH reaching the last column
    assert!(!t.grid().row(0).wrapped);
    // ECH that stops short keeps the continuation.
    let mut t = wrapped();
    t.feed(b"\x1b[1;1H\x1b[X");
    assert!(t.grid().row(0).wrapped);
}

#[test]
fn overwriting_last_cell_breaks_wrap() {
    let mut t = wrapped();
    t.feed(b"\x1b[1;10HZ");
    assert!(!t.grid().row(0).wrapped);
    // ...and a fresh continuation re-sets it.
    t.feed(b"\x1b[2;1H"); // park the cursor; flag stays cleared
    assert!(!t.grid().row(0).wrapped);
    let mut t = wrapped();
    t.feed(b"\x1b[1;10HZw"); // overwrite then continue: re-wrapped
    assert!(t.grid().row(0).wrapped);
    assert_eq!(t.cell(1, 0).ch, 'w');
}
