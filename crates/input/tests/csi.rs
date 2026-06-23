use super::*;

fn mods(shift: bool, alt: bool, ctrl: bool) -> Mods {
    Mods {
        shift,
        alt,
        ctrl,
        cmd: false,
    }
}

#[test]
fn modifier_param_table() {
    // (shift, alt, ctrl) -> 1 + shift*1 + alt*2 + ctrl*4
    let cases = [
        ((false, false, false), 1),
        ((true, false, false), 2),
        ((false, true, false), 3),
        ((true, true, false), 4),
        ((false, false, true), 5),
        ((true, false, true), 6),
        ((false, true, true), 7),
        ((true, true, true), 8),
    ];
    for ((s, a, c), want) in cases {
        assert_eq!(modifier_param(mods(s, a, c)), want, "({s},{a},{c})");
    }
}

#[test]
fn cursor_forms() {
    assert_eq!(cursor(b'A', false), b"\x1b[A");
    assert_eq!(cursor(b'A', true), b"\x1bOA");
    assert_eq!(cursor_modified(b'A', mods(true, false, true)), b"\x1b[1;6A");
}

#[test]
fn tilde_forms() {
    assert_eq!(tilde(3, mods(false, false, false)), b"\x1b[3~");
    assert_eq!(tilde(3, mods(false, false, true)), b"\x1b[3;5~");
    assert_eq!(tilde(15, mods(true, true, true)), b"\x1b[15;8~");
}

#[test]
fn ss3_form() {
    assert_eq!(ss3(b'P'), b"\x1bOP");
}
