use super::*;

#[test]
fn construction_defaults() {
    let t = Terminal::new(80, 24, 100);
    assert_eq!(t.cols(), 80);
    assert_eq!(t.rows(), 24);
    assert!(!t.is_alt_screen());
    assert!(t.cursor_visible());
    assert_eq!(t.cursor_pos(), (0, 0));
    assert_eq!(t.title(), "");
    assert_eq!(t.cursor_style(), CursorStyle::BlinkingBlock);
}

#[test]
fn feed_prints_text() {
    let mut t = Terminal::new(10, 3, 0);
    t.feed(b"hi");
    assert_eq!(t.row_text(0), "hi");
    assert_eq!(t.cursor_pos(), (0, 2));
}

#[test]
fn visible_rows_with_offset() {
    let mut t = Terminal::new(4, 2, 10);
    t.feed(b"a\r\nb\r\nc\r\nd");
    // "a" and "b" scrolled into history.
    assert_eq!(t.row_text(0), "c");
    t.set_display_offset(2);
    assert_eq!(t.row_text(0), "a");
    assert_eq!(t.row_text(1), "b");
    t.set_display_offset(99);
    assert_eq!(t.display_offset(), 2);
}

#[test]
fn display_offset_stays_stable_as_output_arrives() {
    let mut t = Terminal::new(4, 2, 10);
    t.feed(b"a\r\nb\r\nc\r\nd"); // scrollback: a, b
    t.set_display_offset(2);
    assert_eq!(t.row_text(0), "a");
    t.feed(b"\r\ne"); // "c" scrolls into history
    assert_eq!(t.display_offset(), 3);
    assert_eq!(t.row_text(0), "a"); // view did not shift
    assert_eq!(t.row_text(1), "b");
}

#[test]
fn display_offset_clamps_when_ring_evicts() {
    let mut t = Terminal::new(4, 2, 2);
    t.feed(b"a\r\nb\r\nc\r\nd"); // ring full: a, b
    t.set_display_offset(2);
    t.feed(b"\r\ne"); // pushes "c", evicts "a"
    assert_eq!(t.display_offset(), 2); // clamped to ring length
    assert_eq!(t.row_text(0), "b");
}

#[test]
fn display_offset_untouched_at_bottom() {
    let mut t = Terminal::new(4, 2, 10);
    t.feed(b"a\r\nb\r\nc");
    assert_eq!(t.display_offset(), 0);
    t.feed(b"\r\nd"); // more history, still at the live bottom
    assert_eq!(t.display_offset(), 0);
}

#[test]
fn scroll_display_deltas_clamp() {
    let mut t = Terminal::new(4, 2, 10);
    t.feed(b"a\r\nb\r\nc\r\nd"); // scrollback len 2
    t.scroll_display(99);
    assert_eq!(t.display_offset(), 2);
    t.scroll_display(-1);
    assert_eq!(t.display_offset(), 1);
    t.scroll_display(-99);
    assert_eq!(t.display_offset(), 0);
}

#[test]
fn entering_alt_resets_display_offset() {
    let mut t = Terminal::new(4, 2, 10);
    t.feed(b"a\r\nb\r\nc\r\nd");
    t.set_display_offset(2);
    t.feed(b"\x1b[?1049h");
    assert_eq!(t.display_offset(), 0);
    // No scrollback on alt: scrolling is a no-op.
    t.scroll_display(5);
    assert_eq!(t.display_offset(), 0);
    // vt does not restore the offset on exit; the app decides.
    t.feed(b"\x1b[?1049l");
    assert_eq!(t.display_offset(), 0);
}

