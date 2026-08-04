use super::*;

fn resize_request(cols: usize) -> ResizeRequest {
    ResizeRequest {
        cols,
        rows: 24,
        cell_width: 8,
        cell_height: 16,
    }
}

#[test]
fn initial_resize_is_immediate_then_drag_resizes_are_coalesced() {
    let mut state = ResizeState::default();
    assert_eq!(
        state.request(resize_request(80), (100, 24)),
        ResizeAction::Immediate
    );
    assert_eq!(
        state.request(resize_request(81), (80, 24)),
        ResizeAction::Arm
    );
    let old_generation = state.generation;
    assert_eq!(
        state.request(resize_request(82), (80, 24)),
        ResizeAction::None
    );
    assert_eq!(state.settle(old_generation), None);
    let latest = state.generation;
    assert_eq!(state.settle(latest), Some(resize_request(82)));
    assert!(!state.armed);
}

#[test]
fn resize_reverted_to_current_size_cancels_pending_target() {
    let mut state = ResizeState::default();
    assert_eq!(
        state.request(resize_request(80), (80, 24)),
        ResizeAction::None
    );
    assert_eq!(
        state.request(resize_request(90), (80, 24)),
        ResizeAction::Arm
    );
    let old_generation = state.generation;
    assert_eq!(
        state.request(resize_request(80), (80, 24)),
        ResizeAction::None
    );
    assert_eq!(state.settle(old_generation), None);
    assert_eq!(state.settle(state.generation), Some(resize_request(80)));
}

#[test]
fn shared_image_cache_enforces_a_global_lru_budget() {
    let pool = Rc::new(RefCell::new(ImageCachePool::new(80)));
    let mut first = ImageCache::new(pool.clone());
    let mut second = ImageCache::new(pool.clone());
    let image = vt::Image {
        width: 4,
        height: 4,
        rgba: vec![0; 64].into(),
    };
    let _first_frame = first.texture(1, &image);
    assert_eq!(pool.borrow().stats().bytes, 64);
    let _second_frame = second.texture(1, &image);
    assert_eq!(
        pool.borrow().stats(),
        ImageCacheStats {
            bytes: 64,
            entries: 1,
            evictions: 1,
        }
    );
}

fn test_colors() -> Colors {
    Colors::from_scheme(theme::default_scheme())
}

/// Call `snapshot` with default cell metrics and a throwaway image cache.
fn take_snapshot(
    term: &mut vt::Terminal,
    colors: &Colors,
    search: Option<&SearchQuery>,
) -> Snapshot {
    let cell = CellSize {
        width: 8.0,
        height: 16.0,
    };
    snapshot(term, colors, search, cell, &mut ImageCache::default(), None)
}

#[test]
fn snapshot_batches_plain_text_into_one_span() {
    let mut term = vt::Terminal::new(20, 4, 0);
    term.feed(b"hello world");
    let snap = take_snapshot(&mut term, &test_colors(), None);
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
    let snap = take_snapshot(&mut term, &test_colors(), None);
    let texts: Vec<&str> = snap.spans.iter().map(|s| s.text.as_str()).collect();
    assert_eq!(texts, vec!["ab", "cd", "ef"]);
    assert!(snap.spans[1].flags.contains(CellFlags::BOLD));
}

#[test]
fn snapshot_merges_background_runs() {
    let mut term = vt::Terminal::new(20, 2, 0);
    term.feed(b"\x1b[41mabc\x1b[0m");
    let snap = take_snapshot(&mut term, &test_colors(), None);
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
    let snap = take_snapshot(&mut term, &colors, None);
    assert_eq!(snap.spans[0].fg, colors.bg);
    assert_eq!(snap.bg_runs[0].color, colors.fg);
}

#[test]
fn snapshot_skips_invisible_but_keeps_background() {
    let mut term = vt::Terminal::new(20, 2, 0);
    term.feed(b"\x1b[8;41mhid\x1b[0m");
    let snap = take_snapshot(&mut term, &test_colors(), None);
    assert!(snap.spans.is_empty());
    assert_eq!(snap.bg_runs.len(), 1);
}

#[test]
fn snapshot_wide_char_is_its_own_span() {
    let mut term = vt::Terminal::new(20, 2, 0);
    term.feed("a世b".as_bytes());
    let snap = take_snapshot(&mut term, &test_colors(), None);
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
    let snap = take_snapshot(&mut term, &colors, None);
    let cursor = snap.cursor.expect("visible by default");
    assert_eq!((cursor.row, cursor.col), (0, 2));
    assert_eq!(cursor.color, colors.cursor);
    term.feed(b"\x1b[?25l");
    let snap = take_snapshot(&mut term, &colors, None);
    assert!(snap.cursor.is_none());
}

#[test]
fn snapshot_cursor_honors_osc12_color() {
    let mut term = vt::Terminal::new(20, 2, 0);
    term.feed(b"\x1b]12;rgb:ff/00/00\x07");
    let snap = take_snapshot(&mut term, &test_colors(), None);
    assert_eq!(snap.cursor.expect("cursor").color, Rgb::new(255, 0, 0));
}

