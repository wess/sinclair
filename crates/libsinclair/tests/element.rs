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
    assert_eq!((run.col, run.len), (0, 3));
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
    assert_eq!((run.col, run.len), (1, 3));
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

/// The text of each viewport slot, which is what that slot paints.
fn slot_text(snap: &Snapshot) -> Vec<String> {
    snap.rows
        .iter()
        .map(|row| {
            row.spans
                .iter()
                .map(|s| s.text.as_str())
                .collect::<String>()
        })
        .collect()
}

/// Scrolling re-homes row objects into new viewport slots, so a row's
/// position must come from its slot and nothing else. Rows used to carry the
/// index they were first built at, which a scroll made stale: a reused row
/// kept painting at its original y and collided with the row that legitimately
/// owned that slot, compositing two lines of text on top of each other.
#[test]
fn scrolled_reuse_paints_rows_at_their_new_slots() {
    let colors = Rc::new(test_colors());
    let mut cache = SnapCache::default();
    let mut images = ImageCache::default();
    let mut term = vt::Terminal::new(20, 4, 100);

    term.feed(b"r0\r\nr1\r\nr2\r\nr3");
    let first = snapshot_reuse(
        &mut term,
        &mut cache,
        &colors,
        None,
        test_cell(),
        &mut images,
        None,
    );
    assert_eq!(slot_text(&first), vec!["r0", "r1", "r2", "r3"]);

    // Each further line scrolls the viewport up by one: full damage plus a
    // committed line, which is exactly the `Reuse::Scrolled` path. Drift under
    // the old bug grew with every scroll a row survived, so scroll repeatedly.
    for n in 4..12 {
        term.feed(format!("\r\nr{n}").as_bytes());
        let snap = snapshot_reuse(
            &mut term,
            &mut cache,
            &colors,
            None,
            test_cell(),
            &mut images,
            None,
        );
        // Only the newly exposed bottom row is resolved; without reuse the
        // assertion below would pass trivially on a full rebuild.
        assert_eq!(cache.last_snapshot_rows(), 1, "row reuse did not happen");
        let want: Vec<String> = (n - 3..=n).map(|i| format!("r{i}")).collect();
        assert_eq!(slot_text(&snap), want, "after scrolling in r{n}");
    }
}

/// The positional half of the same invariant, checked on the one paint layer
/// that needs no window: a reused row's primitives must land at the y of the
/// slot the row now occupies, not the slot it was first built at.
#[test]
fn scrolled_reuse_moves_a_rows_background_up_with_it() {
    let colors = Rc::new(test_colors());
    let cell = test_cell();
    let origin = point(px(0.0), px(0.0));
    let mut cache = SnapCache::default();
    let mut images = ImageCache::default();
    let mut term = vt::Terminal::new(20, 4, 100);

    // A red run on the bottom row; the other rows have default backgrounds
    // and so contribute no quads at all.
    term.feed(b"a\r\nb\r\nc\r\n\x1b[41mDDD\x1b[0m");
    let snap = snapshot_reuse(
        &mut term,
        &mut cache,
        &colors,
        None,
        cell,
        &mut images,
        None,
    );
    let quads = bg_quads(&snap.rows, origin, cell);
    assert_eq!(quads.len(), 1);
    assert_eq!(quads[0].0.origin.y, px(3.0 * cell.height));

    // Scroll it up a row at a time. Under the old code the quad stayed at
    // y = 3 * cell height for the row's whole life on screen.
    for slot in (0..3).rev() {
        term.feed(b"\r\nz");
        let snap = snapshot_reuse(
            &mut term,
            &mut cache,
            &colors,
            None,
            cell,
            &mut images,
            None,
        );
        assert_eq!(cache.last_snapshot_rows(), 1, "row reuse did not happen");
        let quads = bg_quads(&snap.rows, origin, cell);
        assert_eq!(quads.len(), 1, "the red run is still the only one");
        assert_eq!(
            quads[0].0.origin.y,
            px(slot as f32 * cell.height),
            "red run should have scrolled up to slot {slot}"
        );
    }
}

