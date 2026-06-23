use super::*;

fn test_colors() -> Colors {
    colors::from_config(&config::Options::default())
}

#[test]
fn snapshot_batches_plain_text_into_one_span() {
    let mut term = vt::Terminal::new(20, 4, 0);
    term.feed(b"hello world");
    let snap = snapshot(&mut term, &test_colors(), None);
    // Spaces between words break spans (they are skipped).
    assert_eq!(snap.spans.len(), 2);
    assert_eq!(snap.spans[0].text, "hello");
    assert_eq!(snap.spans[0].col, 0);
    assert_eq!(snap.spans[1].text, "world");
    assert_eq!(snap.spans[1].col, 6);
    assert!(snap.bg_runs.is_empty());
}

#[test]
fn snapshot_splits_spans_on_style_change() {
    let mut term = vt::Terminal::new(20, 2, 0);
    term.feed(b"ab\x1b[1mcd\x1b[0mef");
    let snap = snapshot(&mut term, &test_colors(), None);
    let texts: Vec<&str> = snap.spans.iter().map(|s| s.text.as_str()).collect();
    assert_eq!(texts, vec!["ab", "cd", "ef"]);
    assert!(snap.spans[1].flags.contains(CellFlags::BOLD));
}

#[test]
fn snapshot_merges_background_runs() {
    let mut term = vt::Terminal::new(20, 2, 0);
    term.feed(b"\x1b[41mabc\x1b[0m");
    let snap = snapshot(&mut term, &test_colors(), None);
    assert_eq!(snap.bg_runs.len(), 1);
    let run = &snap.bg_runs[0];
    assert_eq!((run.row, run.col, run.len), (0, 0, 3));
    assert_eq!(run.color, theme::default_scheme().ansi[1]);
}

#[test]
fn snapshot_inverse_swaps_colors() {
    let colors = test_colors();
    let mut term = vt::Terminal::new(20, 2, 0);
    term.feed(b"\x1b[7mx\x1b[0m");
    let snap = snapshot(&mut term, &colors, None);
    assert_eq!(snap.spans[0].fg, colors.bg);
    assert_eq!(snap.bg_runs[0].color, colors.fg);
}

#[test]
fn snapshot_skips_invisible_but_keeps_background() {
    let mut term = vt::Terminal::new(20, 2, 0);
    term.feed(b"\x1b[8;41mhid\x1b[0m");
    let snap = snapshot(&mut term, &test_colors(), None);
    assert!(snap.spans.is_empty());
    assert_eq!(snap.bg_runs.len(), 1);
}

#[test]
fn snapshot_wide_char_is_its_own_span() {
    let mut term = vt::Terminal::new(20, 2, 0);
    term.feed("a世b".as_bytes());
    let snap = snapshot(&mut term, &test_colors(), None);
    let texts: Vec<&str> = snap.spans.iter().map(|s| s.text.as_str()).collect();
    assert_eq!(texts, vec!["a", "世", "b"]);
    assert_eq!(snap.spans[1].width, 2);
    // The cell after the wide char sits two columns later.
    assert_eq!(snap.spans[2].col, 3);
}

#[test]
fn snapshot_cursor_follows_visibility() {
    let colors = test_colors();
    let mut term = vt::Terminal::new(20, 2, 0);
    term.feed(b"hi");
    let snap = snapshot(&mut term, &colors, None);
    let cursor = snap.cursor.expect("visible by default");
    assert_eq!((cursor.row, cursor.col), (0, 2));
    assert_eq!(cursor.color, colors.cursor);
    term.feed(b"\x1b[?25l");
    let snap = snapshot(&mut term, &colors, None);
    assert!(snap.cursor.is_none());
}

#[test]
fn snapshot_cursor_honors_osc12_color() {
    let mut term = vt::Terminal::new(20, 2, 0);
    term.feed(b"\x1b]12;rgb:ff/00/00\x07");
    let snap = snapshot(&mut term, &test_colors(), None);
    assert_eq!(snap.cursor.expect("cursor").color, Rgb::new(255, 0, 0));
}

#[test]
fn cursor_shape_mapping() {
    use config::CursorStyle as C;
    use vt::CursorStyle as V;
    // Power-on default defers to config.
    assert_eq!(cursor_shape(V::BlinkingBlock, C::Bar), C::Bar);
    assert_eq!(cursor_shape(V::BlinkingBlock, C::Block), C::Block);
    // Explicit DECSCUSR wins.
    assert_eq!(cursor_shape(V::SteadyBlock, C::Bar), C::Block);
    assert_eq!(cursor_shape(V::BlinkingBar, C::Block), C::Bar);
    assert_eq!(cursor_shape(V::SteadyUnderline, C::Block), C::Underline);
}

