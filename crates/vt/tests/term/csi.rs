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
fn da1_reports_vt220_with_sixel() {
  let mut t = Terminal::new(10, 3, 0);
  t.feed(b"\x1b[c");
  assert_eq!(t.take_output(), b"\x1b[?62;4;22c");
}

#[test]
fn xtsmgraphics_reports_color_registers() {
  let mut t = Terminal::new(10, 3, 0);
  t.feed(b"\x1b[?1;1S");
  assert_eq!(t.take_output(), b"\x1b[?1;0;256S");
  t.feed(b"\x1b[?1;4S");
  assert_eq!(t.take_output(), b"\x1b[?1;0;256S");
  // Action out of range: status 2.
  t.feed(b"\x1b[?1;5S");
  assert_eq!(t.take_output(), b"\x1b[?1;2S");
}

#[test]
fn xtsmgraphics_reports_sixel_geometry() {
  let mut t = Terminal::new(10, 3, 0);
  t.set_cell_pixels(8, 16);
  t.feed(b"\x1b[?2;1S");
  assert_eq!(t.take_output(), b"\x1b[?2;0;80;48S");
  t.feed(b"\x1b[?2;4S");
  assert_eq!(t.take_output(), b"\x1b[?2;0;80;48S");
  // Geometry is not settable: status 3.
  t.feed(b"\x1b[?2;3;100;100S");
  assert_eq!(t.take_output(), b"\x1b[?2;3S");
  // ReGIS (or anything else) is not an item we know: status 1.
  t.feed(b"\x1b[?3;1S");
  assert_eq!(t.take_output(), b"\x1b[?3;1S");
}

