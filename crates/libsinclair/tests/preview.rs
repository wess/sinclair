use super::*;

use gpui::size as gsize;

fn colors() -> Colors {
    Colors::from_scheme(theme::default_scheme())
}

fn term(text: &str) -> vt::Terminal {
    let mut term = vt::Terminal::new(20, 4, 100);
    term.feed(text.as_bytes());
    term
}

#[test]
fn blank_cells_leave_no_ink() {
    let colors = colors();
    let mini = capture(&mut term("ab  cd"), &colors);
    assert_eq!(mini.cols, 20);
    assert_eq!(mini.rows.len(), 4);
    let ink = &mini.rows[0].ink;
    // The gap splits the line in two, and trailing blanks add nothing: a row
    // of spaces is what makes an idle pane read as idle.
    assert_eq!(ink.len(), 2);
    assert_eq!((ink[0].col, ink[0].len), (0, 2));
    assert_eq!((ink[1].col, ink[1].len), (4, 2));
    assert!(mini.rows[1].ink.is_empty());
}

#[test]
fn adjacent_cells_of_one_color_merge_and_a_color_change_splits() {
    let colors = colors();
    // Three red, then three green, with no gap between them.
    let mini = capture(&mut term("\x1b[31maaa\x1b[32mbbb"), &colors);
    let ink = &mini.rows[0].ink;
    assert_eq!(ink.len(), 2);
    assert_eq!((ink[0].col, ink[0].len), (0, 3));
    assert_eq!((ink[1].col, ink[1].len), (3, 3));
    assert_ne!(ink[0].color, ink[1].color);
}

#[test]
fn default_background_is_left_to_the_card_and_a_set_one_is_a_run() {
    let colors = colors();
    let mini = capture(&mut term("ab\x1b[44mcd"), &colors);
    let bg = &mini.rows[0].bg;
    assert_eq!(bg.len(), 1);
    assert_eq!((bg[0].col, bg[0].len), (2, 2));
}

#[test]
fn inverse_swaps_the_pair_so_a_status_line_reads_as_a_bar() {
    let colors = colors();
    let mini = capture(&mut term("\x1b[7mab"), &colors);
    let row = &mini.rows[0];
    assert_eq!(row.bg.len(), 1);
    assert_eq!(row.bg[0].color, colors.fg);
    assert_eq!(row.ink[0].color, colors.bg);
}

#[test]
fn a_scrolled_back_screen_hides_the_cursor() {
    let colors = colors();
    let mut t = vt::Terminal::new(20, 2, 100);
    t.feed(b"one\r\ntwo\r\nthree\r\n");
    assert!(capture(&mut t, &colors).cursor.is_some());
    t.scroll_display(1);
    assert!(capture(&mut t, &colors).cursor.is_none());
}

#[test]
fn the_miniature_keeps_the_terminal_aspect_and_centres_in_the_card() {
    let cell = CellSize {
        width: 10.0,
        height: 20.0,
    };
    // A card twice as wide as the 10x4 grid needs: the fit is height-bound,
    // so the miniature keeps its shape and the slack goes to the margins.
    let bounds = Bounds::new(point(px(0.0), px(0.0)), gsize(px(400.0), px(80.0)));
    let (origin, cw, ch) = fit(bounds, 10, 4, cell);
    assert_eq!((cw, ch), (10.0, 20.0));
    assert_eq!(origin.x, px(150.0));
    assert_eq!(origin.y, px(0.0));
}

#[test]
fn quads_are_placed_by_row_and_stay_inside_the_card() {
    let colors = colors();
    let mini = capture(&mut term("hi"), &colors);
    let cell = CellSize {
        width: 8.0,
        height: 16.0,
    };
    let bounds = Bounds::new(point(px(0.0), px(0.0)), gsize(px(80.0), px(64.0)));
    let quads = quads(&mini, bounds, cell, 1.0);
    assert!(!quads.is_empty());
    for (b, _) in &quads {
        assert!(b.origin.x >= bounds.origin.x);
        assert!(b.origin.y >= bounds.origin.y);
        assert!(b.origin.x + b.size.width <= bounds.origin.x + bounds.size.width);
        assert!(b.origin.y + b.size.height <= bounds.origin.y + bounds.size.height);
    }
}

#[test]
fn an_unmeasured_card_paints_nothing_rather_than_dividing_by_zero() {
    let colors = colors();
    let mini = capture(&mut term("hi"), &colors);
    let cell = CellSize {
        width: 8.0,
        height: 16.0,
    };
    let bounds = Bounds::new(point(px(0.0), px(0.0)), gsize(px(0.0), px(0.0)));
    assert!(quads(&mini, bounds, cell, 1.0).is_empty());
}
