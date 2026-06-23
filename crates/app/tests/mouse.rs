use super::*;

#[test]
fn reporting_requires_mode_and_no_shift() {
    assert!(!reports(MouseMode::None, false));
    assert!(reports(MouseMode::Click, false));
    assert!(reports(MouseMode::Drag, false));
    assert!(reports(MouseMode::Motion, false));
    // Shift always bypasses reporting.
    assert!(!reports(MouseMode::Motion, true));
    assert!(!reports(MouseMode::Click, true));
}

#[test]
fn motion_reporting_per_mode() {
    let held = Some(MouseButton::Left);
    assert!(!reports_motion(MouseMode::None, held));
    assert!(!reports_motion(MouseMode::Click, held));
    assert!(reports_motion(MouseMode::Drag, held));
    assert!(!reports_motion(MouseMode::Drag, None));
    assert!(reports_motion(MouseMode::Motion, held));
    assert!(reports_motion(MouseMode::Motion, None));
}

#[test]
fn click_counts_map_to_modes() {
    assert_eq!(click_mode(0), SelectionMode::Cell); // defensive
    assert_eq!(click_mode(1), SelectionMode::Cell);
    assert_eq!(click_mode(2), SelectionMode::Word);
    assert_eq!(click_mode(3), SelectionMode::Line);
    // Rapid clicking cycles.
    assert_eq!(click_mode(4), SelectionMode::Cell);
    assert_eq!(click_mode(5), SelectionMode::Word);
    assert_eq!(click_mode(6), SelectionMode::Line);
}

#[test]
fn wheel_routing_precedence() {
    use WheelRoute::*;
    // Reporting wins over everything when active and shift is up.
    assert_eq!(route_wheel(MouseMode::Click, false, true, true), Report);
    assert_eq!(route_wheel(MouseMode::Motion, false, false, false), Report);
    // Shift bypasses reporting; alt scroll applies on the alt screen.
    assert_eq!(route_wheel(MouseMode::Motion, true, true, true), Display);
    assert_eq!(route_wheel(MouseMode::None, false, true, true), Arrows);
    // Alt scroll needs both the alt screen and the mode.
    assert_eq!(route_wheel(MouseMode::None, false, true, false), Display);
    assert_eq!(route_wheel(MouseMode::None, false, false, true), Display);
    assert_eq!(route_wheel(MouseMode::None, false, false, false), Display);
}

#[test]
fn wheel_lines_accumulates_fractions() {
    let mut acc = 0.0;
    assert_eq!(wheel_lines(&mut acc, 0.4), 0);
    assert_eq!(wheel_lines(&mut acc, 0.4), 0);
    assert_eq!(wheel_lines(&mut acc, 0.4), 1); // 1.2 -> 1, carry 0.2
    assert!((acc - 0.2).abs() < 1e-6);
    assert_eq!(wheel_lines(&mut acc, 2.0), 2);
}

#[test]
fn wheel_lines_whole_deltas_pass_through() {
    let mut acc = 0.0;
    assert_eq!(wheel_lines(&mut acc, 3.0), 3);
    assert_eq!(wheel_lines(&mut acc, -2.0), -2);
    assert_eq!(acc, 0.0);
}

#[test]
fn wheel_lines_direction_change_drops_fraction() {
    let mut acc = 0.0;
    assert_eq!(wheel_lines(&mut acc, 0.9), 0);
    // Reversing direction must not fight the stale +0.9.
    assert_eq!(wheel_lines(&mut acc, -1.0), -1);
    assert_eq!(acc, 0.0);
}