/// Every row the reuse path produced, as comparable primitives.
fn rows_of(snap: &Snapshot) -> Vec<(Vec<Span>, Vec<BgRun>, Vec<BoxCell>)> {
    snap.rows
        .iter()
        .map(|r| (r.spans.clone(), r.bg_runs.clone(), r.boxes.clone()))
        .collect()
}

/// The reuse machinery (vt damage tracking, `Reuse::Dirty` row carry-over and
/// `Reuse::Scrolled` re-homing, plus whole-snapshot reuse) is an optimization:
/// it must be indistinguishable from resolving every visible cell afresh. Any
/// divergence is a stale-pixel bug on screen.
///
/// Drives two identical terminals through the same bytes -- one through the
/// incremental path, one rebuilt from scratch each step -- and compares. This
/// is the guard that a missed damage mark or a bad reuse key cannot hide
/// behind "it usually looks right".
#[track_caller]
fn assert_reuse_matches_full_rebuild(cols: usize, rows: usize, scrollback: usize, steps: &[&[u8]]) {
    let colors = Rc::new(test_colors());
    let cell = test_cell();
    let mut cache = SnapCache::default();
    let mut incremental_images = ImageCache::default();
    let mut fresh_images = ImageCache::default();
    let mut incremental = vt::Terminal::new(cols, rows, scrollback);
    let mut fresh = vt::Terminal::new(cols, rows, scrollback);

    for (i, step) in steps.iter().enumerate() {
        incremental.feed(step);
        fresh.feed(step);
        let got = snapshot_reuse(
            &mut incremental,
            &mut cache,
            &colors,
            None,
            cell,
            &mut incremental_images,
            None,
        );
        let want = snapshot(&mut fresh, &colors, None, cell, &mut fresh_images, None);
        assert_eq!(
            rows_of(&got),
            rows_of(&want),
            "step {i}: reused snapshot diverged from a full rebuild after {:?}",
            String::from_utf8_lossy(step)
        );
        assert_eq!(got.cursor, want.cursor, "step {i}: cursor diverged");
        assert_eq!(got.offset, want.offset, "step {i}: display offset diverged");
    }
}

#[test]
fn reuse_matches_a_full_rebuild_for_plain_scrolling_output() {
    let mut steps: Vec<Vec<u8>> = Vec::new();
    for i in 0..40 {
        steps.push(format!("line {i} of scrolling output\r\n").into_bytes());
    }
    let refs: Vec<&[u8]> = steps.iter().map(|s| s.as_slice()).collect();
    assert_reuse_matches_full_rebuild(40, 8, 100, &refs);
}

#[test]
fn reuse_matches_a_full_rebuild_for_cursor_addressed_redraws() {
    // The shape a progress display uses: park the cursor, rewrite lines in
    // place, erase to end of line, come back up.
    assert_reuse_matches_full_rebuild(
        40,
        6,
        100,
        &[
            b"aaa\r\nbbb\r\nccc\r\nddd\r\n",
            b"\x1b[2A",          // up two
            b"\x1b[2Krewritten", // erase line, rewrite
            b"\x1b[1B\rmore\x1b[K",
            b"\x1b[H", // home
            b"\x1b[Jcleared down",
            b"\x1b[4;1Hbottom",
            b"\x1b[1;1H\x1b[1M", // delete a line
            b"\x1b[1L",          // insert a line
            b"\x1b[3;5r",        // scroll region
            b"\x1b[3;1Hin region\r\n\r\n\r\n",
            b"\x1b[r",     // reset region
            b"\x1bM\x1bM", // reverse index twice
        ],
    );
}

#[test]
fn reuse_matches_a_full_rebuild_for_styles_wide_chars_and_box_drawing() {
    assert_reuse_matches_full_rebuild(
        30,
        6,
        100,
        &[
            b"\x1b[1;31mbold red\x1b[0m\r\n",
            b"\x1b[4;32munderline\x1b[0m\r\n",
            b"\x1b[7minverse\x1b[0m\r\n",
            "\u{4e16}\u{754c} wide\r\n".as_bytes(),
            "\u{250c}\u{2500}\u{2510}\r\n".as_bytes(),
            "\u{2588}\u{2591} blocks\r\n".as_bytes(),
            b"\x1b[48;2;10;20;30mtruecolor bg\x1b[0m\r\n",
            b"\x1b[9mstrike\x1b[0m\r\n",
            b"\x1b[2mdim\x1b[0m\r\n",
            b"tail\r\n\r\n\r\n\r\n",
        ],
    );
}

