use super::*;

fn bounds(width: f32, height: f32) -> WindowState {
  WindowState {
    x: 100.0,
    y: 80.0,
    width,
    height,
  }
}

#[test]
fn usable_accepts_ordinary_geometry() {
  assert!(bounds(1200.0, 800.0).usable());
  // Negative origins are fine: a second display can sit left of or above the
  // primary one, and those windows have negative coordinates.
  assert!(WindowState {
    x: -1800.0,
    y: -400.0,
    width: 900.0,
    height: 600.0
  }
  .usable());
}

#[test]
fn usable_rejects_degenerate_and_absurd_sizes() {
  assert!(!bounds(0.0, 0.0).usable());
  assert!(!bounds(-10.0, 600.0).usable());
  // Below the window minimum: restoring this would open something unusable.
  assert!(!bounds(120.0, 600.0).usable());
  // Only a corrupt file produces these.
  assert!(!bounds(50_000.0, 600.0).usable());
}

/// A session written before window geometry and dock state existed must still
/// load — the fields are optional, and their absence means "use the config".
#[test]
fn an_older_session_without_window_or_docks_still_loads() {
  let json = r#"{"tabs":[],"active":0}"#;
  let state: SessionState = serde_json::from_str(json).expect("older session");
  assert!(state.window.is_none());
  assert!(state.docks.is_none());
}

#[test]
fn window_and_dock_state_round_trip() {
  let state = SessionState {
    tabs: Vec::new(),
    active: 0,
    window: Some(bounds(1440.0, 900.0)),
    docks: Some([
      DockState {
        open: true,
        width: 320.0,
        sections: vec![("terminals".into(), false)],
      },
      DockState::default(),
    ]),
  };
  let json = serde_json::to_string(&state).unwrap();
  let back: SessionState = serde_json::from_str(&json).unwrap();
  let w = back.window.unwrap();
  assert_eq!((w.x, w.y, w.width, w.height), (100.0, 80.0, 1440.0, 900.0));
  let docks = back.docks.unwrap();
  assert!(docks[0].open);
  assert_eq!(docks[0].width, 320.0);
  assert_eq!(docks[0].sections, vec![("terminals".to_string(), false)]);
  assert!(!docks[1].open);
}

/// A session written before pane buffers were saved still loads; those tabs
/// simply restore with empty panes, as they always did.
#[test]
fn an_older_session_without_buffers_still_loads() {
  let json = r#"{"tabs":[{"layout":{"t":"leaf"},"cwds":["/tmp"]}],"active":0}"#;
  let state: SessionState = serde_json::from_str(json).expect("older session");
  assert!(state.tabs[0].buffers.is_empty());
  assert_eq!(state.tabs[0].cwds, vec![Some("/tmp".to_string())]);
}

#[test]
fn pane_buffers_round_trip() {
  let state = SessionState {
    tabs: vec![TabState {
      layout: Layout::Leaf,
      cwds: vec![Some("/tmp".into()), None],
      title: None,
      commands: vec![None, None],
      sessions: vec![None, None],
      buffers: vec![Some("\u{1b}[31mred\u{1b}[0m\r\n".into()), None],
    }],
    active: 0,
    window: None,
    docks: None,
  };
  let json = serde_json::to_string(&state).unwrap();
  let back: SessionState = serde_json::from_str(&json).unwrap();
  assert_eq!(
    back.tabs[0].buffers,
    vec![Some("\u{1b}[31mred\u{1b}[0m\r\n".to_string()), None]
  );
}

fn tmpdir(tag: &str) -> std::path::PathBuf {
  let dir = std::env::temp_dir().join(format!("sinclairsession{tag}{}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  std::fs::create_dir_all(&dir).unwrap();
  dir
}

/// The session file holds terminal output, so it goes down owner-only and
/// through a rename: an interrupted save must not leave a partial file where
/// the last good one was.
#[test]
fn persist_is_atomic_and_owner_only() {
  let dir = tmpdir("a");
  let path = dir.join("session.json");
  std::fs::write(&path, b"{\"tabs\":[]}").unwrap();
  persist(&path, b"{\"tabs\":[],\"active\":0}");
  assert_eq!(
    std::fs::read_to_string(&path).unwrap(),
    "{\"tabs\":[],\"active\":0}"
  );
  let leftovers: Vec<_> = std::fs::read_dir(&dir)
    .unwrap()
    .filter_map(|e| e.ok())
    .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
    .collect();
  assert!(leftovers.is_empty(), "temp file left behind");
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(&path).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600, "session file is not owner-only");
  }
  let _ = std::fs::remove_dir_all(&dir);
}
