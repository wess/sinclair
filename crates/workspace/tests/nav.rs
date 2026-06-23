use super::*;
use crate::id::PaneIds;
use crate::layout::compute_layout;
use crate::tree::Axis;

fn ids(n: usize) -> Vec<PaneId> {
    let mut alloc = PaneIds::new();
    (0..n).map(|_| alloc.next()).collect()
}

/// a b
/// c d
fn grid(divider: f32) -> (Vec<PaneId>, PaneTree, Layout) {
    let p = ids(4);
    let mut tree = PaneTree::new(p[0]);
    tree.split(p[0], Axis::Horizontal, p[1], false).unwrap();
    tree.split(p[0], Axis::Vertical, p[2], false).unwrap();
    tree.split(p[1], Axis::Vertical, p[3], false).unwrap();
    let layout = compute_layout(&tree, Rect::new(0.0, 0.0, 100.0, 100.0), divider);
    (p, tree, layout)
}

#[test]
fn grid_navigation_from_every_pane() {
    let (p, _, layout) = grid(0.0);
    let (a, b, c, d) = (p[0], p[1], p[2], p[3]);
    let cases = [
        (a, Direction::Right, Some(b)),
        (a, Direction::Down, Some(c)),
        (a, Direction::Left, None),
        (a, Direction::Up, None),
        (b, Direction::Left, Some(a)),
        (b, Direction::Down, Some(d)),
        (b, Direction::Right, None),
        (b, Direction::Up, None),
        (c, Direction::Up, Some(a)),
        (c, Direction::Right, Some(d)),
        (c, Direction::Down, None),
        (c, Direction::Left, None),
        (d, Direction::Up, Some(b)),
        (d, Direction::Left, Some(c)),
        (d, Direction::Down, None),
        (d, Direction::Right, None),
    ];
    for (from, dir, expected) in cases {
        assert_eq!(neighbor(&layout, from, dir), expected, "{from:?} {dir:?}");
    }
}

#[test]
fn grid_navigation_with_divider_thickness() {
    let (p, _, layout) = grid(4.0);
    assert_eq!(neighbor(&layout, p[0], Direction::Right), Some(p[1]));
    assert_eq!(neighbor(&layout, p[3], Direction::Up), Some(p[1]));
    assert_eq!(neighbor(&layout, p[3], Direction::Left), Some(p[2]));
}

#[test]
fn nearest_center_breaks_column_ties() {
    // Left pane spans full height; right side is b over c with c larger.
    let p = ids(3);
    let mut tree = PaneTree::new(p[0]);
    tree.split(p[0], Axis::Horizontal, p[1], false).unwrap();
    let inner = tree.split(p[1], Axis::Vertical, p[2], false).unwrap();
    tree.set_ratio(inner, 0.2);
    let layout = compute_layout(&tree, Rect::new(0.0, 0.0, 100.0, 100.0), 0.0);
    // a's center y = 50; b center y = 10, c center y = 60 -> picks c.
    assert_eq!(neighbor(&layout, p[0], Direction::Right), Some(p[2]));
    assert_eq!(neighbor(&layout, p[1], Direction::Left), Some(p[0]));
    assert_eq!(neighbor(&layout, p[2], Direction::Up), Some(p[1]));
}

#[test]
fn next_prev_wrap_in_layout_order() {
    let (p, tree, _) = grid(0.0);
    // Layout order: a, c, b, d.
    assert_eq!(next(&tree, p[0]), Some(p[2]));
    assert_eq!(next(&tree, p[2]), Some(p[1]));
    assert_eq!(next(&tree, p[1]), Some(p[3]));
    assert_eq!(next(&tree, p[3]), Some(p[0]));
    assert_eq!(prev(&tree, p[0]), Some(p[3]));
    assert_eq!(prev(&tree, p[3]), Some(p[1]));
}

#[test]
fn unknown_pane_yields_none() {
    let (p, tree, layout) = grid(0.0);
    let stranger = ids(5)[4];
    assert!(!p.contains(&stranger));
    assert_eq!(neighbor(&layout, stranger, Direction::Left), None);
    assert_eq!(next(&tree, stranger), None);
    assert_eq!(prev(&tree, stranger), None);
}