#[test]
fn mouse_mode_tracks_decset() {
    use crate::mode::MouseMode;
    let mut t = Terminal::new(10, 3, 0);
    assert_eq!(t.mouse_mode(), MouseMode::None);
    assert!(!t.mouse_sgr());
    t.feed(b"\x1b[?1000h");
    assert_eq!(t.mouse_mode(), MouseMode::Click);
    t.feed(b"\x1b[?1002h");
    assert_eq!(t.mouse_mode(), MouseMode::Drag);
    t.feed(b"\x1b[?1003h");
    assert_eq!(t.mouse_mode(), MouseMode::Motion);
    t.feed(b"\x1b[?1006h");
    assert!(t.mouse_sgr());
    t.feed(b"\x1b[?1003l");
    assert_eq!(t.mouse_mode(), MouseMode::Drag);
    t.feed(b"\x1b[?1002l\x1b[?1000l\x1b[?1006l");
    assert_eq!(t.mouse_mode(), MouseMode::None);
    assert!(!t.mouse_sgr());
}

#[test]
fn alternate_scroll_defaults_on_and_tracks_1007() {
    let mut t = Terminal::new(10, 3, 0);
    assert!(t.alternate_scroll()); // default on, like alacritty/kitty/iTerm2
    t.feed(b"\x1b[?1007l");
    assert!(!t.alternate_scroll());
    t.feed(b"\x1b[?1007h");
    assert!(t.alternate_scroll());
    // RIS returns to the default.
    t.feed(b"\x1b[?1007l\x1bc");
    assert!(t.alternate_scroll());
}

#[test]
fn resize_clamps_display_offset() {
    let mut t = Terminal::new(4, 2, 10);
    t.feed(b"a\r\nb\r\nc");
    t.set_display_offset(1);
    t.resize(6, 4);
    assert!(t.display_offset() <= t.grid().scrollback().len());
    assert_eq!(t.cols(), 6);
    assert_eq!(t.rows(), 4);
}

#[test]
fn fresh_terminal_is_fully_damaged() {
    let mut t = Terminal::new(10, 3, 0);
    assert_eq!(t.take_damage(), Damage::Full);
    assert_eq!(t.take_damage(), Damage::Rows(vec![]));
}

#[test]
fn printing_marks_row_dirty_and_take_clears() {
    let mut t = Terminal::new(10, 3, 0);
    t.take_damage();
    t.feed(b"hi");
    assert_eq!(t.take_damage(), Damage::Rows(vec![0]));
    assert_eq!(t.take_damage(), Damage::Rows(vec![]));
    t.feed(b"\x1b[3;1Hx");
    assert_eq!(t.take_damage(), Damage::Rows(vec![2]));
}

#[test]
fn scroll_escalates_to_full_damage() {
    let mut t = Terminal::new(4, 2, 10);
    t.take_damage();
    t.feed(b"a\r\nb\r\nc"); // last linefeed scrolls
    assert_eq!(t.take_damage(), Damage::Full);
}

#[test]
fn resize_escalates_to_full_damage() {
    let mut t = Terminal::new(10, 3, 0);
    t.take_damage();
    t.resize(8, 4);
    assert_eq!(t.take_damage(), Damage::Full);
}

#[test]
fn alt_switch_escalates_to_full_damage() {
    let mut t = Terminal::new(10, 3, 0);
    t.take_damage();
    t.feed(b"\x1b[?1049h");
    assert_eq!(t.take_damage(), Damage::Full);
    t.feed(b"\x1b[?1049l");
    assert_eq!(t.take_damage(), Damage::Full);
    // Leaving alt while already on primary changes nothing.
    t.feed(b"\x1b[?1049l");
    assert_eq!(t.take_damage(), Damage::Rows(vec![]));
}

#[test]
fn ris_and_ed_escalate_to_full_damage() {
    let mut t = Terminal::new(10, 3, 0);
    t.take_damage();
    t.feed(b"\x1bc");
    assert_eq!(t.take_damage(), Damage::Full);
    t.feed(b"\x1b[2J");
    assert_eq!(t.take_damage(), Damage::Full);
    t.feed(b"\x1b[3J");
    assert_eq!(t.take_damage(), Damage::Full);
}

#[test]
fn palette_osc_escalates_to_full_damage() {
    let mut t = Terminal::new(10, 3, 0);
    t.take_damage();
    t.feed(b"\x1b]4;1;rgb:ff/00/00\x07");
    assert_eq!(t.take_damage(), Damage::Full);
    t.feed(b"\x1b]104\x07");
    assert_eq!(t.take_damage(), Damage::Full);
}

