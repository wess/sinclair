use super::*;
use crate::id::ItemIds;

fn items(n: usize) -> Vec<ItemId> {
    let mut alloc = ItemIds::new();
    (0..n).map(|_| alloc.next()).collect()
}

#[test]
fn new_pane_holds_one_active_item() {
    let it = items(1);
    let pane = Pane::new(it[0]);
    assert_eq!(pane.items(), &[it[0]]);
    assert_eq!(pane.active(), it[0]);
    assert_eq!(pane.active_index(), 0);
    assert!(!pane.is_empty());
}

#[test]
fn add_inserts_and_activates() {
    let it = items(3);
    let mut pane = Pane::new(it[0]);
    assert_eq!(pane.add(it[1], None), 1); // appended, active
    assert_eq!(pane.active(), it[1]);
    assert_eq!(pane.add(it[2], Some(0)), 0); // inserted first, active
    assert_eq!(pane.items(), &[it[2], it[0], it[1]]);
    assert_eq!(pane.active(), it[2]);
    // Re-adding an existing item just activates it.
    assert_eq!(pane.add(it[1], None), 2);
    assert_eq!(pane.active(), it[1]);
    assert_eq!(pane.len(), 3);
}

#[test]
fn remove_keeps_active_valid_and_reports_empty() {
    let it = items(3);
    let mut pane = Pane::new(it[0]);
    pane.add(it[1], None);
    pane.add(it[2], None); // items [0,1,2], active=2
    // Remove an item before the active one: active shifts down.
    assert!(!pane.remove(it[0]));
    assert_eq!(pane.items(), &[it[1], it[2]]);
    assert_eq!(pane.active(), it[2]);
    // Remove the active (last): active clamps.
    assert!(!pane.remove(it[2]));
    assert_eq!(pane.active(), it[1]);
    // Remove the last remaining: pane is now empty.
    assert!(pane.remove(it[1]));
    assert!(pane.is_empty());
}

#[test]
fn reorder_preserves_active() {
    let it = items(3);
    let mut pane = Pane::new(it[0]);
    pane.add(it[1], None);
    pane.add(it[2], None); // [0,1,2], active=2 (it[2])
    assert!(pane.reorder(0, 2)); // move it[0] to end
    assert_eq!(pane.items(), &[it[1], it[2], it[0]]);
    // it[2] is still the active item, at its new index.
    assert_eq!(pane.active(), it[2]);
}

#[test]
fn activate_next_prev_wrap() {
    let it = items(3);
    let mut pane = Pane::new(it[0]);
    pane.add(it[1], None); // active=1
    pane.activate_next();
    assert_eq!(pane.active(), it[0]); // wrapped
    pane.activate_prev();
    assert_eq!(pane.active(), it[1]);
    assert!(pane.activate_item(it[0]));
    assert_eq!(pane.active_index(), 0);
    assert!(!pane.activate_item(it[2])); // an item not in this pane
}
