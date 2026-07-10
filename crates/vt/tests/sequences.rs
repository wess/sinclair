//! Integration tests: feed real escape sequences, assert grid state.

use vt::{Cell, CellFlags, Color, Modes, Terminal};

fn term(cols: usize, rows: usize) -> Terminal {
    Terminal::new(cols, rows, 100)
}

fn screen_text(t: &mut Terminal) -> Vec<String> {
    (0..t.rows()).map(|r| t.row_text(r)).collect()
}

fn cell(t: &Terminal, row: usize, col: usize) -> Cell {
    *t.cell(row, col)
}

#[test]
fn plain_printing() {
    let mut t = term(10, 3);
    t.feed(b"hello");
    assert_eq!(t.row_text(0), "hello");
    assert_eq!(t.cursor_pos(), (0, 5));
}

#[test]
fn cr_lf() {
    let mut t = term(10, 3);
    t.feed(b"ab\r\ncd\rX");
    assert_eq!(t.row_text(0), "ab");
    assert_eq!(t.row_text(1), "Xd");
    assert_eq!(t.cursor_pos(), (1, 1));
}

#[test]
fn autowrap_with_pending_wrap() {
    let mut t = term(5, 3);
    t.feed(b"abcde");
    // Pending wrap: cursor holds at the last column.
    assert_eq!(t.cursor_pos(), (0, 4));
    t.feed(b"f");
    assert_eq!(t.row_text(0), "abcde");
    assert_eq!(t.row_text(1), "f");
    assert_eq!(t.cursor_pos(), (1, 1));
}

#[test]
fn pending_wrap_cancelled_by_cr() {
    let mut t = term(5, 3);
    t.feed(b"abcde\rX");
    // CR cleared the pending wrap; X overwrites column 0 of row 0.
    assert_eq!(t.row_text(0), "Xbcde");
    assert_eq!(t.row_text(1), "");
}

#[test]
fn autowrap_disabled_overwrites_last_column() {
    let mut t = term(5, 3);
    t.feed(b"\x1b[?7labcdefg");
    assert_eq!(t.row_text(0), "abcdg");
    assert_eq!(t.row_text(1), "");
    t.feed(b"\x1b[?7h");
    assert!(t.modes().contains(Modes::AUTOWRAP));
}

#[test]
fn wide_chars_occupy_two_cells() {
    let mut t = term(6, 3);
    t.feed("漢字".as_bytes());
    assert!(cell(&t, 0, 0).flags.contains(CellFlags::WIDE));
    assert!(cell(&t, 0, 1).flags.contains(CellFlags::WIDE_SPACER));
    assert_eq!(cell(&t, 0, 2).ch, '字');
    assert_eq!(t.row_text(0), "漢字");
    assert_eq!(t.cursor_pos(), (0, 4));
}

#[test]
fn wide_char_wraps_instead_of_splitting() {
    let mut t = term(5, 3);
    t.feed("abcd漢".as_bytes());
    assert_eq!(t.row_text(0), "abcd");
    assert_eq!(t.row_text(1), "漢");
}

#[test]
fn cup_and_hvp_position_cursor() {
    let mut t = term(10, 5);
    t.feed(b"\x1b[3;4Ha");
    assert_eq!(cell(&t, 2, 3).ch, 'a');
    t.feed(b"\x1b[2;2fb");
    assert_eq!(cell(&t, 1, 1).ch, 'b');
    // Out-of-range clamps.
    t.feed(b"\x1b[99;99Hc");
    assert_eq!(cell(&t, 4, 9).ch, 'c');
}

#[test]
fn ed_variants() {
    let mut t = term(4, 3);
    t.feed(b"aaaa\r\nbbbb\r\ncccc\x1b[2;2H");
    t.feed(b"\x1b[0J");
    assert_eq!(screen_text(&mut t), vec!["aaaa", "b", ""]);

    let mut t = term(4, 3);
    t.feed(b"aaaa\r\nbbbb\r\ncccc\x1b[2;2H");
    t.feed(b"\x1b[1J");
    assert_eq!(screen_text(&mut t), vec!["", "  bb", "cccc"]);

    let mut t = term(4, 3);
    t.feed(b"aaaa\r\nbbbb\r\ncccc");
    t.feed(b"\x1b[2J");
    assert_eq!(screen_text(&mut t), vec!["", "", ""]);
}

