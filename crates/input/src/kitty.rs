//! Kitty keyboard protocol key encoding.
//!
//! When any enhancement flag is on, this module — not the legacy encoder —
//! decides how a keystroke is spelled. It returns `None` only for the cases
//! the protocol deliberately leaves alone: a key that should send its text,
//! and a functional key whose legacy escape code the protocol still wants
//! (which the legacy encoder already spells correctly).
//!
//! Three things the legacy encoder structurally cannot say, and this one can:
//! the super/hyper/meta and lock modifiers, the press/repeat/release event
//! type, and the keys with no legacy spelling at all — F13 and up, the keypad,
//! the media keys, and the modifier keys themselves.

use crate::kitty_flags;
use crate::{KeyEvent, Mods};

/// How a key is written on the wire.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Form {
  /// `CSI <code> ; <mods> u`
  CsiU,
  /// `CSI <code> ; <mods> ~`
  Tilde,
  /// `CSI 1 ; <mods> <final>` — the cursor-key shape, where the code is
  /// always 1 and the key is named by the final byte.
  Letter(u8),
}

/// Why a key is (or is not) reported, which decides when the legacy spelling
/// still wins.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
  /// Produces text. Reported as an escape code only when a modifier makes the
  /// text ambiguous, or when the client asked for every key.
  Text,
  /// Has a legacy C0 byte that stays in use while the key is unmodified:
  /// Enter, Tab, Backspace.
  Legacy,
  /// Escape — the key the disambiguate mode exists for. Always `CSI 27 u`.
  Escape,
  /// Has a legacy escape code good enough for the common modifiers, so the
  /// legacy encoder keeps it until something needs saying that it cannot say.
  Functional,
  /// Has no legacy spelling at all, so it is always reported here.
  Extended,
  /// A modifier key pressed by itself; reported only in all-keys mode.
  Modifier,
}

/// One key's wire spelling.
#[derive(Clone, Copy)]
struct Spelling {
  code: u32,
  form: Form,
  kind: Kind,
}

const fn csi_u(code: u32, kind: Kind) -> Spelling {
  Spelling {
    code,
    form: Form::CsiU,
    kind,
  }
}

const fn tilde(code: u32, kind: Kind) -> Spelling {
  Spelling {
    code,
    form: Form::Tilde,
    kind,
  }
}

const fn letter(final_byte: u8, kind: Kind) -> Spelling {
  Spelling {
    code: 1,
    form: Form::Letter(final_byte),
    kind,
  }
}

/// Encode a keystroke under the active flags. `None` leaves the key to the
/// legacy encoder — either because it produces text, or because its legacy
/// escape code is exactly what the protocol asks for here.
pub(crate) fn encode(
  key: &str,
  text: Option<&str>,
  mods: Mods,
  flags: u8,
  event: KeyEvent,
) -> Option<Vec<u8>> {
  let report_events = flags & kitty_flags::REPORT_EVENT_TYPES != 0;
  // A key-up exists on the wire only when the program asked for event types.
  if event == KeyEvent::Release && !report_events {
    return None;
  }
  let all_keys = flags & kitty_flags::REPORT_ALL_KEYS_AS_ESCAPE_CODES != 0;
  let spelling = lookup(key)?;
  // Press is the default event type and is never written out.
  let event_code = match (report_events, event) {
    (true, KeyEvent::Repeat) => Some(2),
    (true, KeyEvent::Release) => Some(3),
    _ => None,
  };

  // A functional key is already an unambiguous escape code, so the protocol
  // keeps its legacy spelling — all-keys mode is about text keys, not this
  // one. It only comes here when it must carry a modifier or an event type
  // that the legacy form has nowhere to put.
  if spelling.kind == Kind::Functional && !mods.beyond_legacy() && event_code.is_none() {
    return None;
  }
  if !all_keys {
    match spelling.kind {
      // A bare modifier is noise unless the client asked for every key.
      Kind::Modifier => return None,
      // Text stays text until a modifier makes it ambiguous. Shift alone
      // does not: it is already spelled by the character it produces.
      Kind::Text if !mods.disambiguates() => return None,
      // Enter/Tab/Backspace keep their C0 bytes while untouched.
      Kind::Legacy if !mods.any() => return None,
      _ => {}
    }
  }
  // Associated text is not sent for a release: nothing was typed.
  let assoc = (flags & kitty_flags::REPORT_ASSOCIATED_TEXT != 0
    && event != KeyEvent::Release
    && spelling.kind == Kind::Text)
    .then_some(text)
    .flatten()
    .filter(|t| !t.is_empty());
  let shifted = (flags & kitty_flags::REPORT_ALTERNATE_KEYS != 0)
    .then(|| shifted_key(spelling, mods, text))
    .flatten();

  Some(build(spelling, mods, event_code, shifted, assoc))
}