#[test]
fn snapshot_cursor_keeps_contrast_over_painted_background() {
    let colors = test_colors();
    // A full-screen program paints the cell under the cursor the same color
    // as the theme cursor (white on the default dark scheme).
    let mut term = vt::Terminal::new(20, 2, 0);
    term.feed(b"\x1b[48;2;255;255;255m \x1b[0m\x1b[D");
    let snap = take_snapshot(&mut term, &colors, None);
    let cursor = snap.cursor.expect("cursor");
    assert_eq!((cursor.row, cursor.col), (0, 0));
    // The theme cursor would vanish on that cell: it must adjust.
    assert_ne!(cursor.color, colors.cursor);
}

#[test]
fn snapshot_cursor_contrast_sees_inverse_cells() {
    let colors = test_colors();
    // An inverse cell (how TUIs draw their own cursor) puts the theme
    // foreground behind the cursor; the light theme cursor must adjust.
    let mut term = vt::Terminal::new(20, 2, 0);
    term.feed(b"\x1b[7mx\x1b[0m\x1b[D");
    let snap = take_snapshot(&mut term, &colors, None);
    let cursor = snap.cursor.expect("cursor");
    assert_eq!((cursor.row, cursor.col), (0, 0));
    assert_ne!(cursor.color, colors.cursor);
}

#[test]
fn cursor_shape_mapping() {
    use vt::CursorStyle as V;
    use CursorShape as C;
    // Power-on default defers to the host's configured shape.
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
    let snap = take_snapshot(&mut term, &test_colors(), None);
    assert_eq!(snap.spans.len(), 1);
    assert!(snap.spans[0].flags.contains(CellFlags::UNDERLINE));
}

