use super::*;

#[test]
fn fresh_tracker_is_full() {
    let mut d = DamageTracker::new();
    assert!(d.is_full());
    assert_eq!(d.take(), Damage::Full);
    assert_eq!(d.take(), Damage::Rows(vec![]));
}

#[test]
fn rows_accumulate_sorted_and_deduped() {
    let mut d = DamageTracker::new();
    d.take();
    d.mark_row(5);
    d.mark_row(1);
    d.mark_row(5);
    assert_eq!(d.take(), Damage::Rows(vec![1, 5]));
    assert_eq!(d.take(), Damage::Rows(vec![]));
}

#[test]
fn full_overrides_rows() {
    let mut d = DamageTracker::new();
    d.take();
    d.mark_row(2);
    d.mark_full();
    assert_eq!(d.take(), Damage::Full);
    assert_eq!(d.take(), Damage::Rows(vec![]));
}

#[test]
fn mark_row_while_full_is_noop() {
    let mut d = DamageTracker::new();
    d.mark_row(3);
    assert_eq!(d.take(), Damage::Full);
    assert_eq!(d.take(), Damage::Rows(vec![]));
}

#[test]
fn is_empty() {
    assert!(Damage::Rows(vec![]).is_empty());
    assert!(!Damage::Rows(vec![0]).is_empty());
    assert!(!Damage::Full.is_empty());
}
