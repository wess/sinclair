use super::*;
use crate::id::PaneIds;

fn ids(n: usize) -> Vec<PaneId> {
    let mut alloc = PaneIds::new();
    (0..n).map(|_| alloc.next()).collect()
}

#[test]
fn single_pane_fills_rect() {
    let p = ids(1);
    let tree = PaneTree::new(p[0]);
    let rect = Rect::new(5.0, 7.0, 100.0, 60.0);
    let layout = compute_layout(&tree, rect, 4.0);
    assert_eq!(layout.panes, vec![(p[0], rect)]);
    assert!(layout.dividers.is_empty());
}

#[test]
fn horizontal_split_with_divider() {
    let p = ids(2);
    let mut tree = PaneTree::new(p[0]);
    let s = tree.split(p[0], Axis::Horizontal, p[1], false).unwrap();
    let layout = compute_layout(&tree, Rect::new(0.0, 0.0, 100.0, 50.0), 4.0);
    assert_eq!(
        layout.pane_rect(p[0]),
        Some(Rect::new(0.0, 0.0, 48.0, 50.0))
    );
    assert_eq!(
        layout.pane_rect(p[1]),
        Some(Rect::new(52.0, 0.0, 48.0, 50.0))
    );
    assert_eq!(
        layout.dividers,
        vec![(s, Rect::new(48.0, 0.0, 4.0, 50.0), Axis::Horizontal)]
    );
}

#[test]
fn vertical_split_with_divider() {
    let p = ids(2);
    let mut tree = PaneTree::new(p[0]);
    let s = tree.split(p[0], Axis::Vertical, p[1], false).unwrap();
    let layout = compute_layout(&tree, Rect::new(0.0, 0.0, 80.0, 110.0), 10.0);
    assert_eq!(
        layout.pane_rect(p[0]),
        Some(Rect::new(0.0, 0.0, 80.0, 50.0))
    );
    assert_eq!(
        layout.pane_rect(p[1]),
        Some(Rect::new(0.0, 60.0, 80.0, 50.0))
    );
    assert_eq!(
        layout.dividers,
        vec![(s, Rect::new(0.0, 50.0, 80.0, 10.0), Axis::Vertical)]
    );
}

#[test]
fn three_deep_nesting_both_axes() {
    // a | (b over (c | d)), all ratios 0.5, no divider thickness.
    let p = ids(4);
    let mut tree = PaneTree::new(p[0]);
    tree.split(p[0], Axis::Horizontal, p[1], false).unwrap();
    tree.split(p[1], Axis::Vertical, p[2], false).unwrap();
    tree.split(p[2], Axis::Horizontal, p[3], false).unwrap();
    let layout = compute_layout(&tree, Rect::new(0.0, 0.0, 100.0, 100.0), 0.0);
    assert_eq!(
        layout.pane_rect(p[0]),
        Some(Rect::new(0.0, 0.0, 50.0, 100.0))
    );
    assert_eq!(
        layout.pane_rect(p[1]),
        Some(Rect::new(50.0, 0.0, 50.0, 50.0))
    );
    assert_eq!(
        layout.pane_rect(p[2]),
        Some(Rect::new(50.0, 50.0, 25.0, 50.0))
    );
    assert_eq!(
        layout.pane_rect(p[3]),
        Some(Rect::new(75.0, 50.0, 25.0, 50.0))
    );
    assert_eq!(layout.dividers.len(), 3);
}

#[test]
fn ratio_change_moves_rects() {
    let p = ids(2);
    let mut tree = PaneTree::new(p[0]);
    let s = tree.split(p[0], Axis::Horizontal, p[1], false).unwrap();
    tree.set_ratio(s, 0.25);
    let layout = compute_layout(&tree, Rect::new(0.0, 0.0, 100.0, 100.0), 0.0);
    assert_eq!(
        layout.pane_rect(p[0]),
        Some(Rect::new(0.0, 0.0, 25.0, 100.0))
    );
    assert_eq!(
        layout.pane_rect(p[1]),
        Some(Rect::new(25.0, 0.0, 75.0, 100.0))
    );

    // Clamped ratio bounds the first pane at 90%.
    tree.set_ratio(s, 1.5);
    let layout = compute_layout(&tree, Rect::new(0.0, 0.0, 100.0, 100.0), 0.0);
    assert_eq!(
        layout.pane_rect(p[0]),
        Some(Rect::new(0.0, 0.0, 90.0, 100.0))
    );
}

#[test]
fn nested_divider_rects() {
    // a | b, then b over c; divider 4, rect 104x104.
    let p = ids(3);
    let mut tree = PaneTree::new(p[0]);
    let outer = tree.split(p[0], Axis::Horizontal, p[1], false).unwrap();
    let inner = tree.split(p[1], Axis::Vertical, p[2], false).unwrap();
    let layout = compute_layout(&tree, Rect::new(0.0, 0.0, 104.0, 104.0), 4.0);
    assert_eq!(
        layout.pane_rect(p[0]),
        Some(Rect::new(0.0, 0.0, 50.0, 104.0))
    );
    assert_eq!(
        layout.pane_rect(p[1]),
        Some(Rect::new(54.0, 0.0, 50.0, 50.0))
    );
    assert_eq!(
        layout.pane_rect(p[2]),
        Some(Rect::new(54.0, 54.0, 50.0, 50.0))
    );
    assert_eq!(
        layout.dividers,
        vec![
            (outer, Rect::new(50.0, 0.0, 4.0, 104.0), Axis::Horizontal),
            (inner, Rect::new(54.0, 50.0, 50.0, 4.0), Axis::Vertical),
        ]
    );
    // Divider order matches the tree's listing.
    let listed: Vec<_> = layout.dividers.iter().map(|(s, _, a)| (*s, *a)).collect();
    assert_eq!(listed, tree.list_dividers());
}
