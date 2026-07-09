use super::*;

const CELL: CellSize = CellSize {
    width: 8.0,
    height: 17.0,
};
const PAD: Padding = Padding { x: 2.0, y: 2.0 };

#[test]
fn grid_size_floors_partial_cells() {
    // 644 - 4 = 640 / 8 = 80 cols; 412.8 - 4 = 408.8 / 17 = 24.04 -> 24.
    assert_eq!(grid_size(644.0, 412.8, PAD, CELL), (80, 24));
    // One pixel short of a column.
    assert_eq!(grid_size(643.0, 412.8, PAD, CELL), (79, 24));
}

#[test]
fn grid_size_clamps_minimums() {
    assert_eq!(grid_size(0.0, 0.0, PAD, CELL), (2, 1));
    assert_eq!(grid_size(-50.0, 5.0, PAD, CELL), (2, 1));
    assert_eq!(grid_size(10.0, 18.0, Padding::default(), CELL), (2, 1));
}

#[test]
fn pixel_size_includes_padding_on_both_sides() {
    assert_eq!(pixel_size(80, 24, PAD, CELL), (644.0, 412.0));
    assert_eq!(pixel_size(80, 24, Padding::default(), CELL), (640.0, 408.0));
}

#[test]
fn round_trips_exact_grids() {
    let (w, h) = pixel_size(120, 40, PAD, CELL);
    assert_eq!(grid_size(w, h, PAD, CELL), (120, 40));
}

#[test]
fn line_height_factor_is_sane() {
    const { assert!(LINE_HEIGHT > 1.0 && LINE_HEIGHT < 2.0) }
}

#[test]
fn cell_at_accounts_for_origin_and_padding() {
    // Window origin (100, 50), pad 2: cell (0,0) spans x 102..110.
    assert_eq!(
        cell_at((102.0, 52.0), (100.0, 50.0), PAD, CELL, 80, 24),
        (0, 0)
    );
    assert_eq!(
        cell_at((109.9, 68.9), (100.0, 50.0), PAD, CELL, 80, 24),
        (0, 0)
    );
    // One pixel into the next cell each way.
    assert_eq!(
        cell_at((110.0, 69.0), (100.0, 50.0), PAD, CELL, 80, 24),
        (1, 1)
    );
    // Mid-grid.
    assert_eq!(
        cell_at(
            (102.0 + 8.0 * 10.0, 52.0 + 17.0 * 3.0),
            (100.0, 50.0),
            PAD,
            CELL,
            80,
            24
        ),
        (3, 10)
    );
}

#[test]
fn cell_at_clamps_to_grid() {
    // Inside the padding band, above/left of cell 0.
    assert_eq!(cell_at((0.0, 0.0), (0.0, 0.0), PAD, CELL, 80, 24), (0, 0));
    // Way past the bottom-right corner.
    assert_eq!(
        cell_at((9999.0, 9999.0), (0.0, 0.0), PAD, CELL, 80, 24),
        (23, 79)
    );
    // Negative positions (drag left/above the window).
    assert_eq!(
        cell_at((-50.0, -50.0), (0.0, 0.0), PAD, CELL, 80, 24),
        (0, 0)
    );
    // Degenerate grid never underflows.
    assert_eq!(cell_at((5.0, 5.0), (0.0, 0.0), PAD, CELL, 0, 0), (0, 0));
}

#[test]
fn selection_point_maps_display_offset() {
    // Live view: viewport row == content line.
    assert_eq!(selection_point(0, 3, 0), vt::Point::new(0, 3));
    assert_eq!(selection_point(5, 0, 0), vt::Point::new(5, 0));
    // Scrolled back 4 lines: top viewport row shows scrollback line -4.
    assert_eq!(selection_point(0, 1, 4), vt::Point::new(-4, 1));
    assert_eq!(selection_point(4, 1, 4), vt::Point::new(0, 1));
    assert_eq!(selection_point(6, 9, 4), vt::Point::new(2, 9));
}