/// The character a text key produces with shift held, when it differs from
/// the key's own code — the protocol's `shifted-key` alternate.
///
/// The third alternate, the base-layout key, is not reported: the host hands
/// us the standard-layout key as `key` already, so it is never the one that
/// differs.
fn shifted_key(spelling: Spelling, mods: Mods, text: Option<&str>) -> Option<u32> {
  if spelling.kind != Kind::Text || !mods.shift {
    return None;
  }
  let mut chars = text?.chars();
  let c = chars.next()?;
  if chars.next().is_some() {
    return None;
  }
  (c as u32 != spelling.code).then_some(c as u32)
}

/// Assemble the sequence. Fields are only written when they carry something,
/// except that an earlier field must appear once a later one does — so an
/// unmodified key with an event type still spells its modifiers as `1`.
fn build(
  spelling: Spelling,
  mods: Mods,
  event: Option<u8>,
  shifted: Option<u32>,
  text: Option<&str>,
) -> Vec<u8> {
  let m = mods.kitty_param();
  let needs_mods = m != 1 || event.is_some() || text.is_some();
  let mut out = String::from("\x1b[");

  match spelling.form {
    // The cursor-key shape carries no key number of its own — the final byte
    // names the key — so the leading `1` appears only to hold the modifier
    // field's place.
    Form::Letter(_) => {
      if needs_mods {
        out.push('1');
      }
    }
    _ => {
      out.push_str(&spelling.code.to_string());
      if let Some(shifted) = shifted {
        out.push(':');
        out.push_str(&shifted.to_string());
      }
    }
  }

  if needs_mods {
    out.push(';');
    out.push_str(&m.to_string());
    if let Some(event) = event {
      out.push(':');
      out.push_str(&event.to_string());
    }
  }
  if let Some(text) = text {
    out.push(';');
    for (i, c) in text.chars().enumerate() {
      if i > 0 {
        out.push(':');
      }
      out.push_str(&(c as u32).to_string());
    }
  }
  out.push(match spelling.form {
    Form::CsiU => 'u',
    Form::Tilde => '~',
    Form::Letter(final_byte) => final_byte as char,
  });
  out.into_bytes()
}

impl Mods {
  /// Any modifier at all, locks included.
  fn any(self) -> bool {
    self.shift
      || self.alt
      || self.ctrl
      || self.cmd
      || self.hyper
      || self.meta
      || self.caps_lock
      || self.num_lock
  }

  /// A modifier that makes a text key's bytes ambiguous, so the protocol
  /// wants the key spelled out. Shift is not one: it is already in the text.
  fn disambiguates(self) -> bool {
    self.ctrl || self.alt || self.cmd || self.hyper || self.meta
  }

  /// A modifier the legacy `CSI 1 ; m` parameter has no bit for.
  fn beyond_legacy(self) -> bool {
    self.cmd || self.hyper || self.meta || self.caps_lock || self.num_lock
  }

  /// The protocol's modifier parameter: one plus the held bits.
  fn kitty_param(self) -> u16 {
    let mut sum = 0u16;
    for (held, bit) in [
      (self.shift, 1),
      (self.alt, 2),
      (self.ctrl, 4),
      (self.cmd, 8),
      (self.hyper, 16),
      (self.meta, 32),
      (self.caps_lock, 64),
      (self.num_lock, 128),
    ] {
      if held {
        sum += bit;
      }
    }
    1 + sum
  }
}

/// The spelling of a named key, or of a single-character text key.
fn lookup(key: &str) -> Option<Spelling> {
  if let Some(spelling) = named(key) {
    return Some(spelling);
  }
  // Anything else that is one character is a text key, and its own codepoint
  // is its key code.
  let mut chars = key.chars();
  match (chars.next(), chars.next()) {
    (Some(c), None) => Some(csi_u(c as u32, Kind::Text)),
    _ => None,
  }
}