#[test]
fn reuse_matches_a_full_rebuild_across_screen_and_scrollback_changes() {
    assert_reuse_matches_full_rebuild(
        30,
        5,
        100,
        &[
            b"one\r\ntwo\r\nthree\r\nfour\r\nfive\r\nsix\r\n",
            b"\x1b[?1049h", // alt screen
            b"alt content\r\nmore alt\r\n",
            b"\x1b[2J\x1b[H", // clear alt
            b"redrawn alt\r\n",
            b"\x1b[?1049l", // back to primary
            b"after alt\r\n",
            b"\x1b[3J", // drop scrollback
            b"\x1bc",   // RIS
            b"post reset\r\n",
        ],
    );
}

/// The same equivalence under a wide, deterministic mix of operations. Fixed
/// seed so a failure is reproducible; the point is to reach interleavings a
/// hand-written corpus does not think of.
#[test]
fn reuse_matches_a_full_rebuild_under_a_mixed_operation_fuzz() {
    for seed in [
        0x5171_2c9f_1a3b_77d5,
        0x0123_4567_89ab_cdef,
        0xfeed_face_dead_beef,
        0x9e37_79b9_7f4a_7c15,
        0x1,
        0xffff_ffff_ffff_ffff,
    ] {
        fuzz_one_seed(seed);
    }
}

fn fuzz_one_seed(seed: u64) {
    let mut state: u64 = seed;
    let mut rand = move || {
        // xorshift64*: deterministic, no external dependency.
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        state.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 33
    };
    let mut steps: Vec<Vec<u8>> = Vec::new();
    for _ in 0..600 {
        let n = rand();
        let step: Vec<u8> = match n % 20 {
            0..=2 => format!("text{}", n % 1000).into_bytes(),
            3 | 4 => b"\r\n".to_vec(),
            5 => format!("\x1b[{};{}H", n % 9 + 1, n % 25 + 1).into_bytes(),
            6 => format!("\x1b[{}A", n % 4 + 1).into_bytes(),
            7 => format!("\x1b[{}B", n % 4 + 1).into_bytes(),
            8 => format!("\x1b[{}C", n % 6 + 1).into_bytes(),
            9 => format!("\x1b[{}D", n % 6 + 1).into_bytes(),
            10 => format!("\x1b[{}K", n % 3).into_bytes(),
            11 => format!("\x1b[{}J", n % 3).into_bytes(),
            12 => format!("\x1b[{}L", n % 3 + 1).into_bytes(),
            13 => format!("\x1b[{}M", n % 3 + 1).into_bytes(),
            14 => format!("\x1b[{}P", n % 4 + 1).into_bytes(),
            15 => format!("\x1b[{}m", n % 8 + 30).into_bytes(),
            16 => format!("\x1b[4{}m", n % 8).into_bytes(),
            17 => "\u{4e16}\u{2500}\u{2588}".as_bytes().to_vec(),
            18 => b"\x1bM".to_vec(),
            _ => b"\t\x08 ".to_vec(),
        };
        steps.push(step);
    }
    let refs: Vec<&[u8]> = steps.iter().map(|s| s.as_slice()).collect();
    assert_reuse_matches_full_rebuild(25, 9, 200, &refs);
}

fn blank_row() -> Rc<RowSnapshot> {
    Rc::new(RowSnapshot {
        source_revision: 0,
        bg_runs: Vec::new(),
        spans: Vec::new(),
        boxes: Vec::new(),
    })
}

fn shaped_for(row: &Rc<RowSnapshot>) -> ShapedRow {
    ShapedRow {
        source: row.clone(),
        lines: Vec::new(),
    }
}

