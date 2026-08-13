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
