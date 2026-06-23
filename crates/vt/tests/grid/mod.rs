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
    assert_eq!(g.scrollback().get(0).unwrap().text(), "a");
    assert_eq!(g.scrollback().get(1).unwrap().text(), "b");
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
    g.resize(8, 5);
    assert_eq!(g.cols(), 8);
    assert_eq!(g.rows(), 5);
    assert_eq!(g.row(0).text(), "a");
    assert_eq!(g.row(4).text(), "");
    g.resize(2, 2);
    assert_eq!(g.cols(), 2);
    assert_eq!(g.rows(), 2);
    assert_eq!(g.row(1).text(), "b");
}

#[test]
fn resize_clamps_to_one() {
    let mut g = Grid::new(4, 4, 0);
    g.resize(0, 0);
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
    g.resize(8, 5);
    assert_eq!(g.take_damage(), Damage::Full);
    g.clear_scrollback();
    assert_eq!(g.take_damage(), Damage::Full);
}
