use super::*;

#[test]
fn ids_are_unique_and_monotonic() {
    let mut ids = PaneIds::new();
    let a = ids.next();
    let b = ids.next();
    let c = ids.next();
    assert_ne!(a, b);
    assert_ne!(b, c);
    assert!(a < b && b < c);
}
