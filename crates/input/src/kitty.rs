//! Kitty keyboard protocol key encoding.
//!
//! This intercepts only the keys the protocol *disambiguates* — Escape and
//! ctrl/alt-modified or all-keys-mode text keys — emitting the `CSI
//! codepoint ; modifiers u` form. Everything else (plain text, arrows,
//! function keys, …) returns `None` so the legacy encoder handles it
//! unchanged; in kitty mode those still produce valid (unambiguous) escape
//! codes.
//!
//! Limitations: only press events are emitted (the host does not deliver
//! distinct release events), so the event-type suffix is never added and
//! the `report_event_types`/`report_associated_text`/`report_alternate_keys`
//! flags are tracked-but-not-encoded. `report_all_keys_as_escape_codes`
//! upgrades text keys to `CSI u`; navigation/function keys keep their legacy
//! (still-escape-code) spellings.

use crate::csi;
use crate::kitty_flags;
use crate::Mods;

/// Encode a keystroke under active kitty flags. Returns `None` for keys
/// this layer does not special-case, leaving them to the legacy encoder.
pub(crate) fn encode(key: &str, mods: Mods, flags: u8) -> Option<Vec<u8>> {
    let codepoint = csi_u_codepoint(key)?;
    let all_keys = flags & kitty_flags::REPORT_ALL_KEYS_AS_ESCAPE_CODES != 0;
    // Escape is always disambiguated; other text/special keys only when a
    // ctrl/alt modifier (which would otherwise collapse to a control byte
    // or ESC-prefix) is held, or when all keys are requested as escapes.
    let force = all_keys || key == "escape" || mods.ctrl || mods.alt;
    force.then(|| csi_u(codepoint, mods))
}

/// The CSI-u key code for a disambiguated key, or `None` for keys that keep
/// their legacy navigation/function encodings.
fn csi_u_codepoint(key: &str) -> Option<u32> {
    Some(match key {
        "escape" => 27,
        "enter" => 13,
        "tab" => 9,
        "backspace" => 127,
        "space" => 32,
        other => {
            let mut chars = other.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => c as u32,
                _ => return None,
            }
        }
    })
}

/// `CSI codepoint u`, or `CSI codepoint ; modifiers u` when modified.
fn csi_u(codepoint: u32, mods: Mods) -> Vec<u8> {
    let m = csi::modifier_param(mods);
    if m == 1 {
        format!("\x1b[{codepoint}u").into_bytes()
    } else {
        format!("\x1b[{codepoint};{m}u").into_bytes()
    }
}

#[cfg(test)]
#[path = "../tests/kitty.rs"]
mod tests;
