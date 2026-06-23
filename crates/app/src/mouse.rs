//! Pure mouse policy: when to report to the pty, how clicks map to
//! selection modes, how wheel deltas become whole lines. No gpui types so
//! everything here is unit-testable.

use input::MouseButton;
use vt::{MouseMode, SelectionMode};

/// Cross-event pointer state, owned by the view and shared with the
/// element's per-frame event closures.
#[derive(Debug, Default)]
pub struct MouseState {
    /// A left-button selection gesture is in progress.
    pub selecting: bool,
    /// The selection gesture extended past its starting cell (or began as
    /// a multi-click), so it survives release.
    pub dragged: bool,
    /// Cell where the selection press landed, to tell click from drag.
    pub pressed: Option<(usize, usize)>,
    /// Button held while reporting to the pty (press sent, release owed).
    pub report_button: Option<MouseButton>,
    /// Last cell reported as motion, for coalescing duplicates.
    pub last_motion: Option<(usize, usize)>,
    /// Fractional wheel lines carried between scroll events.
    pub wheel: f32,
}

/// Whether mouse events go to the pty instead of driving selection.
/// Shift always reclaims the mouse for the terminal user.
pub fn reports(mode: MouseMode, shift: bool) -> bool {
    mode != MouseMode::None && !shift
}

/// Whether a motion event is reported, given the button currently held:
/// Click mode never reports motion, Drag only while a button is down,
/// Motion always.
pub fn reports_motion(mode: MouseMode, held: Option<MouseButton>) -> bool {
    match mode {
        MouseMode::Motion => true,
        MouseMode::Drag => held.is_some(),
        MouseMode::Click | MouseMode::None => false,
    }
}

/// Click count to selection mode: single = cell, double = word, triple =
/// line; further rapid clicks cycle.
pub fn click_mode(count: usize) -> SelectionMode {
    match (count.max(1) - 1) % 3 {
        0 => SelectionMode::Cell,
        1 => SelectionMode::Word,
        _ => SelectionMode::Line,
    }
}

/// Where a wheel event goes. Mouse reporting wins (shift bypasses), then
/// alternate scroll on the alt screen, else the display scrolls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WheelRoute {
    /// Encode wheel presses for the pty.
    Report,
    /// Synthesize arrow keys (alt screen + DECSET 1007).
    Arrows,
    /// Scroll the viewport through scrollback.
    Display,
}

pub fn route_wheel(mode: MouseMode, shift: bool, alt_screen: bool, alt_scroll: bool) -> WheelRoute {
    if reports(mode, shift) {
        WheelRoute::Report
    } else if alt_screen && alt_scroll && !shift {
        WheelRoute::Arrows
    } else {
        WheelRoute::Display
    }
}

/// Fold a wheel delta (in lines, possibly fractional — trackpad pixel
/// deltas divided by the cell height) into the accumulator and take out
/// the whole lines. A direction change drops the leftover fraction so
/// reversals respond immediately.
pub fn wheel_lines(acc: &mut f32, delta: f32) -> i32 {
    if *acc != 0.0 && acc.signum() != delta.signum() && delta != 0.0 {
        *acc = 0.0;
    }
    *acc += delta;
    let whole = acc.trunc();
    *acc -= whole;
    whole as i32
}

#[cfg(test)]
#[path = "../tests/mouse.rs"]
mod tests;