/// The protocol's functional-key table, by the host's key name.
fn named(key: &str) -> Option<Spelling> {
  let spelling = match key {
    "escape" => csi_u(27, Kind::Escape),
    "enter" => csi_u(13, Kind::Legacy),
    "tab" => csi_u(9, Kind::Legacy),
    "backspace" => csi_u(127, Kind::Legacy),
    "space" => csi_u(32, Kind::Text),

    "insert" => tilde(2, Kind::Functional),
    "delete" => tilde(3, Kind::Functional),
    "pageup" => tilde(5, Kind::Functional),
    "pagedown" => tilde(6, Kind::Functional),
    "up" => letter(b'A', Kind::Functional),
    "down" => letter(b'B', Kind::Functional),
    "right" => letter(b'C', Kind::Functional),
    "left" => letter(b'D', Kind::Functional),
    "end" => letter(b'F', Kind::Functional),
    "home" => letter(b'H', Kind::Functional),

    "capslock" => csi_u(57358, Kind::Extended),
    "scrolllock" => csi_u(57359, Kind::Extended),
    "numlock" => csi_u(57360, Kind::Extended),
    "printscreen" => csi_u(57361, Kind::Extended),
    "pause" => csi_u(57362, Kind::Extended),
    "menu" => csi_u(57363, Kind::Extended),

    // F1-F4 keep the cursor-key shape; F3 is the odd one out, spelled as a
    // tilde key, and F5-F12 follow the tilde table.
    "f1" => letter(b'P', Kind::Functional),
    "f2" => letter(b'Q', Kind::Functional),
    "f3" => tilde(13, Kind::Functional),
    "f4" => letter(b'S', Kind::Functional),
    "f5" => tilde(15, Kind::Functional),
    "f6" => tilde(17, Kind::Functional),
    "f7" => tilde(18, Kind::Functional),
    "f8" => tilde(19, Kind::Functional),
    "f9" => tilde(20, Kind::Functional),
    "f10" => tilde(21, Kind::Functional),
    "f11" => tilde(23, Kind::Functional),
    "f12" => tilde(24, Kind::Functional),

    "kp_0" => csi_u(57399, Kind::Extended),
    "kp_1" => csi_u(57400, Kind::Extended),
    "kp_2" => csi_u(57401, Kind::Extended),
    "kp_3" => csi_u(57402, Kind::Extended),
    "kp_4" => csi_u(57403, Kind::Extended),
    "kp_5" => csi_u(57404, Kind::Extended),
    "kp_6" => csi_u(57405, Kind::Extended),
    "kp_7" => csi_u(57406, Kind::Extended),
    "kp_8" => csi_u(57407, Kind::Extended),
    "kp_9" => csi_u(57408, Kind::Extended),
    "kp_decimal" => csi_u(57409, Kind::Extended),
    "kp_divide" => csi_u(57410, Kind::Extended),
    "kp_multiply" => csi_u(57411, Kind::Extended),
    "kp_subtract" => csi_u(57412, Kind::Extended),
    "kp_add" => csi_u(57413, Kind::Extended),
    "kp_enter" => csi_u(57414, Kind::Extended),
    "kp_equal" => csi_u(57415, Kind::Extended),
    "kp_separator" => csi_u(57416, Kind::Extended),
    "kp_left" => csi_u(57417, Kind::Extended),
    "kp_right" => csi_u(57418, Kind::Extended),
    "kp_up" => csi_u(57419, Kind::Extended),
    "kp_down" => csi_u(57420, Kind::Extended),
    "kp_pageup" => csi_u(57421, Kind::Extended),
    "kp_pagedown" => csi_u(57422, Kind::Extended),
    "kp_home" => csi_u(57423, Kind::Extended),
    "kp_end" => csi_u(57424, Kind::Extended),
    "kp_insert" => csi_u(57425, Kind::Extended),
    "kp_delete" => csi_u(57426, Kind::Extended),
    "kp_begin" => letter(b'E', Kind::Extended),

    "media_play" => csi_u(57428, Kind::Extended),
    "media_pause" => csi_u(57429, Kind::Extended),
    "media_play_pause" => csi_u(57430, Kind::Extended),
    "media_reverse" => csi_u(57431, Kind::Extended),
    "media_stop" => csi_u(57432, Kind::Extended),
    "media_fast_forward" => csi_u(57433, Kind::Extended),
    "media_rewind" => csi_u(57434, Kind::Extended),
    "media_track_next" => csi_u(57435, Kind::Extended),
    "media_track_previous" => csi_u(57436, Kind::Extended),
    "media_record" => csi_u(57437, Kind::Extended),
    "lower_volume" => csi_u(57438, Kind::Extended),
    "raise_volume" => csi_u(57439, Kind::Extended),
    "mute_volume" => csi_u(57440, Kind::Extended),

    "left_shift" => csi_u(57441, Kind::Modifier),
    "left_control" => csi_u(57442, Kind::Modifier),
    "left_alt" => csi_u(57443, Kind::Modifier),
    "left_super" => csi_u(57444, Kind::Modifier),
    "left_hyper" => csi_u(57445, Kind::Modifier),
    "left_meta" => csi_u(57446, Kind::Modifier),
    "right_shift" => csi_u(57447, Kind::Modifier),
    "right_control" => csi_u(57448, Kind::Modifier),
    "right_alt" => csi_u(57449, Kind::Modifier),
    "right_super" => csi_u(57450, Kind::Modifier),
    "right_hyper" => csi_u(57451, Kind::Modifier),
    "right_meta" => csi_u(57452, Kind::Modifier),
    "iso_level3_shift" => csi_u(57453, Kind::Modifier),
    "iso_level5_shift" => csi_u(57454, Kind::Modifier),

    _ => return f_key(key),
  };
  Some(spelling)
}

/// F13 through F35, which have no legacy spelling and run consecutively from
/// 57376. F1-F12 are handled above; they do have legacy forms.
fn f_key(key: &str) -> Option<Spelling> {
  let n: u32 = key.strip_prefix('f')?.parse().ok()?;
  (13..=35)
    .contains(&n)
    .then(|| csi_u(57376 + n - 13, Kind::Extended))
}

#[cfg(test)]
#[path = "../tests/kitty.rs"]
mod tests;