#[test]
fn el_variants() {
    let mut t = term(6, 1);
    t.feed(b"abcdef\x1b[1;4H\x1b[K");
    assert_eq!(t.row_text(0), "abc");

    let mut t = term(6, 1);
    t.feed(b"abcdef\x1b[1;4H\x1b[1K");
    assert_eq!(t.row_text(0), "    ef");

    let mut t = term(6, 1);
    t.feed(b"abcdef\x1b[2K");
    assert_eq!(t.row_text(0), "");
}

#[test]
fn erase_uses_pen_background() {
    let mut t = term(4, 2);
    t.feed(b"\x1b[44mab\x1b[1;1H\x1b[K");
    assert_eq!(cell(&t, 0, 0).bg, Color::Indexed(4));
    assert_eq!(cell(&t, 0, 0).ch, ' ');
}

#[test]
fn scroll_region_with_ind_and_ri() {
    let mut t = term(4, 4);
    t.feed(b"a\r\nb\r\nc\r\nd");
    // Region rows 2-3 (1-based), cursor homes after DECSTBM.
    t.feed(b"\x1b[2;3r");
    assert_eq!(t.cursor_pos(), (0, 0));
    // IND from the region bottom scrolls only the region.
    t.feed(b"\x1b[3;1H\x1bD");
    assert_eq!(screen_text(&mut t), vec!["a", "c", "", "d"]);
    // RI from the region top scrolls the region down.
    t.feed(b"\x1b[2;1H\x1bM");
    assert_eq!(screen_text(&mut t), vec!["a", "", "c", "d"]);
}

#[test]
fn scroll_region_su_sd() {
    let mut t = term(4, 4);
    t.feed(b"a\r\nb\r\nc\r\nd\x1b[2;3r\x1b[1S");
    assert_eq!(screen_text(&mut t), vec!["a", "c", "", "d"]);
    t.feed(b"\x1b[1T");
    assert_eq!(screen_text(&mut t), vec!["a", "", "c", "d"]);
}

#[test]
fn alt_screen_enter_exit_restores_primary() {
    let mut t = term(10, 3);
    t.feed(b"primary\x1b[31m");
    t.feed(b"\x1b[?1049h");
    assert!(t.is_alt_screen());
    assert_eq!(t.row_text(0), "");
    // The cursor position carries over into the alternate screen.
    assert_eq!(t.cursor_pos(), (0, 7));
    t.feed(b"\x1b[Halt");
    assert_eq!(t.row_text(0), "alt");
    t.feed(b"\x1b[?1049l");
    assert!(!t.is_alt_screen());
    assert_eq!(t.row_text(0), "primary");
    // Cursor and pen restored by the paired DECRC.
    assert_eq!(t.cursor_pos(), (0, 7));
    assert_eq!(t.screen().cursor.pen.fg, Color::Indexed(1));
}

#[test]
fn alt_screen_1047_clears_on_exit() {
    let mut t = term(10, 3);
    t.feed(b"\x1b[?1047halt\x1b[?1047l\x1b[?1047h");
    assert!(t.is_alt_screen());
    assert_eq!(t.row_text(0), "");
}

#[test]
fn alt_screen_has_no_scrollback() {
    let mut t = term(4, 2);
    t.feed(b"\x1b[?1049h");
    t.feed(b"a\r\nb\r\nc\r\nd");
    assert_eq!(t.grid().scrollback().len(), 0);
}

