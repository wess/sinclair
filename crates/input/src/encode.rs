//! `encode_key`: one keystroke -> the bytes a legacy xterm would send.

use crate::csi;
use crate::kitty;
use crate::{Mods, TermState};

/// Encode a keystroke. `key` is the gpui keystroke name; `text` is the
/// platform-resolved typed text for printable keys. Returns `None` when the
/// key produces no pty bytes (cmd chords, unknown non-printables).
pub fn encode_key(key: &str, text: Option<&str>, mods: Mods, state: TermState) -> Option<Vec<u8>> {
    if mods.cmd {
        return None;
    }
    // Kitty keyboard protocol intercepts the keys it disambiguates; others
    // fall through to legacy encoding below.
    if state.kitty_flags != 0 {
        if let Some(bytes) = kitty::encode(key, mods, state.kitty_flags) {
            return Some(bytes);
        }
    }
    if let Some(bytes) = special(key, mods, state) {
        return Some(bytes);
    }
    if mods.ctrl {
        if let Some(byte) = ctrl_byte(key) {
            return Some(alt_prefixed(mods, vec![byte]));
        }
    }
    printable(key, text, mods)
}

/// Named (non-printable) keys; `None` means "not a special key".
fn special(key: &str, mods: Mods, state: TermState) -> Option<Vec<u8>> {
    let bytes = match key {
        "enter" => alt_prefixed(mods, vec![b'\r']),
        "tab" => {
            if mods.shift {
                alt_prefixed(mods, vec![csi::ESC, b'[', b'Z'])
            } else {
                alt_prefixed(mods, vec![b'\t'])
            }
        }
        "escape" => alt_prefixed(mods, vec![csi::ESC]),
        "backspace" => {
            let byte = if mods.ctrl { 0x08 } else { 0x7f };
            alt_prefixed(mods, vec![byte])
        }
        "up" => cursor_key(b'A', mods, state),
        "down" => cursor_key(b'B', mods, state),
        "right" => cursor_key(b'C', mods, state),
        "left" => cursor_key(b'D', mods, state),
        "home" => cursor_key(b'H', mods, state),
        "end" => cursor_key(b'F', mods, state),
        "insert" => csi::tilde(2, mods),
        "delete" => csi::tilde(3, mods),
        "pageup" => csi::tilde(5, mods),
        "pagedown" => csi::tilde(6, mods),
        "f1" => fkey_ss3(b'P', mods),
        "f2" => fkey_ss3(b'Q', mods),
        "f3" => fkey_ss3(b'R', mods),
        "f4" => fkey_ss3(b'S', mods),
        "f5" => csi::tilde(15, mods),
        "f6" => csi::tilde(17, mods),
        "f7" => csi::tilde(18, mods),
        "f8" => csi::tilde(19, mods),
        "f9" => csi::tilde(20, mods),
        "f10" => csi::tilde(21, mods),
        "f11" => csi::tilde(23, mods),
        "f12" => csi::tilde(24, mods),
        _ => return None,
    };
    Some(bytes)
}

/// Arrows/home/end: CSI normally, SS3 in app mode, modified CSI form when
/// any modifier is held (modifiers force CSI even in app mode).
fn cursor_key(final_byte: u8, mods: Mods, state: TermState) -> Vec<u8> {
    if csi::is_modified(mods) {
        csi::cursor_modified(final_byte, mods)
    } else {
        csi::cursor(final_byte, state.cursor_keys_app)
    }
}

/// f1-f4: SS3 unmodified, `CSI 1;{m}{final}` modified.
fn fkey_ss3(final_byte: u8, mods: Mods) -> Vec<u8> {
    if csi::is_modified(mods) {
        csi::cursor_modified(final_byte, mods)
    } else {
        csi::ss3(final_byte)
    }
}

/// The legacy ctrl-key table: letters -> 0x01..0x1a plus the punctuation
/// control bytes.
fn ctrl_byte(key: &str) -> Option<u8> {
    if key == "space" {
        return Some(0x00);
    }
    let mut chars = key.chars();
    let c = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    match c {
        'a'..='z' => Some(c as u8 - b'a' + 1),
        '@' => Some(0x00),
        '[' => Some(0x1b),
        '\\' => Some(0x1c),
        ']' => Some(0x1d),
        '^' => Some(0x1e),
        '_' | '-' => Some(0x1f),
        '?' | '8' => Some(0x7f),
        _ => None,
    }
}

/// Printable path: emit `text` as UTF-8; alt (without ctrl) ESC-prefixes a
/// single ASCII char and passes non-ASCII text through unchanged.
fn printable(key: &str, text: Option<&str>, mods: Mods) -> Option<Vec<u8>> {
    let text = match text {
        Some(t) if !t.is_empty() => t.to_string(),
        _ => fallback_text(key)?,
    };
    if mods.alt && !mods.ctrl {
        let mut chars = text.chars();
        if let (Some(c), None) = (chars.next(), chars.next()) {
            if c.is_ascii() {
                return Some(vec![csi::ESC, c as u8]);
            }
        }
    }
    Some(text.into_bytes())
}

/// Derive text from the key name when the platform supplied none.
fn fallback_text(key: &str) -> Option<String> {
    if key == "space" {
        return Some(" ".to_string());
    }
    let mut chars = key.chars();
    let c = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    Some(c.to_string())
}

fn alt_prefixed(mods: Mods, mut bytes: Vec<u8>) -> Vec<u8> {
    if mods.alt {
        bytes.insert(0, csi::ESC);
    }
    bytes
}

#[cfg(test)]
#[path = "../tests/encode.rs"]
mod tests;
