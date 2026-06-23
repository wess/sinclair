//! CSI/SS3 sequence builders and xterm modifier-parameter math.

use crate::Mods;

pub(crate) const ESC: u8 = 0x1b;

/// Any encoding-relevant modifier held (cmd is rejected before this).
pub(crate) fn is_modified(mods: Mods) -> bool {
    mods.shift || mods.alt || mods.ctrl
}

/// xterm modifier parameter: 1 + (shift=1, alt=2, ctrl=4).
pub(crate) fn modifier_param(mods: Mods) -> u8 {
    let mut sum = 0;
    if mods.shift {
        sum += 1;
    }
    if mods.alt {
        sum += 2;
    }
    if mods.ctrl {
        sum += 4;
    }
    1 + sum
}

/// Unmodified cursor-class key: `CSI {final}`, or `SS3 {final}` in
/// application cursor mode.
pub(crate) fn cursor(final_byte: u8, app: bool) -> Vec<u8> {
    if app {
        ss3(final_byte)
    } else {
        vec![ESC, b'[', final_byte]
    }
}

/// Modified cursor-class key (also f1-f4): `CSI 1 ; {m} {final}`.
/// Modifiers force the CSI form even in application cursor mode.
pub(crate) fn cursor_modified(final_byte: u8, mods: Mods) -> Vec<u8> {
    let mut out = vec![ESC, b'[', b'1', b';'];
    push_num(&mut out, modifier_param(mods));
    out.push(final_byte);
    out
}

/// `SS3 {final}` (ESC O ...).
pub(crate) fn ss3(final_byte: u8) -> Vec<u8> {
    vec![ESC, b'O', final_byte]
}

/// Tilde-class key: `CSI {n} ~`, or `CSI {n} ; {m} ~` when modified.
pub(crate) fn tilde(n: u8, mods: Mods) -> Vec<u8> {
    let mut out = vec![ESC, b'['];
    push_num(&mut out, n);
    if is_modified(mods) {
        out.push(b';');
        push_num(&mut out, modifier_param(mods));
    }
    out.push(b'~');
    out
}

fn push_num(out: &mut Vec<u8>, n: u8) {
    out.extend_from_slice(n.to_string().as_bytes());
}

#[cfg(test)]
#[path = "../tests/csi.rs"]
mod tests;
