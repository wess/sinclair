use super::*;

fn grid_with_letters(rows: usize) -> Grid {
    let mut g = Grid::new(4, rows, 100);
    for r in 0..rows {
        g.cell_mut(r, 0).ch = (b'a' + r as u8) as char;
    }
    g
}

#[test]
fn new_grid_dimensions() {
    let g = Grid::new(80, 24, 10);
    assert_eq!(g.cols(), 80);
    assert_eq!(g.rows(), 24);
    assert_eq!(g.row(0).len(), 80);
}

#[test]
fn scroll_up_moves_rows_and_blanks_bottom() {
    let mut g = grid_with_letters(3);
    g.scroll_up(0, 2, 1, false, Cell::default());
    assert_eq!(g.row(0).text(), "b");
    assert_eq!(g.row(1).text(), "c");
    assert_eq!(g.row(2).text(), "");
}

#[test]
fn scroll_up_saves_to_scrollback() {
    let mut g = grid_with_letters(3);
    g.scroll_up(0, 2, 2, true, Cell::default());
    assert_eq!(g.scrollback().len(), 2);
    assert_eq!(g.scrollback_mut().row(0).unwrap().text(), "a");
    assert_eq!(g.scrollback_mut().row(1).unwrap().text(), "b");
    assert_eq!(g.row(0).text(), "c");
}

#[test]
fn scroll_up_region_only() {
    let mut g = grid_with_letters(4);
    g.scroll_up(1, 2, 1, false, Cell::default());
    assert_eq!(g.row(0).text(), "a");
    assert_eq!(g.row(1).text(), "c");
    assert_eq!(g.row(2).text(), "");
    assert_eq!(g.row(3).text(), "d");
}

#[test]
fn scroll_down_moves_rows_and_blanks_top() {
    let mut g = grid_with_letters(3);
    g.scroll_down(0, 2, 1, Cell::default());
    assert_eq!(g.row(0).text(), "");
    assert_eq!(g.row(1).text(), "a");
    assert_eq!(g.row(2).text(), "b");
}

#[test]
fn scroll_clamps_oversized_count() {
    let mut g = grid_with_letters(3);
    g.scroll_up(0, 2, 99, false, Cell::default());
    assert_eq!(g.row(0).text(), "");
    assert_eq!(g.row(2).text(), "");
}

#[test]
fn resize_grows_and_shrinks() {
    let mut g = grid_with_letters(3);
    g.resize(8, 5, (0, 0));
    assert_eq!(g.cols(), 8);
    assert_eq!(g.rows(), 5);
    assert_eq!(g.row(0).text(), "a");
    assert_eq!(g.row(4).text(), "");
    // Shrinking to 2 rows pushes the oldest reflowed line ("a") to scrollback.
    g.resize(2, 2, (0, 0));
    assert_eq!(g.cols(), 2);
    assert_eq!(g.rows(), 2);
    assert_eq!(g.row(0).text(), "b");
    assert_eq!(g.row(1).text(), "c");
}

#[test]
fn height_shrink_rotates_top_rows_into_scrollback() {
    let mut g = grid_with_letters(4);
    let cursor = g.resize(4, 2, (3, 0));
    assert_eq!(g.rows(), 2);
    assert_eq!(g.row(0).text(), "c");
    assert_eq!(g.row(1).text(), "d");
    assert_eq!(g.scrollback().len(), 2);
    assert_eq!(g.scrollback_mut().row(0).unwrap().text(), "a");
    assert_eq!(g.scrollback_mut().row(1).unwrap().text(), "b");
    assert_eq!(cursor, (1, 0));
}

#[test]
fn height_shrink_drops_blank_rows_below_cursor_first() {
    let mut g = Grid::new(4, 4, 100);
    g.cell_mut(0, 0).ch = 'a';
    g.cell_mut(1, 0).ch = 'b'; // rows 2 and 3 stay blank, cursor on row 1
    let cursor = g.resize(4, 2, (1, 0));
    assert_eq!(g.row(0).text(), "a");
    assert_eq!(g.row(1).text(), "b");
    assert!(g.scrollback().is_empty());
    assert_eq!(cursor, (1, 0));
}

#[test]
fn height_grow_pulls_rows_back_from_scrollback() {
    let mut g = grid_with_letters(4);
    g.resize(4, 2, (3, 0)); // "a" and "b" rotate out
    let cursor = g.resize(4, 4, (1, 0));
    assert!(g.scrollback().is_empty());
    assert_eq!(g.row(0).text(), "a");
    assert_eq!(g.row(1).text(), "b");
    assert_eq!(g.row(2).text(), "c");
    assert_eq!(g.row(3).text(), "d");
    assert_eq!(cursor, (3, 0));
}

#[test]
fn alt_screen_height_shrink_truncates_bottom() {
    // No scrollback (the alternate screen): plain truncation, as before.
    let mut g = Grid::new(4, 4, 0);
    for r in 0..4 {
        g.cell_mut(r, 0).ch = (b'a' + r as u8) as char;
    }
    g.resize(4, 2, (0, 0));
    assert_eq!(g.row(0).text(), "a");
    assert_eq!(g.row(1).text(), "b");
    assert_eq!(g.scrollback().len(), 0);
}