#[test]
fn snapshot_underlined_space_is_kept() {
    let mut term = vt::Terminal::new(20, 2, 0);
    term.feed(b"\x1b[4m \x1b[0m");
    let snap = snapshot(&mut term, &test_colors(), None);
    assert_eq!(snap.spans.len(), 1);
    assert!(snap.spans[0].flags.contains(CellFlags::UNDERLINE));
}

#[test]
fn snapshot_selection_overrides_colors() {
    // Default fg must differ from selection fg for the span split.
    let mut opts = config::Options::default();
    opts.foreground = Some("#abb2bf".to_string());
    let colors = colors::from_config(&opts);
    assert_ne!(colors.fg, colors.selection_fg);
    let mut term = vt::Terminal::new(20, 2, 0);
    term.feed(b"hello");
    term.start_selection(vt::SelectionMode::Cell, vt::Point::new(0, 1));
    term.update_selection(vt::Point::new(0, 3));
    let snap = snapshot(&mut term, &colors, None);
    // "h" + "ell" (selected) + "o".
    let texts: Vec<&str> = snap.spans.iter().map(|s| s.text.as_str()).collect();
    assert_eq!(texts, vec!["h", "ell", "o"]);
    assert_eq!(snap.spans[0].fg, colors.fg);
    assert_eq!(snap.spans[1].fg, colors.selection_fg);
    assert_eq!(snap.spans[2].fg, colors.fg);
    // Selected cells paint the selection background.
    assert_eq!(snap.bg_runs.len(), 1);
    let run = &snap.bg_runs[0];
    assert_eq!((run.row, run.col, run.len), (0, 1, 3));
    assert_eq!(run.color, colors.selection_bg);
}

#[test]
fn snapshot_selection_honors_display_offset() {
    let colors = test_colors();
    let mut term = vt::Terminal::new(10, 2, 10);
    term.feed(b"old\r\na\r\nb\r\nc"); // pushes rows into scrollback
                                      // Select the scrollback line holding "old" (line -2).
    term.start_selection(vt::SelectionMode::Cell, vt::Point::new(-2, 0));
    term.update_selection(vt::Point::new(-2, 2));
    // At the live bottom the selected row is off screen: no override.
    let snap = snapshot(&mut term, &colors, None);
    assert!(snap.bg_runs.iter().all(|r| r.color != colors.selection_bg));
    // Scrolled back so the row is visible, the override applies.
    term.scroll_display(2);
    let snap = snapshot(&mut term, &colors, None);
    assert!(snap.bg_runs.iter().any(|r| r.color == colors.selection_bg));
    assert_eq!(snap.offset, 2);
    assert_eq!(snap.scrollback, 2);
}

#[test]
fn scroll_indicator_only_when_scrolled_back() {
    let bounds = Bounds::new(point(px(0.0), px(0.0)), size(px(100.0), px(400.0)));
    assert!(scroll_indicator(&bounds, 24, 0, 100).is_none());
    assert!(scroll_indicator(&bounds, 24, 0, 0).is_none());
    let thumb = scroll_indicator(&bounds, 24, 50, 100).expect("visible");
    // Hugs the right edge at the configured width.
    assert_eq!(thumb.origin.x, px(100.0 - INDICATOR_WIDTH));
    assert_eq!(thumb.size.width, px(INDICATOR_WIDTH));
    assert!(f32::from(thumb.size.height) >= INDICATOR_MIN);
}

#[test]
fn scroll_indicator_tracks_position() {
    let bounds = Bounds::new(point(px(0.0), px(0.0)), size(px(100.0), px(400.0)));
    // Fully scrolled back: thumb at the top.
    let top = scroll_indicator(&bounds, 24, 100, 100).expect("top");
    assert_eq!(top.origin.y, px(0.0));
    // Barely scrolled: thumb near the bottom, still inside bounds.
    let low = scroll_indicator(&bounds, 24, 1, 100).expect("low");
    assert!(low.origin.y > top.origin.y);
    assert!(f32::from(low.origin.y) + f32::from(low.size.height) <= 400.0 + 1e-3);
}
