use super::*;
use crate::cell::{Cell, CellFlags};

const EXTRA: &[char] = &['/', '-', '_', '.', '~'];

fn put(grid: &mut Grid, row: usize, text: &str) {
    for (c, ch) in text.chars().enumerate() {
        grid.cell_mut(row, c).ch = ch;
    }
}

fn grid_3x10(lines: &[&str]) -> Grid {
    let mut g = Grid::new(10, lines.len(), 100);
    for (r, l) in lines.iter().enumerate() {
        put(&mut g, r, l);
    }
    g
}

fn span(s: (isize, usize), e: (isize, usize)) -> (Point, Point) {
    (Point::new(s.0, s.1), Point::new(e.0, e.1))
}

#[test]
fn point_ordering_is_reading_order() {
    assert!(Point::new(-1, 9) < Point::new(0, 0));
    assert!(Point::new(0, 1) < Point::new(0, 2));
    assert!(Point::new(1, 0) > Point::new(0, 9));
}

#[test]
fn normalization_orders_start_end() {
    // Anchor below/after the extent.
    let mut sel = Selection::new(SelectionMode::Cell, span((2, 5), (2, 5)));
    sel.update(span((0, 3), (0, 3)));
    assert_eq!(sel.start(), Point::new(0, 3));
    assert_eq!(sel.end(), Point::new(2, 5));
    // Extent moves past the anchor the other way.
    sel.update(span((3, 1), (3, 1)));
    assert_eq!(sel.start(), Point::new(2, 5));
    assert_eq!(sel.end(), Point::new(3, 1));
}

#[test]
fn contains_streams_across_rows() {
    let mut sel = Selection::new(SelectionMode::Cell, span((0, 4), (0, 4)));
    sel.update(span((2, 2), (2, 2)));
    assert!(sel.contains(Point::new(0, 4)));
    assert!(!sel.contains(Point::new(0, 3)));
    assert!(sel.contains(Point::new(1, 0))); // middle row fully selected
    assert!(sel.contains(Point::new(1, 9)));
    assert!(sel.contains(Point::new(2, 2)));
    assert!(!sel.contains(Point::new(2, 3)));
}

#[test]
fn shift_lines_moves_all_points() {
    let mut sel = Selection::new(SelectionMode::Cell, span((1, 0), (2, 9)));
    sel.shift_lines(-3);
    assert_eq!(sel.start(), Point::new(-2, 0));
    assert_eq!(sel.end(), Point::new(-1, 9));
}

#[test]
fn word_expansion_within_row() {
    let g = grid_3x10(&["foo bar.gz"]);
    let (s, e) = expand_word(&g, Point::new(0, 5), EXTRA);
    assert_eq!((s, e), span((0, 4), (0, 9))); // "bar.gz": '.' is a word char
    let (s, e) = expand_word(&g, Point::new(0, 1), EXTRA);
    assert_eq!((s, e), span((0, 0), (0, 2)));
}

#[test]
fn word_expansion_on_non_word_is_single_cell() {
    let g = grid_3x10(&["foo bar"]);
    let (s, e) = expand_word(&g, Point::new(0, 3), EXTRA);
    assert_eq!((s, e), span((0, 3), (0, 3)));
}

#[test]
fn word_expansion_crosses_soft_wrap() {
    let mut g = Grid::new(4, 2, 10);
    put(&mut g, 0, " abc");
    put(&mut g, 1, "de f");
    g.row_mut(0).wrapped = true;
    let (s, e) = expand_word(&g, Point::new(0, 2), EXTRA);
    assert_eq!((s, e), span((0, 1), (1, 1)));
    // Without the wrap flag the word stops at the row edge.
    g.row_mut(0).wrapped = false;
    let (s, e) = expand_word(&g, Point::new(0, 2), EXTRA);
    assert_eq!((s, e), span((0, 1), (0, 3)));
}

