use super::*;
use workspace::PaneIds;

fn ids(n: usize) -> Vec<PaneId> {
    let mut alloc = PaneIds::new();
    (0..n).map(|_| alloc.next()).collect()
}

#[test]
fn splitrect_root_is_full_rect() {
    let p = ids(2);
    let mut tree = PaneTree::new(p[0]);
    let s = tree.split(p[0], Axis::Horizontal, p[1], false).unwrap();
    let rect = Rect::new(10.0, 20.0, 100.0, 50.0);
    assert_eq!(splitrect(&tree, rect, 2.0, s), Some(rect));
}

#[test]
fn splitrect_nested_matches_compute_layout() {
    // a | (b over c): the inner split spans exactly b + divider + c.
    let p = ids(3);
    let mut tree = PaneTree::new(p[0]);
    let outer = tree.split(p[0], Axis::Horizontal, p[1], false).unwrap();
    let inner = tree.split(p[1], Axis::Vertical, p[2], false).unwrap();
    let rect = Rect::new(0.0, 0.0, 104.0, 104.0);
    let layout = compute_layout(&tree, rect, 4.0);

    assert_eq!(splitrect(&tree, rect, 4.0, outer), Some(rect));
    let irect = splitrect(&tree, rect, 4.0, inner).expect("inner rect");
    let b = layout.pane_rect(p[1]).unwrap();
    let c = layout.pane_rect(p[2]).unwrap();
    assert_eq!((irect.x, irect.y), (b.x, b.y));
    assert_eq!(irect.w, b.w);
    assert_eq!(irect.h, b.h + 4.0 + c.h);
    // The inner divider sits inside the inner split's rect.
    let (_, drect, _) = layout.dividers[1];
    assert!(drect.y >= irect.y && drect.y + drect.h <= irect.y + irect.h);
}

#[test]
fn splitrect_honors_nondefault_ratio() {
    let p = ids(3);
    let mut tree = PaneTree::new(p[0]);
    let outer = tree.split(p[0], Axis::Horizontal, p[1], false).unwrap();
    tree.set_ratio(outer, 0.25);
    let inner = tree.split(p[1], Axis::Vertical, p[2], false).unwrap();
    let rect = Rect::new(0.0, 0.0, 102.0, 60.0);
    // avail = 100, first = 25, divider 2: inner starts at x = 27.
    let irect = splitrect(&tree, rect, 2.0, inner).expect("inner");
    assert_eq!(irect, Rect::new(27.0, 0.0, 75.0, 60.0));
}

#[test]
fn splitrect_missing_split_is_none() {
    // Mint an id the queried trees never allocated: a second split in
    // a throwaway tree (per-tree counters make it id 2).
    let p = ids(3);
    let mut donor = PaneTree::new(p[0]);
    donor.split(p[0], Axis::Horizontal, p[1], false).unwrap();
    let foreign = donor.split(p[1], Axis::Vertical, p[2], false).unwrap();

    let rect = Rect::new(0.0, 0.0, 10.0, 10.0);
    let mut tree = PaneTree::new(p[0]);
    tree.split(p[0], Axis::Horizontal, p[1], false).unwrap();
    assert_eq!(splitrect(&tree, rect, 1.0, foreign), None);
    let single = PaneTree::new(p[0]);
    assert_eq!(splitrect(&single, rect, 1.0, foreign), None);
}

#[test]
fn dragratio_tracks_pointer_horizontally() {
    let rect = Rect::new(0.0, 0.0, 102.0, 50.0);
    // Pointer at the divider center for ratio 0.5: x = 50 + 1.
    let r = dragratio(rect, Axis::Horizontal, 2.0, (51.0, 0.0));
    assert!((r - 0.5).abs() < 1e-6);
    let r = dragratio(rect, Axis::Horizontal, 2.0, (26.0, 999.0));
    assert!((r - 0.25).abs() < 1e-6);
}

#[test]
fn dragratio_vertical_uses_y_and_offset() {
    let rect = Rect::new(0.0, 100.0, 80.0, 102.0);
    let r = dragratio(rect, Axis::Vertical, 2.0, (0.0, 151.0));
    assert!((r - 0.5).abs() < 1e-6);
}

#[test]
fn dragratio_clamps_to_bounds() {
    let rect = Rect::new(0.0, 0.0, 102.0, 50.0);
    assert_eq!(dragratio(rect, Axis::Horizontal, 2.0, (-50.0, 0.0)), 0.1);
    assert_eq!(dragratio(rect, Axis::Horizontal, 2.0, (500.0, 0.0)), 0.9);
}