#[test]
fn sgr_named_256_truecolor() {
    let mut t = term(20, 2);
    t.feed(b"\x1b[31;44ma");
    assert_eq!(cell(&t, 0, 0).fg, Color::Indexed(1));
    assert_eq!(cell(&t, 0, 0).bg, Color::Indexed(4));
    t.feed(b"\x1b[38;5;208mb");
    assert_eq!(cell(&t, 0, 1).fg, Color::Indexed(208));
    t.feed(b"\x1b[38;2;10;20;30mc");
    assert_eq!(cell(&t, 0, 2).fg, Color::Rgb(10, 20, 30));
    t.feed(b"\x1b[48:2:1:2:3md");
    assert_eq!(cell(&t, 0, 3).bg, Color::Rgb(1, 2, 3));
    t.feed(b"\x1b[0me");
    assert_eq!(cell(&t, 0, 4).fg, Color::Default);
    assert_eq!(cell(&t, 0, 4).bg, Color::Default);
}

#[test]
fn sgr_attributes_render_into_cells() {
    let mut t = term(10, 1);
    t.feed(b"\x1b[1;3;4ma\x1b[ms");
    let c = cell(&t, 0, 0);
    assert!(c.flags.contains(CellFlags::BOLD));
    assert!(c.flags.contains(CellFlags::ITALIC));
    assert!(c.flags.contains(CellFlags::UNDERLINE));
    assert!(cell(&t, 0, 1).flags.is_empty());
}

#[test]
fn decom_with_decstbm() {
    let mut t = term(10, 6);
    // Region rows 3-5, origin mode on: CUP 1;1 lands on grid row 2.
    t.feed(b"\x1b[3;5r\x1b[?6h\x1b[1;1HX");
    assert_eq!(cell(&t, 2, 0).ch, 'X');
    // Cursor cannot leave the region while origin mode is on.
    t.feed(b"\x1b[99;1HY");
    assert_eq!(cell(&t, 4, 0).ch, 'Y');
    // CPR reports region-relative coordinates.
    t.feed(b"\x1b[1;1H\x1b[6n");
    assert_eq!(t.take_output(), b"\x1b[1;1R");
    // Origin off: absolute again.
    t.feed(b"\x1b[?6l\x1b[1;1H\x1b[6n");
    assert_eq!(t.take_output(), b"\x1b[1;1R");
}

#[test]
fn tab_stops() {
    let mut t = term(24, 2);
    t.feed(b"\tx");
    assert_eq!(t.cell(0, 8).ch, 'x');
    t.feed(b"\ty");
    assert_eq!(t.cell(0, 16).ch, 'y');
    // CHT and CBT.
    let mut t = term(24, 2);
    t.feed(b"\x1b[2Ix");
    assert_eq!(t.cell(0, 16).ch, 'x');
    t.feed(b"\x1b[2Zy");
    assert_eq!(t.cell(0, 8).ch, 'y');
    // TBC 0 clears one stop; TBC 3 clears all.
    let mut t = term(24, 2);
    t.feed(b"\x1b[9G\x1b[g\r\tx");
    assert_eq!(t.cell(0, 16).ch, 'x');
    let mut t = term(24, 2);
    t.feed(b"\x1b[3g\tx");
    assert_eq!(t.cell(0, 23).ch, 'x');
}

#[test]
fn decsc_decrc_roundtrip() {
    let mut t = term(10, 4);
    t.feed(b"\x1b[2;3H\x1b[35m\x1b7");
    t.feed(b"\x1b[H\x1b[0m");
    assert_eq!(t.cursor_pos(), (0, 0));
    t.feed(b"\x1b8");
    assert_eq!(t.cursor_pos(), (1, 2));
    assert_eq!(t.screen().cursor.pen.fg, Color::Indexed(5));
}

#[test]
fn csi_s_u_save_restore() {
    let mut t = term(10, 4);
    t.feed(b"\x1b[3;4H\x1b[s\x1b[H\x1b[u");
    assert_eq!(t.cursor_pos(), (2, 3));
}

#[test]
fn ich_dch() {
    let mut t = term(8, 1);
    t.feed(b"abcdef\x1b[1;2H\x1b[2@");
    assert_eq!(t.row_text(0), "a  bcdef");
    t.feed(b"\x1b[2P");
    assert_eq!(t.row_text(0), "abcdef");
}

