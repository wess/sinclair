use super::*;

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
const ALT: Mods = Mods { alt: true, ..NONE };
const CTRL: Mods = Mods { ctrl: true, ..NONE };
const CMD: Mods = Mods { cmd: true, ..NONE };
const CTRL_ALT: Mods = Mods {
    ctrl: true,
    alt: true,
    ..NONE
};
const CTRL_SHIFT: Mods = Mods {
    ctrl: true,
    shift: true,
    ..NONE
};
const ALT_SHIFT: Mods = Mods {
    alt: true,
    shift: true,
    ..NONE
};
const ALL: Mods = Mods {
    shift: true,
    alt: true,
    ctrl: true,
    cmd: false,
};

const NORMAL: TermState = TermState {
    cursor_keys_app: false,
    keypad_app: false,
    bracketed_paste: false,
    kitty_flags: 0,
};
const APP: TermState = TermState {
    cursor_keys_app: true,
    keypad_app: true,
    bracketed_paste: false,
    kitty_flags: 0,
};

fn enc(key: &str, text: Option<&str>, mods: Mods, state: TermState) -> Option<Vec<u8>> {
    encode_key(key, text, mods, state)
}

#[test]
fn cmd_chords_return_none() {
    for key in ["a", "enter", "up", "f5", "delete", "space"] {
        assert_eq!(enc(key, Some("a"), CMD, NORMAL), None, "{key}");
        let with_ctrl = Mods { ctrl: true, ..CMD };
        assert_eq!(enc(key, Some("a"), with_ctrl, NORMAL), None, "{key}");
    }
}

#[test]
fn printable_emits_text() {
    let cases: &[(&str, Option<&str>, Mods, &[u8])] = &[
        ("a", Some("a"), NONE, b"a"),
        ("a", Some("A"), SHIFT, b"A"), // shift pre-resolved by platform
        ("1", Some("1"), NONE, b"1"),
        ("1", Some("!"), SHIFT, b"!"),
        ("/", Some("/"), NONE, b"/"),
        ("space", Some(" "), NONE, b" "),
        ("e", Some("\u{e9}"), NONE, "é".as_bytes()), // dead-key result
        // Fallbacks when the platform supplies no text.
        ("/", None, NONE, b"/"),
        ("space", None, NONE, b" "),
        ("z", None, NONE, b"z"),
    ];
    for (key, text, mods, want) in cases {
        assert_eq!(
            enc(key, *text, *mods, NORMAL).as_deref(),
            Some(*want),
            "{key} {text:?}"
        );
    }
}

#[test]
fn alt_printable_prefixes_esc_for_single_ascii() {
    let cases: &[(&str, Option<&str>, Mods, &[u8])] = &[
        ("a", Some("a"), ALT, b"\x1ba"),
        ("x", Some("X"), ALT_SHIFT, b"\x1bX"),
        ("space", Some(" "), ALT, b"\x1b "),
        ("1", Some("1"), ALT, b"\x1b1"),
        // Non-ASCII alt text passes through unchanged (macOS option).
        ("e", Some("\u{e9}"), ALT, "é".as_bytes()),
        ("o", Some("\u{f8}"), ALT, "ø".as_bytes()),
        // Multi-char text passes through unchanged.
        ("a", Some("ab"), ALT, b"ab"),
    ];
    for (key, text, mods, want) in cases {
        assert_eq!(
            enc(key, *text, *mods, NORMAL).as_deref(),
            Some(*want),
            "{key} {text:?}"
        );
    }
}

#[test]
fn ctrl_letter_full_table() {
    for (i, key) in ('a'..='z').enumerate() {
        let want = vec![i as u8 + 1];
        assert_eq!(
            enc(&key.to_string(), Some(&key.to_string()), CTRL, NORMAL),
            Some(want.clone()),
            "ctrl+{key}"
        );
        // shift+ctrl+letter encodes the same byte.
        assert_eq!(
            enc(
                &key.to_string(),
                Some(&key.to_ascii_uppercase().to_string()),
                CTRL_SHIFT,
                NORMAL
            ),
            Some(want),
            "ctrl+shift+{key}"
        );
    }
    // The two overlaps with C0 names are intentional.
    assert_eq!(enc("i", Some("i"), CTRL, NORMAL).unwrap(), b"\t");
    assert_eq!(enc("m", Some("m"), CTRL, NORMAL).unwrap(), b"\r");
}

