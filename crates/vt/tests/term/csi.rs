use crate::mode::Modes;
use crate::term::Terminal;

#[test]
fn cup_moves_cursor() {
    let mut t = Terminal::new(10, 5, 0);
    t.feed(b"\x1b[3;4H");
    assert_eq!(t.cursor_pos(), (2, 3));
    t.feed(b"\x1b[H");
    assert_eq!(t.cursor_pos(), (0, 0));
}

#[test]
fn private_modes_toggle() {
    let mut t = Terminal::new(10, 5, 0);
    t.feed(b"\x1b[?25l");
    assert!(!t.cursor_visible());
    t.feed(b"\x1b[?25h");
    assert!(t.cursor_visible());
    t.feed(b"\x1b[?2004h");
    assert!(t.modes().contains(Modes::BRACKETED_PASTE));
    t.feed(b"\x1b[?1000h\x1b[?1006h");
    assert!(t.modes().contains(Modes::MOUSE_CLICK));
    assert!(t.modes().contains(Modes::MOUSE_SGR));
    t.feed(b"\x1b[?1000l");
    assert!(!t.modes().contains(Modes::MOUSE_CLICK));
}

#[test]
fn insert_mode_shifts_on_print() {
    let mut t = Terminal::new(10, 3, 0);
    t.feed(b"abc\x1b[1G\x1b[4hX");
    assert_eq!(t.row_text(0), "Xabc");
    t.feed(b"\x1b[4l");
    assert!(!t.modes().contains(Modes::INSERT));
}

#[test]
fn dsr_five_reports_ok() {
    let mut t = Terminal::new(10, 3, 0);
    t.feed(b"\x1b[5n");
    assert_eq!(t.take_output(), b"\x1b[0n");
}

#[test]
fn da1_reports_vt220() {
    let mut t = Terminal::new(10, 3, 0);
    t.feed(b"\x1b[c");
    assert_eq!(t.take_output(), b"\x1b[?62;22c");
}

#[test]
fn decscusr_stores_style() {
    use crate::cursor::CursorStyle;
    let mut t = Terminal::new(10, 3, 0);
    t.feed(b"\x1b[4 q");
    assert_eq!(t.cursor_style(), CursorStyle::SteadyUnderline);
    t.feed(b"\x1b[0 q");
    assert_eq!(t.cursor_style(), CursorStyle::BlinkingBlock);
}

#[test]
fn title_stack_push_pop() {
    let mut t = Terminal::new(10, 3, 0);
    t.feed(b"\x1b]2;first\x07\x1b[22;0t\x1b]2;second\x07");
    assert_eq!(t.title(), "second");
    t.feed(b"\x1b[23;0t");
    assert_eq!(t.title(), "first");
}

#[test]
fn rep_repeats_last_char() {
    let mut t = Terminal::new(10, 3, 0);
    t.feed(b"x\x1b[3b");
    assert_eq!(t.row_text(0), "xxxx");
}

#[test]
fn kitty_keyboard_push_pop_set_query() {
    let mut t = Terminal::new(10, 3, 0);
    assert_eq!(t.kitty_keyboard_flags(), 0);
    // Query in legacy mode.
    t.feed(b"\x1b[?u");
    assert_eq!(t.take_output(), b"\x1b[?0u");
    // Push disambiguate.
    t.feed(b"\x1b[>1u");
    assert_eq!(t.kitty_keyboard_flags(), 1);
    t.feed(b"\x1b[?u");
    assert_eq!(t.take_output(), b"\x1b[?1u");
    // Add a bit via set mode 2.
    t.feed(b"\x1b[=2;2u");
    assert_eq!(t.kitty_keyboard_flags(), 3);
    // Pop back to legacy.
    t.feed(b"\x1b[<1u");
    assert_eq!(t.kitty_keyboard_flags(), 0);
}

#[test]
fn kitty_keyboard_is_per_screen() {
    let mut t = Terminal::new(10, 3, 0);
    t.feed(b"\x1b[>5u"); // main screen
    assert_eq!(t.kitty_keyboard_flags(), 5);
    t.feed(b"\x1b[?1049h"); // enter alt: its own (empty) stack
    assert_eq!(t.kitty_keyboard_flags(), 0);
    t.feed(b"\x1b[?1049l"); // back to main
    assert_eq!(t.kitty_keyboard_flags(), 5);
}

#[test]
fn kitty_keyboard_reset_by_ris() {
    let mut t = Terminal::new(10, 3, 0);
    t.feed(b"\x1b[>9u\x1bc");
    assert_eq!(t.kitty_keyboard_flags(), 0);
}

#[test]
fn unknown_csi_is_ignored() {
    let mut t = Terminal::new(10, 3, 0);
    t.feed(b"\x1b[999z\x1b[?9999hok");
    assert_eq!(t.row_text(0), "ok");
}