#[test]
fn il_dl_within_region() {
    let mut t = term(4, 4);
    t.feed(b"a\r\nb\r\nc\r\nd");
    t.feed(b"\x1b[2;1H\x1b[1L");
    assert_eq!(screen_text(&mut t), vec!["a", "", "b", "c"]);
    t.feed(b"\x1b[2;1H\x1b[1M");
    assert_eq!(screen_text(&mut t), vec!["a", "b", "c", ""]);
}

#[test]
fn dl_outside_region_is_ignored() {
    let mut t = term(4, 4);
    t.feed(b"a\r\nb\r\nc\r\nd\x1b[3;4r\x1b[1;1H\x1b[1M");
    assert_eq!(screen_text(&mut t), vec!["a", "b", "c", "d"]);
}

#[test]
fn ech_blanks_without_shifting() {
    let mut t = term(8, 1);
    t.feed(b"abcdef\x1b[1;2H\x1b[3X");
    assert_eq!(t.row_text(0), "a   ef");
}

#[test]
fn scrollback_accumulates_and_ed3_clears() {
    let mut t = term(4, 2);
    t.feed(b"a\r\nb\r\nc\r\nd");
    assert_eq!(t.grid().scrollback().len(), 2);
    assert_eq!(t.grid_mut().scrollback_mut().row(0).unwrap().text(), "a");
    t.feed(b"\x1b[3J");
    assert_eq!(t.grid().scrollback().len(), 0);
}

#[test]
fn dsr6_reports_position() {
    let mut t = term(10, 5);
    t.feed(b"\x1b[4;6H\x1b[6n");
    assert_eq!(t.take_output(), b"\x1b[4;6R");
    assert!(t.take_output().is_empty());
}

#[test]
fn osc_title_bel_and_st() {
    let mut t = term(10, 2);
    t.feed(b"\x1b]0;bel title\x07");
    assert_eq!(t.title(), "bel title");
    t.feed(b"\x1b]2;st title\x1b\\");
    assert_eq!(t.title(), "st title");
}

#[test]
fn cursor_movement_sequences() {
    let mut t = term(10, 5);
    t.feed(b"\x1b[3;3H\x1b[A");
    assert_eq!(t.cursor_pos(), (1, 2));
    t.feed(b"\x1b[2B");
    assert_eq!(t.cursor_pos(), (3, 2));
    t.feed(b"\x1b[3C");
    assert_eq!(t.cursor_pos(), (3, 5));
    t.feed(b"\x1b[2D");
    assert_eq!(t.cursor_pos(), (3, 3));
    t.feed(b"\x1b[E");
    assert_eq!(t.cursor_pos(), (4, 0));
    t.feed(b"\x1b[2F");
    assert_eq!(t.cursor_pos(), (2, 0));
    t.feed(b"\x1b[7G");
    assert_eq!(t.cursor_pos(), (2, 6));
    t.feed(b"\x1b[2d");
    assert_eq!(t.cursor_pos(), (1, 6));
    t.feed(b"\x1b[2e");
    assert_eq!(t.cursor_pos(), (3, 6));
    t.feed(b"\x1b[2a");
    assert_eq!(t.cursor_pos(), (3, 8));
}

#[test]
fn split_feeds_parse_identically() {
    let mut a = term(10, 3);
    let mut b = term(10, 3);
    let bytes: &[u8] = b"\x1b[31mhi\x1b[0m \xe6\xbc\xa2";
    a.feed(bytes);
    for &byte in bytes {
        b.feed(&[byte]);
    }
    assert_eq!(screen_text(&mut a), screen_text(&mut b));
    assert_eq!(a.cursor_pos(), b.cursor_pos());
}

#[test]
fn garbage_never_panics() {
    let mut t = term(8, 3);
    t.feed(b"\x1b[;;;m\x1b[999;999H\x1b[?9999h\x1b]\x07\x1bZ\x1b[>c");
    t.feed(&[0xff, 0xfe, 0x1b, b'[', 0x80, b'm']);
    t.feed(b"still alive");
    assert!(screen_text(&mut t).join("").contains("still"));
}