#[test]
fn ctrl_punctuation_table() {
    let cases: &[(&str, u8)] = &[
        ("space", 0x00),
        ("@", 0x00),
        ("[", 0x1b),
        ("\\", 0x1c),
        ("]", 0x1d),
        ("^", 0x1e),
        ("_", 0x1f),
        ("-", 0x1f),
        ("?", 0x7f),
        ("8", 0x7f),
    ];
    for (key, byte) in cases {
        assert_eq!(
            enc(key, Some(key), CTRL, NORMAL),
            Some(vec![*byte]),
            "ctrl+{key}"
        );
    }
}

#[test]
fn ctrl_alt_prefixes_esc() {
    assert_eq!(enc("a", Some("a"), CTRL_ALT, NORMAL).unwrap(), b"\x1b\x01");
    assert_eq!(
        enc("space", Some(" "), CTRL_ALT, NORMAL).unwrap(),
        b"\x1b\x00"
    );
}

#[test]
fn ctrl_unmapped_falls_back_to_text() {
    // xterm sends the plain character for ctrl+digit outside the table.
    assert_eq!(enc("1", Some("1"), CTRL, NORMAL).unwrap(), b"1");
}

#[test]
fn enter_tab_escape_backspace() {
    let cases: &[(&str, Mods, &[u8])] = &[
        ("enter", NONE, b"\r"),
        ("enter", ALT, b"\x1b\r"),
        ("tab", NONE, b"\t"),
        ("tab", SHIFT, b"\x1b[Z"),
        ("tab", ALT, b"\x1b\t"),
        ("tab", ALT_SHIFT, b"\x1b\x1b[Z"),
        ("escape", NONE, b"\x1b"),
        ("escape", ALT, b"\x1b\x1b"),
        ("backspace", NONE, b"\x7f"),
        ("backspace", CTRL, b"\x08"),
        ("backspace", ALT, b"\x1b\x7f"),
        ("backspace", CTRL_ALT, b"\x1b\x08"),
    ];
    for (key, mods, want) in cases {
        assert_eq!(
            enc(key, None, *mods, NORMAL).as_deref(),
            Some(*want),
            "{key} {mods:?}"
        );
    }
}

#[test]
fn arrows_normal_and_app_mode() {
    let cases: &[(&str, u8)] = &[
        ("up", b'A'),
        ("down", b'B'),
        ("right", b'C'),
        ("left", b'D'),
    ];
    for (key, fin) in cases {
        assert_eq!(
            enc(key, None, NONE, NORMAL).unwrap(),
            vec![0x1b, b'[', *fin],
            "{key} normal"
        );
        assert_eq!(
            enc(key, None, NONE, APP).unwrap(),
            vec![0x1b, b'O', *fin],
            "{key} app"
        );
    }
}

#[test]
fn modified_arrows_all_combos_force_csi() {
    let combos: &[(Mods, &str)] = &[
        (SHIFT, "2"),
        (ALT, "3"),
        (ALT_SHIFT, "4"),
        (CTRL, "5"),
        (CTRL_SHIFT, "6"),
        (CTRL_ALT, "7"),
        (ALL, "8"),
    ];
    for (mods, m) in combos {
        let want = format!("\x1b[1;{m}A").into_bytes();
        // Same bytes in normal and app mode: modifiers force CSI.
        assert_eq!(enc("up", None, *mods, NORMAL).unwrap(), want, "{mods:?}");
        assert_eq!(enc("up", None, *mods, APP).unwrap(), want, "{mods:?} app");
    }
    assert_eq!(enc("down", None, CTRL, APP).unwrap(), b"\x1b[1;5B");
    assert_eq!(enc("right", None, SHIFT, APP).unwrap(), b"\x1b[1;2C");
    assert_eq!(enc("left", None, ALT, APP).unwrap(), b"\x1b[1;3D");
}