/// Which cached slots still point at the row they were shaped for.
fn settled_slots(cache: &[Option<ShapedRow>], rows: &[Rc<RowSnapshot>]) -> Vec<usize> {
    cache
        .iter()
        .enumerate()
        .filter(|(i, slot)| {
            slot.as_ref()
                .is_some_and(|s| Rc::ptr_eq(&s.source, &rows[*i]))
        })
        .map(|(i, _)| i)
        .collect()
}

#[test]
fn realign_reshapes_nothing_when_no_row_moved() {
    let rows: Vec<Rc<RowSnapshot>> = (0..5).map(|_| blank_row()).collect();
    let mut cache: Vec<Option<ShapedRow>> = rows.iter().map(|r| Some(shaped_for(r))).collect();
    // An idle frame hands back the very same row objects.
    assert_eq!(realign_shaped_rows(&mut cache, &rows), Vec::<usize>::new());
    assert_eq!(settled_slots(&cache, &rows), vec![0, 1, 2, 3, 4]);
}

#[test]
fn realign_rehomes_scrolled_rows_and_shapes_only_the_new_one() {
    let rows: Vec<Rc<RowSnapshot>> = (0..5).map(|_| blank_row()).collect();
    let mut cache: Vec<Option<ShapedRow>> = rows.iter().map(|r| Some(shaped_for(r))).collect();
    // A one-line scroll: every surviving row moves up a slot, one row is new.
    let fresh = blank_row();
    let scrolled: Vec<Rc<RowSnapshot>> = rows[1..].iter().cloned().chain([fresh]).collect();
    assert_eq!(realign_shaped_rows(&mut cache, &scrolled), vec![4]);
    // The four survivors kept their shaped lines, at their new slots.
    assert_eq!(settled_slots(&cache, &scrolled), vec![0, 1, 2, 3]);
}

#[test]
fn realign_shapes_only_the_row_that_changed_in_place() {
    let rows: Vec<Rc<RowSnapshot>> = (0..5).map(|_| blank_row()).collect();
    let mut cache: Vec<Option<ShapedRow>> = rows.iter().map(|r| Some(shaped_for(r))).collect();
    // Partial damage replaces one row object, leaving the rest untouched.
    let mut damaged = rows.clone();
    damaged[2] = blank_row();
    assert_eq!(realign_shaped_rows(&mut cache, &damaged), vec![2]);
    assert_eq!(settled_slots(&cache, &damaged), vec![0, 1, 3, 4]);
}

#[test]
fn realign_follows_the_row_count_when_the_grid_resizes() {
    let rows: Vec<Rc<RowSnapshot>> = (0..5).map(|_| blank_row()).collect();
    let mut cache: Vec<Option<ShapedRow>> = rows.iter().map(|r| Some(shaped_for(r))).collect();
    // Shrink: the cache must not keep stale slots past the end.
    let shorter: Vec<Rc<RowSnapshot>> = rows[..3].to_vec();
    assert_eq!(
        realign_shaped_rows(&mut cache, &shorter),
        Vec::<usize>::new()
    );
    assert_eq!(cache.len(), 3);
    // Grow: the added slots need shaping, the existing ones do not.
    let taller: Vec<Rc<RowSnapshot>> = shorter
        .iter()
        .cloned()
        .chain((0..2).map(|_| blank_row()))
        .collect();
    assert_eq!(realign_shaped_rows(&mut cache, &taller), vec![3, 4]);
    assert_eq!(cache.len(), 5);
}

#[test]
fn realign_handles_a_row_object_appearing_in_two_slots() {
    // Duplicate rows can only be served once from the cache; the other slot
    // must be reported for shaping rather than silently left empty.
    let shared = blank_row();
    let rows = [shared.clone(), blank_row()];
    let mut cache: Vec<Option<ShapedRow>> = rows.iter().map(|r| Some(shaped_for(r))).collect();
    let duped = vec![shared.clone(), shared.clone()];
    let todo = realign_shaped_rows(&mut cache, &duped);
    assert_eq!(todo, vec![1]);
    assert!(cache[0].is_some());
    assert!(cache[1].is_none());
}