#[test]
fn display_offset_change_escalates_to_full_damage() {
    let mut t = Terminal::new(4, 2, 10);
    t.feed(b"a\r\nb\r\nc");
    t.take_damage();
    t.set_display_offset(1);
    assert_eq!(t.take_damage(), Damage::Full);
    // Setting the same offset again is not damage.
    t.set_display_offset(1);
    assert_eq!(t.take_damage(), Damage::Rows(vec![]));
}

#[test]
fn full_damage_clears_stale_row_damage() {
    let mut t = Terminal::new(10, 3, 0);
    t.take_damage();
    t.feed(b"hi");
    t.resize(8, 4);
    assert_eq!(t.take_damage(), Damage::Full);
    assert_eq!(t.take_damage(), Damage::Rows(vec![]));
}

#[test]
fn bell_take_and_clear() {
    let mut t = Terminal::new(10, 3, 0);
    assert!(!t.take_bell());
    t.feed(b"\x07");
    assert!(t.take_bell());
    assert!(!t.take_bell());
}

#[test]
fn title_change_signal() {
    let mut t = Terminal::new(10, 3, 0);
    assert_eq!(t.take_title_changed(), None);
    t.feed(b"\x1b]2;hello\x07");
    assert_eq!(t.take_title_changed(), Some("hello".to_string()));
    assert_eq!(t.take_title_changed(), None);
    assert_eq!(t.title(), "hello");
    t.feed(b"\x1b]0;again\x07");
    assert_eq!(t.take_title_changed(), Some("again".to_string()));
}

#[test]
fn cursor_keys_app_tracks_decckm() {
    let mut t = Terminal::new(10, 3, 0);
    assert!(!t.cursor_keys_app());
    t.feed(b"\x1b[?1h");
    assert!(t.cursor_keys_app());
    t.feed(b"\x1b[?1l");
    assert!(!t.cursor_keys_app());
    // RIS clears it.
    t.feed(b"\x1b[?1h\x1bc");
    assert!(!t.cursor_keys_app());
}

#[test]
fn keypad_app_tracks_deckpam_deckpnm_and_ris() {
    let mut t = Terminal::new(10, 3, 0);
    assert!(!t.keypad_app());
    t.feed(b"\x1b=");
    assert!(t.keypad_app());
    t.feed(b"\x1b>");
    assert!(!t.keypad_app());
    t.feed(b"\x1b=\x1bc");
    assert!(!t.keypad_app());
}

#[test]
fn bracketed_paste_accessor() {
    let mut t = Terminal::new(10, 3, 0);
    assert!(!t.bracketed_paste());
    t.feed(b"\x1b[?2004h");
    assert!(t.bracketed_paste());
    t.feed(b"\x1b[?2004l");
    assert!(!t.bracketed_paste());
    t.feed(b"\x1b[?2004h\x1bc");
    assert!(!t.bracketed_paste());
}

#[test]
fn focus_reporting_emits_only_when_enabled() {
    let mut t = Terminal::new(10, 3, 0);
    // Off by default: report_focus is a no-op.
    assert!(!t.focus_reporting());
    t.report_focus(true);
    assert!(t.take_output().is_empty());
    // Enable ?1004 and focus in/out emit CSI I / CSI O.
    t.feed(b"\x1b[?1004h");
    assert!(t.focus_reporting());
    t.report_focus(true);
    assert_eq!(t.take_output(), b"\x1b[I");
    t.report_focus(false);
    assert_eq!(t.take_output(), b"\x1b[O");
    t.feed(b"\x1b[?1004l");
    t.report_focus(true);
    assert!(t.take_output().is_empty());
}

#[test]
fn synchronized_output_tracks_2026() {
    let mut t = Terminal::new(10, 3, 0);
    assert!(!t.synchronized_output());
    t.feed(b"\x1b[?2026h");
    assert!(t.synchronized_output());
    t.feed(b"\x1b[?2026l");
    assert!(!t.synchronized_output());
}

