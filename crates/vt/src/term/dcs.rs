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
}

/// Begin a DCS. XTGETTCAP is `DCS + q ...`: intermediate `+`, final `q`.
pub(crate) fn hook(inner: &mut Inner, intermediates: &[u8], action: char) {
    inner.dcs = if intermediates == [b'+'] && action == 'q' {
        Dcs::XtGetTcap(Vec::new())
    } else {
        Dcs::None
    };
}

/// Accumulate a payload byte.
pub(crate) fn put(inner: &mut Inner, byte: u8) {
    if let Dcs::XtGetTcap(buf) = &mut inner.dcs {
        buf.push(byte);
    }
}

/// Finish the DCS and emit any reply.
pub(crate) fn unhook(inner: &mut Inner) {
    if let Dcs::XtGetTcap(buf) = std::mem::take(&mut inner.dcs) {
        for cap in buf.split(|&b| b == b';') {
            reply_cap(inner, cap);
        }
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
