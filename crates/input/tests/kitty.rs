use super::*;
use crate::KeyEvent;

const NONE: Mods = Mods {
    shift: false,
    alt: false,
    ctrl: false,
    cmd: false,
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

const DIS: u8 = kitty_flags::DISAMBIGUATE;
const ALL: u8 = kitty_flags::REPORT_ALL_KEYS_AS_ESCAPE_CODES;
const EVENTS: u8 = kitty_flags::REPORT_EVENT_TYPES;

/// Encode a plain key press (the default event) — most tests only care about
/// press bytes, so this keeps them terse.
fn press(key: &str, mods: Mods, flags: u8) -> Option<Vec<u8>> {
    encode(key, mods, flags, KeyEvent::Press)
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
    assert_eq!(encode("a", CTRL, DIS, KeyEvent::Repeat).unwrap(), b"\x1b[97;5u");
    assert_eq!(encode("a", CTRL, DIS, KeyEvent::Release), None);
    // With the flag, repeat is `:2` and release is `:3`.
    assert_eq!(
        encode("a", CTRL, DIS | EVENTS, KeyEvent::Repeat).unwrap(),
        b"\x1b[97;5:2u"
    );
    assert_eq!(
        encode("a", CTRL, DIS | EVENTS, KeyEvent::Release).unwrap(),
        b"\x1b[97;5:3u"
    );
}

#[test]
fn event_type_forces_the_modifier_field_when_unmodified() {
    // An unmodified all-keys press is `CSI 97 u`; its release carries `;1:3`
    // because the event sub-parameter needs the modifiers field present.
    assert_eq!(
        encode("a", NONE, ALL | EVENTS, KeyEvent::Press).unwrap(),
        b"\x1b[97u"
    );
    assert_eq!(
        encode("a", NONE, ALL | EVENTS, KeyEvent::Release).unwrap(),
        b"\x1b[97;1:3u"
    );
}

#[test]
fn release_only_for_reported_keys() {
    // A plain letter is not in the disambiguated set, so even with event types
    // on there is no release to report (it never produced a press either).
    assert_eq!(encode("a", NONE, DIS | EVENTS, KeyEvent::Release), None);
}
