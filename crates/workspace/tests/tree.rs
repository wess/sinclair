use super::*;
use crate::id::PaneIds;

fn ids(n: usize) -> Vec<PaneId> {
    let mut alloc = PaneIds::new();
    (0..n).map(|_| alloc.next()).collect()
}

#[test]
fn new_tree_holds_root() {
    let p = ids(1);
    let tree = PaneTree::new(p[0]);
    assert_eq!(tree.panes(), vec![p[0]]);
    assert!(tree.contains(p[0]));
    assert!(tree.list_dividers().is_empty());
}

#[test]
fn split_orders_panes() {
    let p = ids(3);
    let mut tree = PaneTree::new(p[0]);
    tree.split(p[0], Axis::Horizontal, p[1], false).unwrap();
    assert_eq!(tree.panes(), vec![p[0], p[1]]);
    // new_first puts the new pane before the target.
    tree.split(p[0], Axis::Vertical, p[2], true).unwrap();
    assert_eq!(tree.panes(), vec![p[2], p[0], p[1]]);
}

#[test]
fn split_rejects_missing_target_and_duplicate_pane() {
    let p = ids(3);
    let mut tree = PaneTree::new(p[0]);
    assert!(tree.split(p[1], Axis::Horizontal, p[2], false).is_none());
    assert!(tree.split(p[0], Axis::Horizontal, p[0], false).is_none());
    assert_eq!(tree.panes(), vec![p[0]]);
}

#[test]
fn remove_collapses_chain() {
    let p = ids(3);
    let mut tree = PaneTree::new(p[0]);
    tree.split(p[0], Axis::Horizontal, p[1], false).unwrap();
    tree.split(p[1], Axis::Vertical, p[2], false).unwrap();
    assert_eq!(tree.panes(), vec![p[0], p[1], p[2]]);

    assert!(tree.remove(p[1]));
    assert_eq!(tree.panes(), vec![p[0], p[2]]);
    assert_eq!(tree.list_dividers().len(), 1);

    assert!(tree.remove(p[2]));
    assert_eq!(tree.panes(), vec![p[0]]);
    assert_eq!(tree.root(), &Node::Leaf(p[0]));
}

#[test]
fn remove_inner_split_sibling_subtree_survives() {
    let p = ids(4);
    let mut tree = PaneTree::new(p[0]);
    tree.split(p[0], Axis::Horizontal, p[1], false).unwrap();
    tree.split(p[0], Axis::Vertical, p[2], false).unwrap();
    tree.split(p[2], Axis::Vertical, p[3], false).unwrap();
    assert_eq!(tree.panes(), vec![p[0], p[2], p[3], p[1]]);

    assert!(tree.remove(p[0]));
    assert_eq!(tree.panes(), vec![p[2], p[3], p[1]]);
    assert!(tree.remove(p[3]));
    assert!(tree.remove(p[1]));
    assert_eq!(tree.panes(), vec![p[2]]);
}

#[test]
fn remove_refuses_last_pane_and_missing() {
    let p = ids(2);
    let mut tree = PaneTree::new(p[0]);
    assert!(!tree.remove(p[0]));
    assert!(!tree.remove(p[1]));
    assert!(tree.contains(p[0]));
}

#[test]
fn ratio_clamps() {
    let p = ids(2);
    let mut tree = PaneTree::new(p[0]);
    let s = tree.split(p[0], Axis::Horizontal, p[1], false).unwrap();
    assert_eq!(tree.ratio(s), Some(0.5));
    assert!(tree.set_ratio(s, 0.05));
    assert_eq!(tree.ratio(s), Some(MIN_RATIO));
    assert!(tree.set_ratio(s, 0.95));
    assert_eq!(tree.ratio(s), Some(MAX_RATIO));
    assert!(tree.set_ratio(s, 0.3));
    assert_eq!(tree.ratio(s), Some(0.3));
    assert!(!tree.set_ratio(SplitId(999), 0.5));
}

#[test]
fn nearest_split_picks_closest_matching_axis() {
    // A | (B / C): root horizontal split, right child vertical split.
    let p = ids(4);
    let mut tree = PaneTree::new(p[0]);
    let outer = tree.split(p[0], Axis::Horizontal, p[1], false).unwrap();
    let inner = tree.split(p[1], Axis::Vertical, p[2], false).unwrap();
    // B's nearest vertical divider is the inner split; horizontal is outer.
    assert_eq!(tree.nearest_split(p[1], Axis::Vertical), Some(inner));
    assert_eq!(tree.nearest_split(p[1], Axis::Horizontal), Some(outer));
    // A only sits under the outer horizontal split.
    assert_eq!(tree.nearest_split(p[0], Axis::Horizontal), Some(outer));
    assert_eq!(tree.nearest_split(p[0], Axis::Vertical), None);
    // Missing pane has no split.
    assert_eq!(tree.nearest_split(p[3], Axis::Horizontal), None);
}

#[test]
fn dividers_listed_parent_first() {
    let p = ids(3);
    let mut tree = PaneTree::new(p[0]);
    let outer = tree.split(p[0], Axis::Horizontal, p[1], false).unwrap();
    let inner = tree.split(p[1], Axis::Vertical, p[2], false).unwrap();
    assert_eq!(
        tree.list_dividers(),
        vec![(outer, Axis::Horizontal), (inner, Axis::Vertical)]
    );
}
