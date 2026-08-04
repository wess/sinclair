use super::*;

fn at(x: f32, y: f32, w: f32, h: f32) -> Bounds<Pixels> {
    Bounds::new(point(px(x), px(y)), size(px(w), px(h)))
}

/// A 1920x1080 primary display.
fn display() -> Bounds<Pixels> {
    at(0.0, 0.0, 1920.0, 1080.0)
}

#[test]
fn cascade_steps_off_the_source_and_keeps_its_size() {
    let placed = cascade(at(100.0, 100.0, 800.0, 600.0), display());
    assert_eq!(placed.origin, point(px(136.0), px(136.0)));
    assert_eq!(placed.size, size(px(800.0), px(600.0)));
}

#[test]
fn cascade_restarts_when_the_step_runs_past_an_edge() {
    // Right edge: x would land at 1136 with only 1120 of room.
    let placed = cascade(at(1100.0, 100.0, 800.0, 600.0), display());
    assert_eq!(placed.origin, point(px(36.0), px(36.0)));
    // Bottom edge.
    let placed = cascade(at(100.0, 460.0, 800.0, 600.0), display());
    assert_eq!(placed.origin, point(px(36.0), px(36.0)));
}

#[test]
fn cascade_restarts_for_a_window_larger_than_its_display() {
    let placed = cascade(at(0.0, 0.0, 2400.0, 1400.0), display());
    assert_eq!(placed.origin, point(px(36.0), px(36.0)));
    assert_eq!(placed.size, size(px(2400.0), px(1400.0)));
}

#[test]
fn dropped_puts_the_window_where_the_ghost_was() {
    let placed = dropped(
        point(px(900.0), px(300.0)),
        point(px(40.0), px(12.0)),
        size(px(800.0), px(600.0)),
        display(),
    );
    assert_eq!(placed.origin, point(px(860.0), px(288.0)));
    assert_eq!(placed.size, size(px(800.0), px(600.0)));
}

#[test]
fn dropped_keeps_a_tear_off_the_edge_on_screen() {
    // Released past the right edge: the window slides back to sit flush.
    let placed = dropped(
        point(px(1900.0), px(1060.0)),
        point(px(40.0), px(12.0)),
        size(px(800.0), px(600.0)),
        display(),
    );
    assert_eq!(placed.origin, point(px(1120.0), px(480.0)));
    // Released past the top-left corner.
    let placed = dropped(
        point(px(10.0), px(4.0)),
        point(px(40.0), px(12.0)),
        size(px(800.0), px(600.0)),
        display(),
    );
    assert_eq!(placed.origin, point(px(0.0), px(0.0)));
}

#[test]
fn dropped_pins_a_window_larger_than_its_display_to_the_corner() {
    let placed = dropped(
        point(px(900.0), px(300.0)),
        point(px(40.0), px(12.0)),
        size(px(2400.0), px(1400.0)),
        display(),
    );
    assert_eq!(placed.origin, point(px(0.0), px(0.0)));
}

#[test]
fn dropped_is_relative_to_a_secondary_display() {
    let display = Bounds::new(point(px(1920.0), px(0.0)), size(px(1920.0), px(1080.0)));
    let placed = dropped(
        point(px(2500.0), px(300.0)),
        point(px(40.0), px(12.0)),
        size(px(800.0), px(600.0)),
        display,
    );
    assert_eq!(placed.origin, point(px(2460.0), px(288.0)));
    // Near that display's left edge it stops at *its* corner, not 0,0.
    let placed = dropped(
        point(px(1930.0), px(300.0)),
        point(px(40.0), px(12.0)),
        size(px(800.0), px(600.0)),
        display,
    );
    assert_eq!(placed.origin, point(px(1920.0), px(288.0)));
}

#[test]
fn cascade_is_relative_to_a_secondary_display() {
    let display = Bounds::new(point(px(1920.0), px(0.0)), size(px(1920.0), px(1080.0)));
    let placed = cascade(at(2000.0, 200.0, 800.0, 600.0), display);
    assert_eq!(placed.origin, point(px(2036.0), px(236.0)));
    // A window near that display's right edge restarts at *its* corner, not 0,0.
    let placed = cascade(at(3020.0, 200.0, 800.0, 600.0), display);
    assert_eq!(placed.origin, point(px(1956.0), px(36.0)));
}
