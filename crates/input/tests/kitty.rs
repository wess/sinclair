use super::*;
use crate::KeyEvent;

const NONE: Mods = Mods {
  shift: false,
  alt: false,
  ctrl: false,
  cmd: false,
  hyper: false,
  meta: false,
  caps_lock: false,
  num_lock: false,
};
const SHIFT: Mods = Mods {
  shift: true,
  ..NONE
};
const CTRL: Mods = Mods { ctrl: true, ..NONE };
const ALT: Mods = Mods { alt: true, ..NONE };
const CTRL_SHIFT: Mods = Mods {
  ctrl: true,
  shift: true,
  ..NONE
};
const CMD: Mods = Mods { cmd: true, ..NONE };
const HYPER: Mods = Mods {
  hyper: true,
  ..NONE
};
const META: Mods = Mods { meta: true, ..NONE };
const CAPS: Mods = Mods {
  caps_lock: true,
  ..NONE
};
const NUM: Mods = Mods {
  num_lock: true,
  ..NONE
};

const DIS: u8 = kitty_flags::DISAMBIGUATE;
const ALL: u8 = kitty_flags::REPORT_ALL_KEYS_AS_ESCAPE_CODES;
const EVENTS: u8 = kitty_flags::REPORT_EVENT_TYPES;
const ALTERNATE: u8 = kitty_flags::REPORT_ALTERNATE_KEYS;
const ASSOC: u8 = kitty_flags::REPORT_ASSOCIATED_TEXT;

/// Encode a plain key press (the default event) — most tests only care about
/// press bytes, so this keeps them terse.
fn press(key: &str, mods: Mods, flags: u8) -> Option<Vec<u8>> {
  encode(key, None, mods, flags, KeyEvent::Press)
}

/// Encode with the platform-resolved text the key would type.
fn typed(key: &str, text: &str, mods: Mods, flags: u8) -> Option<Vec<u8>> {
  encode(key, Some(text), mods, flags, KeyEvent::Press)
}

#[test]
fn escape_always_disambiguated() {
  assert_eq!(press("escape", NONE, DIS).unwrap(), b"\x1b[27u");
  assert_eq!(press("escape", SHIFT, DIS).unwrap(), b"\x1b[27;2u");
}

#[test]
fn ctrl_letter_becomes_csi_u() {
  // ctrl+a: codepoint 97, modifiers 1+ctrl(4) = 5.
  assert_eq!(press("a", CTRL, DIS).unwrap(), b"\x1b[97;5u");
  // ctrl+shift+a: 1+shift(1)+ctrl(4) = 6.
  assert_eq!(press("a", CTRL_SHIFT, DIS).unwrap(), b"\x1b[97;6u");
}

#[test]
fn alt_letter_becomes_csi_u() {
  // alt+a: 1+alt(2) = 3.
  assert_eq!(press("a", ALT, DIS).unwrap(), b"\x1b[97;3u");
}

#[test]
fn super_letter_becomes_csi_u() {
  // cmd+a: 1+super(8) = 9.
  assert_eq!(press("a", CMD, DIS).unwrap(), b"\x1b[97;9u");
}

#[test]
fn plain_and_shift_text_defer_to_legacy_unless_all_keys() {
  // Disambiguate only: plain/shifted printable returns None (legacy).
  assert_eq!(press("a", NONE, DIS), None);
  assert_eq!(press("a", SHIFT, DIS), None);
  assert_eq!(press("1", NONE, DIS), None);
  // All-keys mode upgrades them to CSI u.
  assert_eq!(press("a", NONE, ALL).unwrap(), b"\x1b[97u");
  assert_eq!(press("a", SHIFT, ALL).unwrap(), b"\x1b[97;2u");
}

#[test]
fn enter_tab_backspace_disambiguate_only_when_modified_or_all() {
  // Plain ones defer to legacy in disambiguate mode.
  assert_eq!(press("enter", NONE, DIS), None);
  assert_eq!(press("tab", NONE, DIS), None);
  assert_eq!(press("backspace", NONE, DIS), None);
  // All-keys upgrades them.
  assert_eq!(press("enter", NONE, ALL).unwrap(), b"\x1b[13u");
  assert_eq!(press("tab", NONE, ALL).unwrap(), b"\x1b[9u");
  assert_eq!(press("backspace", NONE, ALL).unwrap(), b"\x1b[127u");
  // Ctrl modified -> CSI u even in disambiguate mode.
  assert_eq!(press("enter", CTRL, DIS).unwrap(), b"\x1b[13;5u");
  // Any modifier disambiguates them, shift and super included, so a
  // program can tell shift+enter / cmd+enter apart from enter.
  assert_eq!(press("enter", SHIFT, DIS).unwrap(), b"\x1b[13;2u");
  assert_eq!(press("enter", CMD, DIS).unwrap(), b"\x1b[13;9u");
  assert_eq!(press("tab", SHIFT, DIS).unwrap(), b"\x1b[9;2u");
  assert_eq!(press("backspace", SHIFT, DIS).unwrap(), b"\x1b[127;2u");
}

