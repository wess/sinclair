use super::*;

#[test]
fn oldest_seq_tracks_eviction() {
  // Nothing evicted while the ring is filling: everything back to 0 lives.
  assert_eq!(oldest_seq(0, 0), 0);
  assert_eq!(oldest_seq(500, 500), 0);
  // Once committed outruns the ring, the difference has been evicted.
  assert_eq!(oldest_seq(10_500, 10_000), 500);
  // Live-row sequences (committed + r) always stay above the floor.
  assert!(oldest_seq(10_500, 10_000) <= 10_500);
}
