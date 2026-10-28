//! `encode_paste`: clipboard text -> pty bytes.

const START: &[u8] = b"\x1b[200~";
const END: &[u8] = b"\x1b[201~";

/// Bracketed: wrap in `ESC[200~` / `ESC[201~`. Non-bracketed: normalize
/// newlines to carriage returns.
///
/// Either way the text is first stripped of control characters other than
/// tab, newline and carriage return (xterm's `allowPasteControls=false`).
/// Removing ESC — and the C1 controls, which some apps decode as their 7-bit
/// forms — is what keeps a paste from closing the bracket early: deleting
/// only the literal end marker is not enough, because
/// `ESC[20` + `ESC[201~` + `1~` reassembles one once the middle goes.
pub fn encode_paste(text: &str, bracketed: bool) -> Vec<u8> {
  let clean = strip_controls(text);
  if bracketed {
    let mut out = Vec::with_capacity(START.len() + clean.len() + END.len());
    out.extend_from_slice(START);
    out.extend_from_slice(clean.as_bytes());
    out.extend_from_slice(END);
    out
  } else {
    clean.replace("\r\n", "\r").replace('\n', "\r").into_bytes()
  }
}

/// Whether a pasted char is a control a paste may not carry.
fn forbidden(c: char) -> bool {
  match c {
    '\t' | '\n' | '\r' => false,
    c => c.is_ascii_control() || ('\u{80}'..='\u{9f}').contains(&c),
  }
}

/// `text` without forbidden controls; borrows when there are none, which is
/// nearly every paste.
fn strip_controls(text: &str) -> std::borrow::Cow<'_, str> {
  if text.chars().any(forbidden) {
    text
      .chars()
      .filter(|&c| !forbidden(c))
      .collect::<String>()
      .into()
  } else {
    text.into()
  }
}

#[cfg(test)]
#[path = "../tests/paste.rs"]
mod tests;