#[test]
fn navigation_and_function_keys_defer_to_legacy() {
  for key in ["up", "down", "home", "delete", "pageup", "f1", "f12"] {
    assert_eq!(press(key, NONE, ALL), None, "{key}");
    assert_eq!(press(key, CTRL, DIS), None, "{key}");
  }
}

#[test]
fn event_types_only_encoded_with_the_flag() {
  // Without report_event_types, a repeat encodes exactly like a press and a
  // release produces nothing.
  assert_eq!(
    encode("a", None, CTRL, DIS, KeyEvent::Repeat).unwrap(),
    b"\x1b[97;5u"
  );
  assert_eq!(encode("a", None, CTRL, DIS, KeyEvent::Release), None);
  // With the flag, repeat is `:2` and release is `:3`.
  assert_eq!(
    encode("a", None, CTRL, DIS | EVENTS, KeyEvent::Repeat).unwrap(),
    b"\x1b[97;5:2u"
  );
  assert_eq!(
    encode("a", None, CTRL, DIS | EVENTS, KeyEvent::Release).unwrap(),
    b"\x1b[97;5:3u"
  );
}

#[test]
fn event_type_forces_the_modifier_field_when_unmodified() {
  // An unmodified all-keys press is `CSI 97 u`; its release carries `;1:3`
  // because the event sub-parameter needs the modifiers field present.
  assert_eq!(
    encode("a", None, NONE, ALL | EVENTS, KeyEvent::Press).unwrap(),
    b"\x1b[97u"
  );
  assert_eq!(
    encode("a", None, NONE, ALL | EVENTS, KeyEvent::Release).unwrap(),
    b"\x1b[97;1:3u"
  );
}

#[test]
fn release_only_for_reported_keys() {
  // A plain letter is not in the disambiguated set, so even with event types
  // on there is no release to report (it never produced a press either).
  assert_eq!(encode("a", None, NONE, DIS | EVENTS, KeyEvent::Release), None);
}

#[test]
fn every_modifier_bit_has_its_place_in_the_parameter() {
  // 1 + shift(1) + alt(2) + ctrl(4) + super(8) + hyper(16) + meta(32)
  //   + caps lock(64) + num lock(128).
  let cases: &[(Mods, &[u8])] = &[
    (SHIFT, b"\x1b[97;2u"),
    (ALT, b"\x1b[97;3u"),
    (CTRL, b"\x1b[97;5u"),
    (CMD, b"\x1b[97;9u"),
    (HYPER, b"\x1b[97;17u"),
    (META, b"\x1b[97;33u"),
  ];
  for (mods, want) in cases {
    assert_eq!(press("a", *mods, ALL).unwrap(), *want, "{mods:?}");
  }
  // The locks are state, not chords: they ride along on any reported key.
  assert_eq!(press("a", CAPS, ALL).unwrap(), b"\x1b[97;65u");
  assert_eq!(press("a", NUM, ALL).unwrap(), b"\x1b[97;129u");

  let everything = Mods {
    shift: true,
    alt: true,
    ctrl: true,
    cmd: true,
    hyper: true,
    meta: true,
    caps_lock: true,
    num_lock: true,
  };
  assert_eq!(press("a", everything, ALL).unwrap(), b"\x1b[97;256u");
}

#[test]
fn a_lock_alone_does_not_drag_text_into_an_escape_code() {
  // Caps lock changes which character is typed, not how it is reported.
  assert_eq!(press("a", CAPS, DIS), None);
  assert_eq!(press("a", NUM, DIS), None);
}

#[test]
fn keys_with_no_legacy_spelling_are_always_reported() {
  // F13 and up, the keypad, and the media keys have no legacy escape code at
  // all, so disambiguate mode alone must report them.
  let cases: &[(&str, &[u8])] = &[
    ("f13", b"\x1b[57376u"),
    ("f20", b"\x1b[57383u"),
    ("f35", b"\x1b[57398u"),
    ("kp_0", b"\x1b[57399u"),
    ("kp_9", b"\x1b[57408u"),
    ("kp_enter", b"\x1b[57414u"),
    ("kp_delete", b"\x1b[57426u"),
    ("capslock", b"\x1b[57358u"),
    ("numlock", b"\x1b[57360u"),
    ("printscreen", b"\x1b[57361u"),
    ("pause", b"\x1b[57362u"),
    ("menu", b"\x1b[57363u"),
    ("media_play", b"\x1b[57428u"),
    ("mute_volume", b"\x1b[57440u"),
  ];
  for (key, want) in cases {
    assert_eq!(press(key, NONE, DIS).unwrap(), *want, "{key}");
  }
  // The keypad's "begin" key is the exception: it does have a legacy shape.
  assert_eq!(press("kp_begin", NONE, DIS).unwrap(), b"\x1b[E");
  assert_eq!(press("kp_begin", CTRL, DIS).unwrap(), b"\x1b[1;5E");
}

#[test]
fn f_key_numbering_stops_where_the_table_does() {
  assert!(press("f36", NONE, DIS).is_none());
  assert!(press("f0", NONE, DIS).is_none());
  // f1-f12 have legacy spellings and stay with the legacy encoder.
  assert_eq!(press("f12", NONE, DIS), None);
}

