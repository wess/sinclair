use super::*;
use crate::id::PaneIds;
use crate::tree::Axis;

fn ids(n: usize) -> Vec<PaneId> {
    let mut alloc = PaneIds::new();
    (0..n).map(|_| alloc.next()).collect()
}

#[test]
fn starts_with_one_active_tab() {
    let p = ids(1);
    let tabs = Tabs::new(p[0]);
    assert_eq!(tabs.len(), 1);
    assert_eq!(tabs.active_index(), 0);
    assert_eq!(tabs.focused(), p[0]);
    assert!(!tabs.is_empty());
}

#[test]
fn new_tab_appends_and_activates() {
    let p = ids(3);
    let mut tabs = Tabs::new(p[0]);
    assert_eq!(tabs.new_tab(p[1]), 1);
    assert_eq!(tabs.new_tab(p[2]), 2);
    assert_eq!(tabs.active_index(), 2);
    assert_eq!(tabs.focused(), p[2]);
}

#[test]
fn close_tab_refuses_last_and_out_of_range() {
    let p = ids(2);
    let mut tabs = Tabs::new(p[0]);
    assert!(!tabs.close_tab(0));
    tabs.new_tab(p[1]);
    assert!(!tabs.close_tab(5));
    assert!(tabs.close_tab(1));
    assert_eq!(tabs.len(), 1);
    assert!(!tabs.close_tab(0));
}

#[test]
fn close_tab_adjusts_active_index() {
    let p = ids(3);
    let mut tabs = Tabs::new(p[0]);
    tabs.new_tab(p[1]);
    tabs.new_tab(p[2]);

    // Closing before the active tab shifts it left.
    tabs.activate(2);
    assert!(tabs.close_tab(0));
    assert_eq!(tabs.active_index(), 1);
    assert_eq!(tabs.focused(), p[2]);

    // Closing the active last tab clamps to the new end.
    assert!(tabs.close_tab(1));
    assert_eq!(tabs.active_index(), 0);
    assert_eq!(tabs.focused(), p[1]);
}

#[test]
fn activate_and_cycling_wrap() {
    let p = ids(3);
    let mut tabs = Tabs::new(p[0]);
    tabs.new_tab(p[1]);
    tabs.new_tab(p[2]);
    assert!(tabs.activate(0));
    assert!(!tabs.activate(3));
    assert_eq!(tabs.active_index(), 0);
    tabs.activate_prev();
    assert_eq!(tabs.active_index(), 2);
    tabs.activate_next();
    assert_eq!(tabs.active_index(), 0);
    tabs.activate_next();
    assert_eq!(tabs.active_index(), 1);
}

#[test]
fn move_tab_reorders_and_tracks_active() {
    let p = ids(3);
    let mut tabs = Tabs::new(p[0]);
    tabs.new_tab(p[1]);
    tabs.new_tab(p[2]); // order: 0,1,2; active 2.

    // Moving the active tab keeps it active at its new index.
    assert!(tabs.move_tab(2, 0)); // order: 2,0,1.
    assert_eq!(tabs.active_index(), 0);
    assert_eq!(tabs.focused(), p[2]);

    // Moving another tab across the active one shifts the index.
    assert!(tabs.move_tab(2, 0)); // order: 1,2,0; active follows p[2] to 1.
    assert_eq!(tabs.active_index(), 1);
    assert_eq!(tabs.focused(), p[2]);
    assert_eq!(tabs.get(0).unwrap().focused, p[1]);
    assert_eq!(tabs.get(2).unwrap().focused, p[0]);

    assert!(!tabs.move_tab(0, 9));
    assert!(!tabs.move_tab(9, 0));
    assert!(tabs.move_tab(1, 1));
}

#[test]
fn focus_only_panes_in_active_tab() {
    let p = ids(3);
    let mut tabs = Tabs::new(p[0]);
    tabs.active_mut()
        .tree
        .split(p[0], Axis::Horizontal, p[1], false)
        .unwrap();
    assert!(tabs.focus(p[1]));
    assert_eq!(tabs.focused(), p[1]);
    assert!(!tabs.focus(p[2]));
    assert_eq!(tabs.focused(), p[1]);

    tabs.new_tab(p[2]);
    assert_eq!(tabs.focused(), p[2]);
    assert!(!tabs.focus(p[0])); // p[0] lives in the other tab.
    tabs.activate(0);
    assert_eq!(tabs.focused(), p[1]); // per-tab focus is remembered.
}
