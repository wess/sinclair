use super::*;

#[test]
fn jump_clears_the_find_bar() {
  // A match deep in scrollback lands JUMP_CLEARANCE rows below the top,
  // out from under the full-width find bar.
  let sb = 100;
  let line = 40;
  let offset = jump_offset(sb, line);
  assert_eq!(offset, sb - line + JUMP_CLEARANCE);
  // Viewport top shows global row `sb - offset`; the match sits below it.
  assert_eq!(line - (sb - offset), JUMP_CLEARANCE);
}

#[test]
fn jump_clamps_at_the_oldest_rows() {
  // The first scrollback rows can't be pushed down further than history
  // allows; the offset pins at the top instead of overflowing.
  assert_eq!(jump_offset(100, 0), 100);
  assert_eq!(jump_offset(100, 1), 100);
  assert_eq!(jump_offset(100, 2), 100);
  assert_eq!(jump_offset(100, 3), 99);
}

#[test]
fn live_screen_matches_stay_put() {
  // Matches on the live grid (line >= scrollback) keep the live view.
  assert_eq!(jump_offset(100, 100), 0);
  assert_eq!(jump_offset(100, 140), 0);
  assert_eq!(jump_offset(0, 0), 0);
}
