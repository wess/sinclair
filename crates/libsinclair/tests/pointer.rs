use super::*;

#[test]
fn web_and_comms_schemes_are_openable() {
  assert!(openable("https://example.com"));
  assert!(openable("http://example.com/path"));
  assert!(openable("ftp://host/file"));
  assert!(openable("mailto:me@example.com"));
  assert!(openable("file:///etc/hosts"));
  // Scheme matching is case-insensitive.
  assert!(openable("HTTPS://example.com"));
}

#[test]
fn dangerous_and_unknown_schemes_are_refused() {
  assert!(!openable("javascript:alert(1)"));
  assert!(!openable("data:text/html,<script>"));
  assert!(!openable("vbscript:msgbox"));
  assert!(!openable("customapp://do-something"));
}

#[test]
fn schemeless_or_pathlike_targets_are_refused() {
  assert!(!openable("example.com"));
  assert!(!openable("/usr/local/bin"));
  assert!(!openable("./relative/path:with-colon"));
  assert!(!openable(""));
}

#[test]
fn link_open_requires_a_local_undragged_press_inside_bounds() {
  // The plain cmd+click: press and release in this pane, no drag.
  assert!(opens_link(true, true, false));
  // Release outside the pane (or an event replayed into a sibling pane in
  // a split): never open.
  assert!(!opens_link(false, true, false));
  // The press never landed here — a sibling pane's handler seeing the
  // same window-level release.
  assert!(!opens_link(true, false, false));
  // A cmd+drag is a selection gesture; releasing over a link must not
  // open it (and must leave the selection alone).
  assert!(!opens_link(true, true, true));
}
