//! Types and helpers for terminal replies: color queries (OSC 4/10/11/12)
//! and clipboard set (OSC 52).

/// Colors the terminal can report when an application queries them. The
/// host installs these from the active theme via
/// [`crate::Terminal::set_report_colors`]; without them, color queries are
/// ignored (the core has no palette of its own).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportColors {
    pub foreground: (u8, u8, u8),
    pub background: (u8, u8, u8),
    pub cursor: (u8, u8, u8),
    /// The full 256-entry palette as actually rendered.
    pub palette: [(u8, u8, u8); 256],
}

/// A clipboard write requested by the program via OSC 52. `kind` is the
/// raw selection field (`c` = clipboard, `p` = primary, possibly several);
/// `data` is the decoded bytes the host should place on the clipboard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clipboard {
    pub kind: String,
    pub data: Vec<u8>,
}

/// Format an 8-bit RGB triple as an xterm color reply body:
/// `rgb:RRRR/GGGG/BBBB`, where each channel is widened to 16 bits by
/// replication (0xAB -> 0xABAB), matching xterm's query answers.
pub fn format_rgb(rgb: (u8, u8, u8)) -> String {
    let wide = |c: u8| (c as u16) * 0x101;
    format!(
        "rgb:{:04x}/{:04x}/{:04x}",
        wide(rgb.0),
        wide(rgb.1),
        wide(rgb.2)
    )
}

/// Decode standard base64 (RFC 4648, no line breaks). Whitespace is
/// skipped; `=` padding is honored. Returns `None` on any invalid byte or
/// a malformed length. Used for OSC 52 clipboard payloads.
pub fn base64_decode(input: &[u8]) -> Option<Vec<u8>> {
    let mut bits: u32 = 0;
    let mut nbits = 0u32;
    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    for &b in input {
        if b == b'=' || b.is_ascii_whitespace() {
            continue;
        }
        let v = base64_value(b)?;
        bits = (bits << 6) | v as u32;
        nbits += 6;
        if nbits >= 8 {
            nbits -= 8;
            out.push((bits >> nbits) as u8);
        }
    }
    Some(out)
}

/// Encode bytes as standard base64 with padding. Used to answer OSC 52
/// clipboard queries.
pub fn base64_encode(input: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[(n >> 18) as usize & 0x3f] as char);
        out.push(ALPHABET[(n >> 12) as usize & 0x3f] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(n >> 6) as usize & 0x3f] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[n as usize & 0x3f] as char
        } else {
            '='
        });
    }
    out
}

/// Decode a hex string (even length, ASCII hex digits) to bytes. Used for
/// XTGETTCAP capability names. `None` on bad length or non-hex.
pub fn hex_decode(input: &[u8]) -> Option<Vec<u8>> {
    if input.len() % 2 != 0 {
        return None;
    }
    input
        .chunks(2)
        .map(|pair| {
            let hi = (pair[0] as char).to_digit(16)?;
            let lo = (pair[1] as char).to_digit(16)?;
            Some((hi * 16 + lo) as u8)
        })
        .collect()
}

/// Encode bytes as lowercase hex. Used for XTGETTCAP reply values.
pub fn hex_encode(input: &[u8]) -> String {
    let mut out = String::with_capacity(input.len() * 2);
    for &b in input {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

fn base64_value(b: u8) -> Option<u8> {
    match b {
        b'A'..=b'Z' => Some(b - b'A'),
        b'a'..=b'z' => Some(b - b'a' + 26),
        b'0'..=b'9' => Some(b - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

#[cfg(test)]
#[path = "../../tests/term/report.rs"]
mod tests;
