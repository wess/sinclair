use super::*;

#[test]
fn default_tab_stops_every_eight() {
    let s = Screen::new(20, 4, 0);
    assert_eq!(s.next_tab(0), 8);
    assert_eq!(s.next_tab(8), 16);
    assert_eq!(s.next_tab(16), 19);
    assert_eq!(s.prev_tab(19), 16);
    assert_eq!(s.prev_tab(8), 0);
    assert_eq!(s.prev_tab(3), 0);
}

#[test]
fn custom_tab_stops() {
    let mut s = Screen::new(20, 4, 0);
    s.clear_all_tabs();
    s.set_tab(5);
    assert_eq!(s.next_tab(0), 5);
    assert_eq!(s.next_tab(5), 19);
    s.clear_tab(5);
    assert_eq!(s.next_tab(0), 19);
}

#[test]
fn new_screen_region_is_full() {
    let s = Screen::new(10, 5, 0);
    assert_eq!(s.scroll_top, 0);
    assert_eq!(s.scroll_bottom, 4);
}

#[test]
fn resize_clamps_cursor_and_resets_region() {
    let mut s = Screen::new(10, 5, 0);
    s.cursor.row = 4;
    s.cursor.col = 9;
    s.scroll_top = 1;
    s.scroll_bottom = 3;
    s.resize(4, 2);
    assert_eq!(s.cursor.row, 1);
    assert_eq!(s.cursor.col, 3);
    assert_eq!(s.scroll_top, 0);
    assert_eq!(s.scroll_bottom, 1);
    assert_eq!(s.tabs.len(), 4);
}