#[test]
fn snapshot_selection_overrides_colors() {
    // Default fg must differ from selection fg for the span split.
    let mut colors = test_colors();
    colors.fg = theme::Rgb::new(0xab, 0xb2, 0xbf);
    assert_ne!(colors.fg, colors.selection_fg);
    let mut term = vt::Terminal::new(20, 2, 0);
    term.feed(b"hello");
    term.start_selection(vt::SelectionMode::Cell, vt::Point::new(0, 1));
    term.update_selection(vt::Point::new(0, 3));
    let snap = take_snapshot(&mut term, &colors, None);
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
    let snap = take_snapshot(&mut term, &colors, None);
    assert!(snap.bg_runs.iter().all(|r| r.color != colors.selection_bg));
    // Scrolled back so the row is visible, the override applies.
    term.scroll_display(2);
    let snap = take_snapshot(&mut term, &colors, None);
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

fn test_cell() -> CellSize {
    CellSize {
        width: 8.0,
        height: 16.0,
    }
}

#[test]
fn undamaged_identical_frames_reuse_the_snapshot() {
    let mut term = vt::Terminal::new(20, 4, 100);
    term.feed(b"hello");
    let colors = Rc::new(test_colors());
    let mut cache = SnapCache::default();
    let mut images = ImageCache::default();
    let a = snapshot_reuse(
        &mut term,
        &mut cache,
        &colors,
        None,
        test_cell(),
        &mut images,
        None,
    );
    let b = snapshot_reuse(
        &mut term,
        &mut cache,
        &colors,
        None,
        test_cell(),
        &mut images,
        None,
    );
    assert!(
        Rc::ptr_eq(&a, &b),
        "no damage + identical inputs must reuse"
    );
    assert_eq!(
        cache.stats(),
        RenderStats {
            frames: 2,
            snapshot_reuses: 1,
            snapshot_rows: 4,
            shaped_rows: 0,
        }
    );
}

#[test]
fn new_output_rebuilds_the_snapshot() {
    let mut term = vt::Terminal::new(20, 4, 100);
    term.feed(b"hello");
    let colors = Rc::new(test_colors());
    let mut cache = SnapCache::default();
    let mut images = ImageCache::default();
    let a = snapshot_reuse(
        &mut term,
        &mut cache,
        &colors,
        None,
        test_cell(),
        &mut images,
        None,
    );
    term.feed(b" world");
    let b = snapshot_reuse(
        &mut term,
        &mut cache,
        &colors,
        None,
        test_cell(),
        &mut images,
        None,
    );
    assert!(!Rc::ptr_eq(&a, &b));
    assert_eq!(b.spans[1].text, "world");
}

#[test]
fn row_damage_rebuilds_only_the_changed_row() {
    let mut term = vt::Terminal::new(20, 4, 100);
    term.feed(b"first\r\nsecond");
    let colors = Rc::new(test_colors());
    let mut cache = SnapCache::default();
    let mut images = ImageCache::default();
    let first = snapshot_reuse(
        &mut term,
        &mut cache,
        &colors,
        None,
        test_cell(),
        &mut images,
        None,
    );
    assert_eq!(cache.last_snapshot_rows(), 4);
    term.feed(b"!");
    let second = snapshot_reuse(
        &mut term,
        &mut cache,
        &colors,
        None,
        test_cell(),
        &mut images,
        None,
    );
    assert_eq!(cache.last_snapshot_rows(), 1);
    assert!(Rc::ptr_eq(&first.rows[0], &second.rows[0]));
    assert!(!Rc::ptr_eq(&first.rows[1], &second.rows[1]));
    assert!(Rc::ptr_eq(&first.rows[2], &second.rows[2]));
    assert!(Rc::ptr_eq(&first.rows[3], &second.rows[3]));
}

#[test]
fn scrolling_reuses_rotated_rows() {
    let mut term = vt::Terminal::new(20, 3, 100);
    term.feed(b"first\r\nsecond\r\nthird");
    let colors = Rc::new(test_colors());
    let mut cache = SnapCache::default();
    let mut images = ImageCache::default();
    let first = snapshot_reuse(
        &mut term,
        &mut cache,
        &colors,
        None,
        test_cell(),
        &mut images,
        None,
    );
    term.feed(b"\r\nfourth");
    let second = snapshot_reuse(
        &mut term,
        &mut cache,
        &colors,
        None,
        test_cell(),
        &mut images,
        None,
    );
    assert_eq!(cache.last_snapshot_rows(), 1);
    assert!(Rc::ptr_eq(&first.rows[1], &second.rows[0]));
    assert!(Rc::ptr_eq(&first.rows[2], &second.rows[1]));
    assert!(!Rc::ptr_eq(&first.rows[0], &second.rows[2]));
}

#[test]
fn cursor_motion_alone_rebuilds_the_snapshot() {
    let mut term = vt::Terminal::new(20, 4, 0);
    term.feed(b"hi");
    let colors = Rc::new(test_colors());
    let mut cache = SnapCache::default();
    let mut images = ImageCache::default();
    let a = snapshot_reuse(
        &mut term,
        &mut cache,
        &colors,
        None,
        test_cell(),
        &mut images,
        None,
    );
    // CUP repositions the cursor without printing a cell.
    term.feed(b"\x1b[2;5H");
    let b = snapshot_reuse(
        &mut term,
        &mut cache,
        &colors,
        None,
        test_cell(),
        &mut images,
        None,
    );
    assert!(!Rc::ptr_eq(&a, &b));
    assert_ne!(a.cursor, b.cursor);
}

#[test]
fn hover_change_rebuilds_without_damage() {
    let mut term = vt::Terminal::new(20, 4, 0);
    term.feed(b"https://example.com");
    let colors = Rc::new(test_colors());
    let mut cache = SnapCache::default();
    let mut images = ImageCache::default();
    let a = snapshot_reuse(
        &mut term,
        &mut cache,
        &colors,
        None,
        test_cell(),
        &mut images,
        None,
    );
    let b = snapshot_reuse(
        &mut term,
        &mut cache,
        &colors,
        None,
        test_cell(),
        &mut images,
        Some((0, 0, 18)),
    );
    assert!(!Rc::ptr_eq(&a, &b), "hover underline is a snapshot input");
}

#[test]
fn snapkeys_compare_search_by_identity_and_colors_by_pointer() {
    let term = vt::Terminal::new(10, 3, 10);
    let colors = Rc::new(test_colors());
    let matches = Rc::new(Vec::new());
    let sq = SearchQuery {
        query: "x".to_string(),
        current: 0,
        matches: matches.clone(),
    };
    let a = snapkey(&term, &colors, Some(&sq), test_cell(), None);
    let b = snapkey(&term, &colors, Some(&sq), test_cell(), None);
    assert!(keyeq(&a, &b));
    // Same query text but a fresh match list: a rescan replaced the Rc.
    let sq2 = SearchQuery {
        query: "x".to_string(),
        current: 0,
        matches: Rc::new(Vec::new()),
    };
    assert!(!keyeq(
        &a,
        &snapkey(&term, &colors, Some(&sq2), test_cell(), None)
    ));
    // Focused-match step.
    let sq3 = SearchQuery {
        query: "x".to_string(),
        current: 1,
        matches,
    };
    assert!(!keyeq(
        &a,
        &snapkey(&term, &colors, Some(&sq3), test_cell(), None)
    ));
    // Closing the overlay.
    assert!(!keyeq(
        &a,
        &snapkey(&term, &colors, None, test_cell(), None)
    ));
    // A theme reload swaps the colors Rc even when values are identical.
    let colors2 = Rc::new(test_colors());
    assert!(!keyeq(
        &a,
        &snapkey(&term, &colors2, Some(&sq), test_cell(), None)
    ));
    // Hover link.
    assert!(!keyeq(
        &a,
        &snapkey(&term, &colors, Some(&sq), test_cell(), Some((0, 0, 2)))
    ));
    // Cell metrics (font size change).
    let big = CellSize {
        width: 10.0,
        height: 20.0,
    };
    assert!(!keyeq(&a, &snapkey(&term, &colors, Some(&sq), big, None)));
}
