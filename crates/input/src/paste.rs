//! `encode_paste`: clipboard text -> pty bytes.

const START: &[u8] = b"\x1b[200~";
const END: &str = "\x1b[201~";

/// Bracketed: wrap in `ESC[200~` / `ESC[201~`, stripping any literal end
/// marker embedded in the text so a paste cannot break out of the bracket.
/// Non-bracketed: normalize newlines to carriage returns.
pub fn encode_paste(text: &str, bracketed: bool) -> Vec<u8> {
    if bracketed {
        let sanitized = text.replace(END, "");
        let mut out = Vec::with_capacity(START.len() + sanitized.len() + END.len());
        out.extend_from_slice(START);
        out.extend_from_slice(sanitized.as_bytes());
        out.extend_from_slice(END.as_bytes());
        out
    } else {
        text.replace("\r\n", "\r").replace('\n', "\r").into_bytes()
    }
}

#[cfg(test)]
#[path = "../tests/paste.rs"]
mod tests;