#[test]
fn modifier_keys_are_reported_only_in_all_keys_mode() {
  assert_eq!(press("left_shift", NONE, DIS), None);
  assert_eq!(press("left_shift", NONE, DIS | EVENTS), None);

  let cases: &[(&str, &[u8])] = &[
    ("left_shift", b"\x1b[57441u"),
    ("left_control", b"\x1b[57442u"),
    ("left_alt", b"\x1b[57443u"),
    ("left_super", b"\x1b[57444u"),
    ("right_shift", b"\x1b[57447u"),
    ("right_meta", b"\x1b[57452u"),
    ("iso_level5_shift", b"\x1b[57454u"),
  ];
  for (key, want) in cases {
    assert_eq!(press(key, NONE, ALL).unwrap(), *want, "{key}");
  }
}

#[test]
fn a_modifier_key_release_is_reported_with_event_types() {
  // This is how a program watching for a bare ctrl tap sees it end.
  assert_eq!(
    encode("left_control", None, NONE, ALL | EVENTS, KeyEvent::Release).unwrap(),
    b"\x1b[57442;1:3u"
  );
}

#[test]
fn functional_keys_report_when_the_legacy_form_runs_out_of_room() {
  // Legacy `CSI 1;m X` has bits for shift/alt/ctrl only, so those stay with
  // the legacy encoder...
  assert_eq!(press("up", CTRL, DIS), None);
  assert_eq!(press("delete", ALT, DIS), None);
  // ...and anything it cannot spell comes here instead.
  assert_eq!(press("up", CMD, DIS).unwrap(), b"\x1b[1;9A");
  assert_eq!(press("delete", CMD, DIS).unwrap(), b"\x1b[3;9~");
  assert_eq!(press("f3", CMD, DIS).unwrap(), b"\x1b[13;9~");
  assert_eq!(press("home", HYPER, DIS).unwrap(), b"\x1b[1;17H");
  assert_eq!(press("f1", CAPS, DIS).unwrap(), b"\x1b[1;65P");
}

#[test]
fn a_functional_key_release_carries_the_event_type() {
  // The press keeps its legacy bytes; only the release needs this path.
  assert_eq!(press("up", NONE, DIS | EVENTS), None);
  assert_eq!(
    encode("up", None, NONE, DIS | EVENTS, KeyEvent::Release).unwrap(),
    b"\x1b[1;1:3A"
  );
  assert_eq!(
    encode("pageup", None, NONE, DIS | EVENTS, KeyEvent::Release).unwrap(),
    b"\x1b[5;1:3~"
  );
}

#[test]
fn alternate_keys_report_the_shifted_character() {
  // shift+a types "A": the base key code stays 97, with 65 alongside it.
  assert_eq!(typed("a", "A", SHIFT, ALL | ALTERNATE).unwrap(), b"\x1b[97:65;2u");
  // Without the flag the alternate is tracked but not written.
  assert_eq!(typed("a", "A", SHIFT, ALL).unwrap(), b"\x1b[97;2u");
  // No shift, no alternate.
  assert_eq!(typed("a", "a", NONE, ALL | ALTERNATE).unwrap(), b"\x1b[97u");
  // A shifted character equal to the base adds nothing.
  assert_eq!(typed("a", "a", SHIFT, ALL | ALTERNATE).unwrap(), b"\x1b[97;2u");
}

#[test]
fn associated_text_is_appended_as_codepoints() {
  assert_eq!(typed("a", "a", NONE, ALL | ASSOC).unwrap(), b"\x1b[97;1;97u");
  assert_eq!(typed("a", "A", SHIFT, ALL | ASSOC).unwrap(), b"\x1b[97;2;65u");
  // Multi-codepoint text is colon-separated.
  assert_eq!(
    typed("e", "e\u{301}", NONE, ALL | ASSOC).unwrap(),
    b"\x1b[101;1;101:769u"
  );
  // A release typed nothing, so it carries no text.
  assert_eq!(
    encode("a", Some("a"), NONE, ALL | ASSOC | EVENTS, KeyEvent::Release).unwrap(),
    b"\x1b[97;1:3u"
  );
}

#[test]
fn alternates_and_text_appear_together_in_field_order() {
  // `CSI code:shifted ; mods:event ; text u` — every field in its place.
  assert_eq!(
    encode(
      "a",
      Some("A"),
      SHIFT,
      ALL | ALTERNATE | ASSOC | EVENTS,
      KeyEvent::Repeat
    )
    .unwrap(),
    b"\x1b[97:65;2:2;65u"
  );
}

#[test]
fn a_non_text_key_carries_no_associated_text() {
  // Only text keys have text; a functional key's field would be meaningless.
  assert_eq!(press("up", CMD, ALL | ASSOC).unwrap(), b"\x1b[1;9A");
  assert_eq!(press("f13", NONE, ALL | ASSOC).unwrap(), b"\x1b[57376u");
}

#[test]
fn an_unknown_key_name_falls_through_to_the_legacy_encoder() {
  assert_eq!(press("nonesuch", CTRL, ALL), None);
}
