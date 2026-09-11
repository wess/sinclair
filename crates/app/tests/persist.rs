use super::*;

/// The saved buffers are bounded twice: by `session-restore-lines` and by the
/// window's share of the byte budget. The share is what keeps a window full
/// of dense panes from writing a session file nobody wants to parse.
#[test]
fn the_byte_budget_is_shared_evenly_between_panes() {
  let share = |panes: usize| (MAX_SESSION_BUFFER_BYTES / panes.max(1)).min(MAX_PANE_BUFFER_BYTES);
  // A lone pane gets its own ceiling, not the whole window budget.
  assert_eq!(share(1), MAX_PANE_BUFFER_BYTES);
  assert_eq!(share(4), MAX_PANE_BUFFER_BYTES);
  // Past that the panes divide the window budget between them.
  assert_eq!(share(8), MAX_SESSION_BUFFER_BYTES / 8);
  assert!(share(32) * 32 <= MAX_SESSION_BUFFER_BYTES);
  // No pane count divides by zero or exceeds the total.
  assert!(share(0) <= MAX_PANE_BUFFER_BYTES);
}
