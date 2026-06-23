//! Mouse reporting encoders (xterm semantics): SGR (1006), legacy X10,
//! and alternate-scroll (1007) arrow synthesis.

use crate::csi;
use crate::Mods;

/// Button involved in a mouse event. `None` = motion with no button held.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
    WheelUp,
    WheelDown,
    None,
}

/// What the mouse did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseAction {
    Press,
    Release,
    Motion,
}

/// Legacy X10 coordinate ceiling (32 + 223 = byte 255).
const LEGACY_MAX: u32 = 223;

/// Base xterm button code.
fn base_code(button: MouseButton) -> u32 {
    match button {
        MouseButton::Left => 0,
        MouseButton::Middle => 1,
        MouseButton::Right => 2,
        MouseButton::WheelUp => 64,
        MouseButton::WheelDown => 65,
        MouseButton::None => 3,
    }
}

/// Modifier bits added to the button code: shift 4, alt 8, ctrl 16.
fn mod_bits(mods: Mods) -> u32 {
    let mut sum = 0;
    if mods.shift {
        sum += 4;
    }
    if mods.alt {
        sum += 8;
    }
    if mods.ctrl {
        sum += 16;
    }
    sum
}

fn is_wheel(button: MouseButton) -> bool {
    matches!(button, MouseButton::WheelUp | MouseButton::WheelDown)
}

/// Encode one mouse event. `col`/`row` are 1-based cell coordinates.
///
/// SGR (1006): `ESC [ < {code} ; {col} ; {row} M` (final `m` for release;
/// release keeps the button code). Legacy X10: `ESC [ M` plus three bytes
/// `32+code, 32+col, 32+row` with release encoded as button 3 and
/// coordinates clamped to 223. Wheel releases are never sent (`None`).
pub fn encode_mouse(
    action: MouseAction,
    button: MouseButton,
    col: u32,
    row: u32,
    mods: Mods,
    sgr: bool,
) -> Option<Vec<u8>> {
    if action == MouseAction::Release && is_wheel(button) {
        return None;
    }
    let motion = if action == MouseAction::Motion { 32 } else { 0 };
    let code = base_code(button) + motion + mod_bits(mods);
    if sgr {
        let fin = if action == MouseAction::Release {
            'm'
        } else {
            'M'
        };
        return Some(format!("\x1b[<{code};{col};{row}{fin}").into_bytes());
    }
    let code = if action == MouseAction::Release {
        3 + mod_bits(mods)
    } else {
        code
    };
    let col = col.clamp(1, LEGACY_MAX) as u8;
    let row = row.clamp(1, LEGACY_MAX) as u8;
    Some(vec![
        csi::ESC,
        b'[',
        b'M',
        32 + code as u8,
        32 + col,
        32 + row,
    ])
}

/// Alternate-scroll (mode 1007) in the alt screen: a wheel tick becomes
/// `lines` repeats of the up/down arrow encoding (CSI A/B, or SS3 A/B in
/// application cursor mode).
pub fn encode_scroll_arrows(up: bool, lines: u32, cursor_keys_app: bool) -> Vec<u8> {
    let one = csi::cursor(if up { b'A' } else { b'B' }, cursor_keys_app);
    let mut out = Vec::with_capacity(one.len() * lines as usize);
    for _ in 0..lines {
        out.extend_from_slice(&one);
    }
    out
}

#[cfg(test)]
#[path = "../tests/mouse.rs"]
mod tests;