#[test]
fn width_resize_preserves_committed_counter() {
    let mut g = Grid::new(4, 2, 100);
    for ch in ['a', 'b', 'c', 'd'] {
        g.cell_mut(0, 0).ch = ch;
        g.scroll_up(0, 1, 1, true, Cell::default());
    }
    g.cell_mut(0, 0).ch = 'e';
    g.cell_mut(1, 0).ch = 'f';
    assert_eq!(g.scrollback().committed(), 4);
    g.resize(6, 2, (1, 0)); // width change reflows through scrollback
    assert_eq!(g.scrollback().len(), 4);
    assert_eq!(g.scrollback().committed(), 4); // not inflated by the rebuild
    assert_eq!(g.row(0).text(), "e");
    assert_eq!(g.row(1).text(), "f");
}

#[test]
fn reflow_keeps_wide_pairs_together() {
    use crate::cell::CellFlags;
    let mut g = Grid::new(4, 2, 100);
    g.cell_mut(0, 0).ch = 'a';
    g.cell_mut(0, 1).ch = 'b';
    g.cell_mut(0, 2).ch = '\u{6f22}';
    g.cell_mut(0, 2).flags.insert(CellFlags::WIDE);
    g.cell_mut(0, 3).flags.insert(CellFlags::WIDE_SPACER);
    g.resize(3, 2, (0, 0));
    // The pair would straddle the 3-column boundary; the break backs off.
    assert_eq!(g.row(0).text(), "ab");
    assert!(g.row(0).wrapped);
    assert!(g.row(1).cells[0].is_wide());
    assert!(g.row(1).cells[1].is_wide_spacer());
}

#[test]
fn reflow_rejoins_and_rewraps_wrapped_line() {
    // Two rows forming one logical line "abcdef" wrapped at width 3.
    let mut g = Grid::new(3, 4, 100);
    for (c, ch) in "abc".chars().enumerate() {
        g.cell_mut(0, c).ch = ch;
    }
    g.row_mut(0).wrapped = true;
    for (c, ch) in "def".chars().enumerate() {
        g.cell_mut(1, c).ch = ch;
    }
    // Widen to 6: the logical line now fits on one row.
    g.resize(6, 4, (1, 0));
    assert_eq!(g.cols(), 6);
    assert_eq!(g.row(0).text(), "abcdef");
    assert!(!g.row(0).wrapped);
    // Narrow to 2: it re-wraps into three rows abc/def -> "ab","cd","ef".
    g.resize(2, 4, (0, 0));
    assert_eq!(g.row(0).text(), "ab");
    assert_eq!(g.row(1).text(), "cd");
    assert_eq!(g.row(2).text(), "ef");
    assert!(g.row(0).wrapped);
    assert!(g.row(1).wrapped);
    assert!(!g.row(2).wrapped);
}

#[test]
fn reflow_follows_the_cursor() {
    // Logical line "abcdef" wrapped at 3; cursor on the 'e' (row 1, col 1).
    let mut g = Grid::new(3, 4, 100);
    for (c, ch) in "abc".chars().enumerate() {
        g.cell_mut(0, c).ch = ch;
    }
    g.row_mut(0).wrapped = true;
    for (c, ch) in "def".chars().enumerate() {
        g.cell_mut(1, c).ch = ch;
    }
    // Widen to 6: 'e' is the 5th char (offset 4) -> row 0, col 4.
    let cursor = g.resize(6, 4, (1, 1));
    assert_eq!(cursor, (0, 4));
}

#[test]
fn reflow_preserves_prompt_mark_on_first_segment() {
    let mut g = Grid::new(6, 4, 100);
    for (c, ch) in "abcdef".chars().enumerate() {
        g.cell_mut(0, c).ch = ch;
    }
    g.row_mut(0).prompt = true;
    g.resize(3, 4, (0, 0));
    // Splits into "abc"/"def"; the prompt mark rides the first segment only.
    assert!(g.row(0).prompt);
    assert!(!g.row(1).prompt);
}

#[test]
fn resize_clamps_to_one() {
    let mut g = Grid::new(4, 4, 0);
    g.resize(0, 0, (0, 0));
    assert_eq!(g.cols(), 1);
    assert_eq!(g.rows(), 1);
}

#[test]
fn fresh_grid_is_fully_damaged() {
    let mut g = Grid::new(4, 4, 0);
    assert_eq!(g.take_damage(), Damage::Full);
    assert_eq!(g.take_damage(), Damage::Rows(vec![]));
}

#[test]
fn cell_and_row_mutation_mark_rows() {
    let mut g = Grid::new(4, 4, 0);
    g.take_damage();
    g.cell_mut(2, 1).ch = 'x';
    g.row_mut(0).fill(Cell::default());
    assert_eq!(g.take_damage(), Damage::Rows(vec![0, 2]));
    assert_eq!(g.take_damage(), Damage::Rows(vec![]));
}

#[test]
fn scroll_escalates_to_full() {
    let mut g = grid_with_letters(3);
    g.take_damage();
    g.scroll_up(0, 2, 1, false, Cell::default());
    assert_eq!(g.take_damage(), Damage::Full);
    g.scroll_down(0, 2, 1, Cell::default());
    assert_eq!(g.take_damage(), Damage::Full);
}

#[test]
fn resize_and_scrollback_clear_escalate_to_full() {
    let mut g = grid_with_letters(3);
    g.take_damage();
    g.resize(8, 5, (0, 0));
    assert_eq!(g.take_damage(), Damage::Full);
    g.clear_scrollback();
    assert_eq!(g.take_damage(), Damage::Full);
}
