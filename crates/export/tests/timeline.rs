use super::*;

#[test]
fn empty_and_single() {
  assert!(adjust(&[], 1.0, None).is_empty());
  assert_eq!(adjust(&[4.2], 1.0, None), vec![0.0]);
}

#[test]
fn preserves_gaps_without_cap() {
  // First event anchors to 0; later gaps carry through at speed 1.
  assert_eq!(adjust(&[0.0, 1.0, 3.0], 1.0, None), vec![0.0, 1.0, 3.0]);
}

#[test]
fn caps_idle_gaps() {
  // A 5s gap is clamped to the 2s cap.
  assert_eq!(adjust(&[0.0, 5.0], 1.0, Some(2.0)), vec![0.0, 2.0]);
}

#[test]
fn speed_scales_gaps() {
  // Double speed halves every gap.
  assert_eq!(adjust(&[0.0, 4.0], 2.0, None), vec![0.0, 2.0]);
}

#[test]
fn cap_applies_before_speed() {
  // Gap 6 -> capped to 2 -> /2 speed = 1.
  assert_eq!(adjust(&[0.0, 6.0], 2.0, Some(2.0)), vec![0.0, 1.0]);
}

#[test]
fn non_monotonic_gap_is_clamped_to_zero() {
  // A backwards timestamp never rewinds playback.
  assert_eq!(adjust(&[0.0, 2.0, 1.0], 1.0, None), vec![0.0, 2.0, 2.0]);
}