#[test]
fn xtwinops_reports_sizes() {
  let mut t = Terminal::new(10, 3, 0);
  t.set_cell_pixels(8, 16);
  t.feed(b"\x1b[14t");
  assert_eq!(t.take_output(), b"\x1b[4;48;80t");
  t.feed(b"\x1b[16t");
  assert_eq!(t.take_output(), b"\x1b[6;16;8t");
  t.feed(b"\x1b[18t");
  assert_eq!(t.take_output(), b"\x1b[8;3;10t");
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

#[test]
fn xtwinops_reports_screen_size_in_chars() {
  let mut t = Terminal::new(10, 3, 0);
  t.feed(b"\x1b[19t");
  assert_eq!(t.take_output(), b"\x1b[9;3;10t");
}

#[test]
fn decrqm_reports_private_modes() {
  let mut t = Terminal::new(10, 3, 0);
  t.feed(b"\x1b[?2026$p");
  assert_eq!(t.take_output(), b"\x1b[?2026;2$y");
  t.feed(b"\x1b[?2026h\x1b[?2026$p");
  assert_eq!(t.take_output(), b"\x1b[?2026;1$y");
  t.feed(b"\x1b[?7$p");
  assert_eq!(t.take_output(), b"\x1b[?7;1$y", "autowrap is on by default");
  t.feed(b"\x1b[?1049h\x1b[?1049$p");
  assert_eq!(t.take_output(), b"\x1b[?1049;1$y");
  t.feed(b"\x1b[?1015$p");
  assert_eq!(t.take_output(), b"\x1b[?1015;4$y");
  t.feed(b"\x1b[?31337$p");
  assert_eq!(t.take_output(), b"\x1b[?31337;0$y");
}

#[test]
fn decrqm_reports_ansi_modes() {
  let mut t = Terminal::new(10, 3, 0);
  t.feed(b"\x1b[4$p");
  assert_eq!(t.take_output(), b"\x1b[4;2$y");
  t.feed(b"\x1b[4h\x1b[4$p");
  assert_eq!(t.take_output(), b"\x1b[4;1$y");
  t.feed(b"\x1b[20$p");
  assert_eq!(t.take_output(), b"\x1b[20;4$y");
  t.feed(b"\x1b[99$p");
  assert_eq!(t.take_output(), b"\x1b[99;0$y");
}

#[test]
fn decxcpr_reports_cursor_with_page() {
  let mut t = Terminal::new(10, 5, 0);
  t.feed(b"\x1b[3;4H\x1b[?6n");
  assert_eq!(t.take_output(), b"\x1b[?3;4;1R");
}

#[test]
fn decbkm_toggles_and_reports() {
  let mut t = Terminal::new(10, 3, 0);
  assert!(!t.backspace_sends_bs());
  t.feed(b"\x1b[?67h");
  assert!(t.backspace_sends_bs());
  t.feed(b"\x1b[?67$p");
  assert_eq!(t.take_output(), b"\x1b[?67;1$y");
}

#[test]
fn decstr_soft_resets_modes_but_keeps_the_screen() {
  let mut t = Terminal::new(10, 5, 0);
  t.feed(b"hello\x1b[2;4r\x1b[?6h\x1b[4h\x1b[?25l\x1b[?7l\x1b[?1h\x1b[1;31m\x1b7");
  t.feed(b"\x1b[!p");
  // Modes back to their defaults.
  t.feed(b"\x1b[?6$p\x1b[4$p\x1b[?25$p\x1b[?7$p\x1b[?1$p");
  assert_eq!(
    t.take_output(),
    b"\x1b[?6;2$y\x1b[4;2$y\x1b[?25;1$y\x1b[?7;1$y\x1b[?1;2$y"
  );
  // Margins are full and origin mode is off: row 5 is reachable and absolute.
  t.feed(b"\x1b[5;5H\x1b[6n");
  assert_eq!(t.take_output(), b"\x1b[5;5R");
  // The text survives, and the pen is plain.
  assert!(t.row_text(0).starts_with("hello"));
  t.feed(b"x");
  let cell = t.cell(4, 4);
  assert_eq!(cell.ch, 'x');
  assert_eq!(cell.fg, Default::default());
  assert!(cell.flags.is_empty());
}

#[test]
fn risky_sequences_are_inert_by_default() {
  // Gated off unless the user opts in (Ghostty 1.4 gates the same two).
  let mut t = Terminal::new(10, 3, 0);
  // XTWINOPS 8: a program must not resize the user's window by default.
  t.feed(b"\x1b[8;50;200t");
  assert_eq!(t.take_resize_request(), None);
  assert_eq!((t.cols(), t.rows()), (10, 3));
  // DECRQCRA: a checksum of screen contents is an oracle for text the
  // program did not write (e.g. a password echoed by another process).
  t.feed(b"secret\x1b[1;1;1;1;3;10*y");
  // XTCHECKSUM (CSI # y) only picks a checksum flavour; it stays inert.
  t.feed(b"\x1b[0#y");
  assert!(t.take_output().is_empty());
}

#[test]
fn window_resize_requests_when_allowed() {
  let mut t = Terminal::new(10, 3, 0);
  t.set_window_resize_allowed(true);
  t.feed(b"\x1b[8;50;200t");
  assert_eq!(t.take_resize_request(), Some((50, 200)));
  // 0 keeps a dimension; absurd sizes are clamped.
  t.feed(b"\x1b[8;0;65000t");
  assert_eq!(t.take_resize_request(), Some((3, 1000)));
  // The grid itself only changes when the host resizes it.
  assert_eq!((t.cols(), t.rows()), (10, 3));
}

#[test]
fn decrqcra_checksums_a_rectangle_when_allowed() {
  let mut t = Terminal::new(10, 3, 0);
  t.set_checksum_report(true);
  t.feed(b"AB");
  // Row 1, columns 1..2: 'A' + 'B' = 0x83, negated = 0xFF7D.
  t.feed(b"\x1b[7;1;1;1;1;2*y");
  assert_eq!(t.take_output(), b"\x1bP7!~FF7D\x1b\\");
  // Blanks count as spaces: one blank cell is 0x20 -> 0xFFE0.
  t.feed(b"\x1b[1;1;3;5;3;5*y");
  assert_eq!(t.take_output(), b"\x1bP1!~FFE0\x1b\\");
}
