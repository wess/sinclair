use crate::charset::Charset;
use crate::term::Terminal;

#[test]
fn bel_sets_flag() {
    let mut t = Terminal::new(10, 3, 0);
    t.feed(b"\x07");
    assert!(t.take_bell());
    assert!(!t.take_bell());
}

#[test]
fn backspace_moves_left() {
    let mut t = Terminal::new(10, 3, 0);
    t.feed(b"ab\x08c");
    assert_eq!(t.row_text(0), "ac");
}

#[test]
fn ht_moves_to_tab_stop() {
    let mut t = Terminal::new(20, 3, 0);
    t.feed(b"\tx");
    assert_eq!(t.cell(0, 8).ch, 'x');
}

#[test]
fn nel_is_cr_plus_lf() {
    let mut t = Terminal::new(10, 3, 0);
    t.feed(b"ab\x1bEc");
    assert_eq!(t.row_text(1), "c");
    assert_eq!(t.cursor_pos(), (1, 1));
}

#[test]
fn hts_sets_tab_stop() {
    let mut t = Terminal::new(20, 3, 0);
    t.feed(b"\x1b[5G\x1bH\r\tx");
    assert_eq!(t.cell(0, 4).ch, 'x');
}

#[test]
fn keypad_modes() {
    let mut t = Terminal::new(10, 3, 0);
    t.feed(b"\x1b=");
    assert!(t.modes().contains(crate::Modes::APP_KEYPAD));
    t.feed(b"\x1b>");
    assert!(!t.modes().contains(crate::Modes::APP_KEYPAD));
}

#[test]
fn charset_designation_and_shift() {
    let mut t = Terminal::new(10, 3, 0);
    t.feed(b"\x1b(0");
    assert_eq!(t.inner.charsets.g0, Charset::DecSpecial);
    t.feed(b"q");
    assert_eq!(t.cell(0, 0).ch, '─');
    t.feed(b"\x1b(B");
    t.feed(b"q");
    assert_eq!(t.cell(0, 1).ch, 'q');
    // SO selects G1.
    t.feed(b"\x1b)0\x0eq\x0fq");
    assert_eq!(t.cell(0, 2).ch, '─');
    assert_eq!(t.cell(0, 3).ch, 'q');
}

#[test]
fn ris_resets() {
    let mut t = Terminal::new(10, 3, 0);
    t.feed(b"hello\x1b[?25l\x1bc");
    assert_eq!(t.row_text(0), "");
    assert!(t.cursor_visible());
}