#[test]
fn search_finds_matches_across_scrollback() {
    let mut t = Terminal::new(10, 2, 10);
    t.feed(b"foo bar\r\nbaz foo\r\nqux"); // "foo" on lines 0 and 1
    let hits = t.search("foo", false);
    assert_eq!(hits.len(), 2);
    // First match is the oldest (lowest global line), col 0.
    assert_eq!((hits[0].start_col, hits[0].end_col), (0, 2));
    // Case-insensitive by default; case-sensitive can differ.
    t.feed(b"\r\nFOO");
    assert_eq!(t.search("foo", false).len(), 3);
    assert_eq!(t.search("foo", true).len(), 2);
}

#[test]
fn da2_reports_secondary_attributes() {
    let mut t = Terminal::new(10, 3, 0);
    t.feed(b"\x1b[>c");
    assert_eq!(t.take_output(), b"\x1b[>0;276;0c");
}

#[test]
fn title_stack_pop_signals_change() {
    let mut t = Terminal::new(10, 3, 0);
    t.feed(b"\x1b]2;first\x07\x1b[22;0t\x1b]2;second\x07");
    t.take_title_changed();
    t.feed(b"\x1b[23;0t");
    assert_eq!(t.take_title_changed(), Some("first".to_string()));
}

#[test]
fn buffer_text_includes_scrollback_and_screen() {
    let mut t = Terminal::new(4, 2, 10);
    t.feed(b"a\r\nb\r\nc\r\nd"); // a, b in scrollback; c, d on screen
    assert_eq!(t.buffer_text(), "a\nb\nc\nd\n");
}

#[test]
fn buffer_text_drops_trailing_blank_rows() {
    let mut t = Terminal::new(4, 3, 10);
    t.feed(b"hi"); // one row of content, two blank rows below
    assert_eq!(t.buffer_text(), "hi\n");
}

#[test]
fn buffer_text_empty_when_blank() {
    let mut t = Terminal::new(4, 3, 10);
    assert_eq!(t.buffer_text(), "");
}

#[test]
fn buffer_text_is_independent_of_scroll_offset() {
    let mut t = Terminal::new(4, 2, 10);
    t.feed(b"a\r\nb\r\nc\r\nd");
    t.set_display_offset(2); // scrolled up into history
    assert_eq!(t.buffer_text(), "a\nb\nc\nd\n");
}

#[test]
fn command_finished_reports_exit_code_once() {
    let mut t = Terminal::new(10, 3, 0);
    assert_eq!(t.take_command_finished(), None);
    t.feed(b"\x1b]133;D;2\x07");
    assert_eq!(t.take_command_finished(), Some(Some(2)));
    assert_eq!(t.take_command_finished(), None); // consumed
    t.feed(b"\x1b]133;D\x07"); // no exit code
    assert_eq!(t.take_command_finished(), Some(None));
    t.feed(b"\x1b]133;A\x07"); // prompt-start mark is not a finish
    assert_eq!(t.take_command_finished(), None);
}

#[test]
fn input_mark_shifts_with_scrolling() {
    let mut t = Terminal::new(10, 3, 10);
    t.feed(b"one\r\n\x1b]133;B\x07ls"); // input mark at (1, 0)
    assert_eq!(t.current_input().as_deref(), Some("ls"));
    t.feed(b"\x1b[3;1H\n"); // cursor to the bottom row; LF scrolls once
    assert_eq!(t.current_input().as_deref(), Some("ls")); // mark rode along
    t.feed(b"\n"); // the input row scrolls off the top
    assert_eq!(t.current_input(), None);
}

#[test]
fn input_mark_cleared_on_alt_entry_and_reset() {
    let mut t = Terminal::new(20, 3, 10);
    t.feed(b"\x1b]133;B\x07ls");
    assert!(t.current_input().is_some());
    t.feed(b"\x1b[?1049h");
    assert_eq!(t.current_input(), None);
    let mut t = Terminal::new(20, 3, 10);
    t.feed(b"\x1b]133;B\x07ls\x1bc");
    assert_eq!(t.current_input(), None);
}