/// "a漢 b" laid out as a real grid: head at 1, spacer at 2, space at
/// 3, 'b' at 4.
fn wide_grid() -> Grid {
    let mut g = Grid::new(10, 1, 0);
    g.cell_mut(0, 0).ch = 'a';
    g.cell_mut(0, 1).ch = '漢';
    g.cell_mut(0, 1).flags.insert(CellFlags::WIDE);
    g.cell_mut(0, 2).flags.insert(CellFlags::WIDE_SPACER);
    g.cell_mut(0, 4).ch = 'b';
    g
}

#[test]
fn word_expansion_treats_wide_spacer_as_its_head() {
    let g = wide_grid();
    let (s, e) = expand_word(&g, Point::new(0, 0), EXTRA);
    assert_eq!((s, e), span((0, 0), (0, 2))); // spans head + spacer
                                              // Starting on the spacer itself classifies as the head.
    let (s, e) = expand_word(&g, Point::new(0, 2), EXTRA);
    assert_eq!((s, e), span((0, 0), (0, 2)));
}

#[test]
fn line_expansion_follows_wraps_both_ways() {
    let mut g = grid_3x10(&["aaaaaaaaaa", "bbbbbbbbbb", "cc"]);
    g.row_mut(0).wrapped = true;
    g.row_mut(1).wrapped = true;
    let (s, e) = expand_line(&g, Point::new(1, 4));
    assert_eq!((s, e), span((0, 0), (2, 9)));
    // An unwrapped row is its own logical line.
    let g2 = grid_3x10(&["a", "b", "c"]);
    let (s, e) = expand_line(&g2, Point::new(1, 4));
    assert_eq!((s, e), span((1, 0), (1, 9)));
}

#[test]
fn line_expansion_reaches_into_scrollback() {
    let mut g = Grid::new(4, 2, 10);
    put(&mut g, 0, "wrap");
    g.row_mut(0).wrapped = true;
    g.scroll_up(0, 1, 1, true, Cell::default()); // "wrap" -> scrollback
    put(&mut g, 0, "tail");
    let (s, e) = expand_line(&g, Point::new(0, 1));
    assert_eq!((s, e), span((-1, 0), (0, 3)));
}

#[test]
fn text_joins_soft_wraps_without_newline() {
    let mut g = grid_3x10(&["aaaaaaaaaa", "bb", "cc"]);
    g.row_mut(0).wrapped = true;
    let sel = Selection::new(SelectionMode::Cell, span((0, 0), (2, 9)));
    assert_eq!(text(&g, &sel), "aaaaaaaaaabb\ncc");
}

#[test]
fn text_trims_trailing_whitespace_per_hard_line() {
    let g = grid_3x10(&["hi   ", "there  "]);
    let sel = Selection::new(SelectionMode::Cell, span((0, 0), (1, 9)));
    assert_eq!(text(&g, &sel), "hi\nthere");
}

#[test]
fn text_skips_wide_spacers() {
    let g = wide_grid();
    let sel = Selection::new(SelectionMode::Cell, span((0, 0), (0, 4)));
    assert_eq!(text(&g, &sel), "a漢 b");
}

#[test]
fn text_partial_first_and_last_rows() {
    let g = grid_3x10(&["abcdefghij", "klmnopqrst"]);
    let sel = Selection::new(SelectionMode::Cell, span((0, 7), (1, 2)));
    assert_eq!(text(&g, &sel), "hij\nklm");
}

#[test]
fn text_spans_scrollback_and_live() {
    let mut g = Grid::new(4, 2, 10);
    put(&mut g, 0, "old");
    g.scroll_up(0, 1, 1, true, Cell::default());
    put(&mut g, 0, "new");
    let sel = Selection::new(SelectionMode::Cell, span((-1, 0), (0, 3)));
    assert_eq!(text(&g, &sel), "old\nnew");
}

#[test]
fn clamp_point_bounds() {
    let mut g = Grid::new(4, 2, 10);
    g.scroll_up(0, 1, 1, true, Cell::default());
    assert_eq!(clamp_point(&g, Point::new(-9, 99)), Point::new(-1, 3));
    assert_eq!(clamp_point(&g, Point::new(9, 0)), Point::new(1, 0));
}
