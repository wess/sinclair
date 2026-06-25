//! Device Control String handling. Currently only XTGETTCAP
//! (`DCS + q <hex caps> ST`), used by programs to query terminfo
//! capabilities.

use super::report::{hex_decode, hex_encode};
use super::Inner;

/// In-progress DCS parse state. vte delivers a DCS as hook -> put* -> unhook.
#[derive(Debug, Default)]
pub(crate) enum Dcs {
    /// Not inside a DCS we care about.
    #[default]
    None,
    /// Accumulating an XTGETTCAP query payload (the hex capability list).
    XtGetTcap(Vec<u8>),
    /// Accumulating a sixel image payload.
    Sixel(Vec<u8>),
}

/// Begin a DCS. XTGETTCAP is `DCS + q ...` (intermediate `+`, final `q`);
/// sixel is `DCS <params> q ...` (final `q`, no `+`).
pub(crate) fn hook(inner: &mut Inner, intermediates: &[u8], action: char) {
    inner.dcs = if action == 'q' {
        if intermediates == [b'+'] {
            Dcs::XtGetTcap(Vec::new())
        } else {
            Dcs::Sixel(Vec::new())
        }
    } else {
        Dcs::None
    };
}

/// Largest DCS payload we buffer. A sequence with no terminator (ST) would
/// otherwise grow the buffer without bound; legitimate sixel images and
/// XTGETTCAP queries are far smaller.
const MAX_DCS_BYTES: usize = 4 * 1024 * 1024;

/// Accumulate a payload byte, dropping anything past the size cap.
pub(crate) fn put(inner: &mut Inner, byte: u8) {
    match &mut inner.dcs {
        Dcs::XtGetTcap(buf) | Dcs::Sixel(buf) => {
            if buf.len() < MAX_DCS_BYTES {
                buf.push(byte);
            }
        }
        Dcs::None => {}
    }
}

/// Finish the DCS and act on the payload.
pub(crate) fn unhook(inner: &mut Inner) {
    match std::mem::take(&mut inner.dcs) {
        Dcs::XtGetTcap(buf) => {
            for cap in buf.split(|&b| b == b';') {
                reply_cap(inner, cap);
            }
        }
        Dcs::Sixel(buf) => {
            if let Some(image) = crate::sixel::decode(&buf) {
                inner.place_sixel(image);
            }
        }
        Dcs::None => {}
    }
}

/// Answer one XTGETTCAP capability. `cap` is the hex-encoded name as the
/// program sent it; the reply echoes that hex verbatim.
fn reply_cap(inner: &mut Inner, cap: &[u8]) {
    let name = hex_decode(cap);
    let hex = String::from_utf8_lossy(cap);
    let (flag, body) = match name.as_deref().and_then(lookup) {
        // Known with a value: `1+r <hexname>=<hexvalue>`.
        Some(Some(value)) => (b'1', format!("{hex}={}", hex_encode(value.as_bytes()))),
        // Known boolean: `1+r <hexname>`.
        Some(None) => (b'1', hex.to_string()),
        // Unknown: `0+r <hexname>`.
        None => (b'0', hex.to_string()),
    };
    inner.output.extend_from_slice(b"\x1bP");
    inner.output.push(flag);
    inner.output.extend_from_slice(b"+r");
    inner.output.extend_from_slice(body.as_bytes());
    inner.output.extend_from_slice(b"\x1b\\");
}

/// Capability lookup. `Some(Some(v))` = string/numeric value, `Some(None)` =
/// boolean (present), `None` = unsupported.
fn lookup(name: &[u8]) -> Option<Option<&'static str>> {
    match name {
        b"Co" | b"colors" => Some(Some("256")),
        b"TN" | b"name" => Some(Some("xterm-256color")),
        b"RGB" => Some(Some("8/8/8")),
        b"Tc" | b"bce" | b"am" | b"km" | b"mir" | b"msgr" | b"npc" | b"xenl" => Some(None),
        _ => None,
    }
}

#[cfg(test)]
#[path = "../../tests/term/dcs.rs"]
mod tests;
