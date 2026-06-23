use super::*;
use MouseAction::{Motion, Press, Release};
use MouseButton::{Left, Middle, Right, WheelDown, WheelUp};

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
const ALL: Mods = Mods {
    shift: true,
    alt: true,
    ctrl: true,
    cmd: false,
};

#[test]
fn sgr_press_release_motion() {
    let cases: &[(MouseAction, MouseButton, u32, u32, Mods, &str)] = &[
        (Press, Left, 1, 1, NONE, "\x1b[<0;1;1M"),
        (Press, Middle, 10, 5, NONE, "\x1b[<1;10;5M"),
        (Press, Right, 80, 24, NONE, "\x1b[<2;80;24M"),
        (Release, Left, 1, 1, NONE, "\x1b[<0;1;1m"),
        (Release, Right, 80, 24, NONE, "\x1b[<2;80;24m"),
        (Motion, Left, 3, 4, NONE, "\x1b[<32;3;4M"),
        (Motion, MouseButton::None, 7, 8, NONE, "\x1b[<35;7;8M"),
        // Coordinates beyond the legacy clamp pass through in SGR.
        (Press, Left, 500, 400, NONE, "\x1b[<0;500;400M"),
    ];
    for (action, button, col, row, mods, want) in cases {
        assert_eq!(
            encode_mouse(*action, *button, *col, *row, *mods, true).as_deref(),
            Some(want.as_bytes()),
            "{action:?} {button:?} ({col},{row})"
        );
    }
}

#[test]
fn sgr_modifier_sums() {
    let cases: &[(MouseAction, MouseButton, Mods, &str)] = &[
        (Press, Left, SHIFT, "\x1b[<4;1;1M"),
        (Press, Left, ALT, "\x1b[<8;1;1M"),
        (Press, Left, CTRL, "\x1b[<16;1;1M"),
        (Press, Left, ALL, "\x1b[<28;1;1M"),
        (Press, Right, CTRL, "\x1b[<18;1;1M"),
        (Release, Middle, ALT, "\x1b[<9;1;1m"),
        (Motion, Left, CTRL, "\x1b[<48;1;1M"),
        (Motion, MouseButton::None, SHIFT, "\x1b[<39;1;1M"),
    ];
    for (action, button, mods, want) in cases {
        assert_eq!(
            encode_mouse(*action, *button, 1, 1, *mods, true).as_deref(),
            Some(want.as_bytes()),
            "{action:?} {button:?} {mods:?}"
        );
    }
}

#[test]
fn sgr_wheel() {
    let cases: &[(MouseButton, Mods, &str)] = &[
        (WheelUp, NONE, "\x1b[<64;10;5M"),
        (WheelDown, NONE, "\x1b[<65;10;5M"),
        (WheelUp, SHIFT, "\x1b[<68;10;5M"),
        (WheelDown, ALL, "\x1b[<93;10;5M"),
    ];
    for (button, mods, want) in cases {
        assert_eq!(
            encode_mouse(Press, *button, 10, 5, *mods, true).as_deref(),
            Some(want.as_bytes()),
            "{button:?} {mods:?}"
        );
    }
}

#[test]
fn wheel_release_is_none_both_encodings() {
    for button in [WheelUp, WheelDown] {
        assert_eq!(encode_mouse(Release, button, 1, 1, NONE, true), None);
        assert_eq!(encode_mouse(Release, button, 1, 1, NONE, false), None);
    }
}

#[test]
fn legacy_bytes() {
    let cases: &[(MouseAction, MouseButton, u32, u32, Mods, [u8; 6])] = &[
        // 32+code, 32+col, 32+row
        (Press, Left, 1, 1, NONE, [0x1b, b'[', b'M', 32, 33, 33]),
        (Press, Middle, 10, 5, NONE, [0x1b, b'[', b'M', 33, 42, 37]),
        (Press, Right, 1, 1, NONE, [0x1b, b'[', b'M', 34, 33, 33]),
        // Modifiers: shift 4, alt 8, ctrl 16.
        (Press, Left, 1, 1, CTRL, [0x1b, b'[', b'M', 48, 33, 33]),
        (Press, Left, 1, 1, ALL, [0x1b, b'[', b'M', 60, 33, 33]),
        // Release is always button 3 (+ mods), regardless of button.
        (Release, Left, 1, 1, NONE, [0x1b, b'[', b'M', 35, 33, 33]),
        (Release, Right, 1, 1, NONE, [0x1b, b'[', b'M', 35, 33, 33]),
        (Release, Middle, 1, 1, CTRL, [0x1b, b'[', b'M', 51, 33, 33]),
        // Motion adds 32.
        (Motion, Left, 2, 3, NONE, [0x1b, b'[', b'M', 64, 34, 35]),
        (
            Motion,
            MouseButton::None,
            2,
            3,
            NONE,
            [0x1b, b'[', b'M', 67, 34, 35],
        ),
        // Wheel presses: 64/65.
        (Press, WheelUp, 1, 1, NONE, [0x1b, b'[', b'M', 96, 33, 33]),
        (Press, WheelDown, 1, 1, NONE, [0x1b, b'[', b'M', 97, 33, 33]),
    ];
    for (action, button, col, row, mods, want) in cases {
        assert_eq!(
            encode_mouse(*action, *button, *col, *row, *mods, false).as_deref(),
            Some(&want[..]),
            "{action:?} {button:?} ({col},{row}) {mods:?}"
        );
    }
}

#[test]
fn legacy_clamps_coordinates() {
    // 223 -> byte 255; anything larger clamps, never None.
    let cases: &[(u32, u32, u8, u8)] = &[
        (223, 223, 255, 255),
        (224, 224, 255, 255),
        (10_000, 1, 255, 33),
        (1, 10_000, 33, 255),
        // 1-based floor: 0 clamps up to 1.
        (0, 0, 33, 33),
    ];
    for (col, row, want_col, want_row) in cases {
        assert_eq!(
            encode_mouse(Press, Left, *col, *row, NONE, false).unwrap(),
            vec![0x1b, b'[', b'M', 32, *want_col, *want_row],
            "({col},{row})"
        );
    }
}

#[test]
fn scroll_arrows_normal_mode() {
    assert_eq!(encode_scroll_arrows(true, 1, false), b"\x1b[A");
    assert_eq!(encode_scroll_arrows(true, 3, false), b"\x1b[A\x1b[A\x1b[A");
    assert_eq!(encode_scroll_arrows(false, 2, false), b"\x1b[B\x1b[B");
    assert_eq!(encode_scroll_arrows(true, 0, false), b"");
}

#[test]
fn scroll_arrows_app_mode() {
    assert_eq!(encode_scroll_arrows(true, 1, true), b"\x1bOA");
    assert_eq!(encode_scroll_arrows(false, 3, true), b"\x1bOB\x1bOB\x1bOB");
    assert_eq!(encode_scroll_arrows(false, 0, true), b"");
}