#[test]
fn cwd_change_is_reported_once() {
    let mut t = Terminal::new(10, 3, 0);
    t.feed(b"\x1b]7;file:///a\x07");
    assert_eq!(t.take_cwd_changed().as_deref(), Some("file:///a"));
    assert_eq!(t.take_cwd_changed(), None); // nothing new
    t.feed(b"\x1b]7;file:///a\x07"); // same dir, no event
    assert_eq!(t.take_cwd_changed(), None);
    t.feed(b"\x1b]7;file:///b\x07"); // changed
    assert_eq!(t.take_cwd_changed().as_deref(), Some("file:///b"));
}

/// Feed the same synthetic history into two terminals, fully compacting one,
/// so reads over compressed and uncompressed scrollback can be compared.
fn compacted_pair() -> (Terminal, Terminal) {
    let mut plain = Terminal::new(40, 4, 100_000);
    let mut packed = Terminal::new(40, 4, 100_000);
    for n in 0..4000usize {
        let line = match n % 3 {
            0 => format!("foo item {n}\r\n"),
            1 => format!("\x1b[31mred bar {n}\x1b[0m\r\n"),
            _ => format!("path /tmp/thing-{n} 漢字\r\n"),
        };
        plain.feed(line.as_bytes());
        packed.feed(line.as_bytes());
    }
    while packed.compact_scrollback() {}
    (plain, packed)
}

#[test]
fn compact_scrollback_reports_done_and_compresses() {
    let (_, mut packed) = compacted_pair();
    assert!(!packed.compact_scrollback()); // no work left
    let (resident, compressed) = packed.scrollback_memory();
    assert!(compressed > 0);
    // Everything beyond the hot floor left residency.
    assert!(resident < 2000 * 40 * std::mem::size_of::<Cell>());
    let mut small = Terminal::new(40, 4, 100);
    small.feed(b"just a line\r\n");
    assert!(!small.compact_scrollback());
}

#[test]
fn search_matches_are_identical_over_compacted_history() {
    let (mut plain, mut packed) = compacted_pair();
    for (needle, case) in [("foo item", true), ("RED BAR", false), ("漢字", true)] {
        let a = plain.search(needle, case);
        let b = packed.search(needle, case);
        assert!(!a.is_empty(), "{needle}");
        assert_eq!(a, b, "{needle}");
    }
    assert_eq!(plain.buffer_text(), packed.buffer_text());
    assert_eq!(plain.text_lines(), packed.text_lines());
}

#[test]
fn selection_text_is_identical_over_compacted_history() {
    let (mut plain, mut packed) = compacted_pair();
    let sb = plain.grid().scrollback().len() as isize;
    for t in [&mut plain, &mut packed] {
        t.start_selection(
            crate::selection::SelectionMode::Cell,
            crate::selection::Point::new(-sb + 5, 2),
        );
        t.update_selection(crate::selection::Point::new(-sb + 900, 10));
    }
    let a = plain.selection_text();
    let b = packed.selection_text();
    assert!(a.as_deref().is_some_and(|s| s.lines().count() > 800));
    assert_eq!(a, b);
}

#[test]
fn visible_rows_read_back_through_compacted_history() {
    let (mut plain, mut packed) = compacted_pair();
    let sb = plain.grid().scrollback().len();
    for offset in [0, 100, sb / 2, sb] {
        plain.set_display_offset(offset);
        packed.set_display_offset(offset);
        for r in 0..plain.rows() {
            assert_eq!(plain.row_text(r), packed.row_text(r), "offset {offset} row {r}");
        }
    }
}

#[test]
fn resize_after_compaction_reflows_like_uncompacted() {
    let (mut plain, mut packed) = compacted_pair();
    plain.resize(33, 5);
    packed.resize(33, 5);
    assert_eq!(plain.buffer_text(), packed.buffer_text());
    assert_eq!(plain.committed_lines(), packed.committed_lines());
    plain.resize(52, 3);
    packed.resize(52, 3);
    assert_eq!(plain.buffer_text(), packed.buffer_text());
    assert_eq!(plain.committed_lines(), packed.committed_lines());
}
