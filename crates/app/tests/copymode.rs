use super::*;

#[test]
fn vtline_maps_global_rows_onto_selection_space() {
    // No scrollback: global row 0 is the top live row.
    assert_eq!(vtline(0, 0), 0);
    assert_eq!(vtline(3, 0), 3);
    // With history, the top live row sits at global index `sb`.
    assert_eq!(vtline(5, 5), 0);
    // Scrollback rows are negative: -1 is the newest history row.
    assert_eq!(vtline(4, 5), -1);
    assert_eq!(vtline(0, 5), -5);
}

#[test]
fn converted_points_select_scrollback_text() {
    let mut term = vt::Terminal::new(10, 3, 100);
    // Ten numbered lines on a 3-row grid: eight scroll into history.
    for i in 0..10 {
        term.feed(format!("line{i}\r\n").as_bytes());
    }
    let sb = term.grid().scrollback().len();
    assert_eq!(sb, 8);

    // Select all of global row 2 ("line2"), which lives in scrollback. The
    // pre-fix global-row points would clamp to the bottom live row instead.
    term.start_selection(vt::SelectionMode::Cell, vt::Point::new(vtline(2, sb), 0));
    term.update_selection(vt::Point::new(vtline(2, sb), 9));
    assert_eq!(term.selection_text().as_deref(), Some("line2"));

    // And a live row: global index sb + 0 is "line8" (the top live row).
    term.start_selection(
        vt::SelectionMode::Cell,
        vt::Point::new(vtline(sb as isize, sb), 0),
    );
    term.update_selection(vt::Point::new(vtline(sb as isize, sb), 9));
    assert_eq!(term.selection_text().as_deref(), Some("line8"));
}