#[test]
fn home_end_forms() {
    assert_eq!(enc("home", None, NONE, NORMAL).unwrap(), b"\x1b[H");
    assert_eq!(enc("end", None, NONE, NORMAL).unwrap(), b"\x1b[F");
    assert_eq!(enc("home", None, NONE, APP).unwrap(), b"\x1bOH");
    assert_eq!(enc("end", None, NONE, APP).unwrap(), b"\x1bOF");
    assert_eq!(enc("home", None, CTRL, APP).unwrap(), b"\x1b[1;5H");
    assert_eq!(enc("end", None, ALL, NORMAL).unwrap(), b"\x1b[1;8F");
}

#[test]
fn tilde_keys() {
    let cases: &[(&str, u8)] = &[("insert", 2), ("delete", 3), ("pageup", 5), ("pagedown", 6)];
    for (key, n) in cases {
        assert_eq!(
            enc(key, None, NONE, NORMAL).unwrap(),
            format!("\x1b[{n}~").into_bytes(),
            "{key}"
        );
        // App-mode state never changes tilde keys.
        assert_eq!(
            enc(key, None, NONE, APP).unwrap(),
            format!("\x1b[{n}~").into_bytes(),
            "{key} app"
        );
    }
    let combos: &[(Mods, &str)] = &[
        (SHIFT, "2"),
        (ALT, "3"),
        (ALT_SHIFT, "4"),
        (CTRL, "5"),
        (CTRL_SHIFT, "6"),
        (CTRL_ALT, "7"),
        (ALL, "8"),
    ];
    for (mods, m) in combos {
        assert_eq!(
            enc("delete", None, *mods, NORMAL).unwrap(),
            format!("\x1b[3;{m}~").into_bytes(),
            "delete {mods:?}"
        );
    }
    assert_eq!(enc("pageup", None, SHIFT, NORMAL).unwrap(), b"\x1b[5;2~");
    assert_eq!(enc("pagedown", None, CTRL, NORMAL).unwrap(), b"\x1b[6;5~");
    assert_eq!(enc("insert", None, ALT, NORMAL).unwrap(), b"\x1b[2;3~");
}

#[test]
fn fkeys_f1_to_f4() {
    let cases: &[(&str, u8)] = &[("f1", b'P'), ("f2", b'Q'), ("f3", b'R'), ("f4", b'S')];
    for (key, fin) in cases {
        assert_eq!(
            enc(key, None, NONE, NORMAL).unwrap(),
            vec![0x1b, b'O', *fin],
            "{key}"
        );
        assert_eq!(
            enc(key, None, CTRL, NORMAL).unwrap(),
            format!("\x1b[1;5{}", *fin as char).into_bytes(),
            "ctrl+{key}"
        );
        assert_eq!(
            enc(key, None, SHIFT, NORMAL).unwrap(),
            format!("\x1b[1;2{}", *fin as char).into_bytes(),
            "shift+{key}"
        );
    }
}

#[test]
fn fkeys_f5_to_f12() {
    let cases: &[(&str, u8)] = &[
        ("f5", 15),
        ("f6", 17),
        ("f7", 18),
        ("f8", 19),
        ("f9", 20),
        ("f10", 21),
        ("f11", 23),
        ("f12", 24),
    ];
    for (key, n) in cases {
        assert_eq!(
            enc(key, None, NONE, NORMAL).unwrap(),
            format!("\x1b[{n}~").into_bytes(),
            "{key}"
        );
        assert_eq!(
            enc(key, None, CTRL_SHIFT, NORMAL).unwrap(),
            format!("\x1b[{n};6~").into_bytes(),
            "ctrl+shift+{key}"
        );
    }
}

#[test]
fn unknown_key_without_text_is_none() {
    assert_eq!(enc("f13", None, NONE, NORMAL), None);
    assert_eq!(enc("menu", None, NONE, NORMAL), None);
}
