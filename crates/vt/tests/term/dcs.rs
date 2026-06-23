use crate::term::Terminal;

#[test]
fn xtgettcap_reports_colors() {
    let mut t = Terminal::new(10, 3, 0);
    // "Co" -> 436f; value "256" -> 323536.
    t.feed(b"\x1bP+q436f\x1b\\");
    assert_eq!(t.take_output(), b"\x1bP1+r436f=323536\x1b\\");
}

#[test]
fn xtgettcap_reports_terminal_name() {
    let mut t = Terminal::new(10, 3, 0);
    // "TN" -> 544e.
    t.feed(b"\x1bP+q544e\x1b\\");
    let out = t.take_output();
    let want = format!(
        "\x1bP1+r544e={}\x1b\\",
        super::hex_encode(b"xterm-256color")
    );
    assert_eq!(out, want.as_bytes());
}

#[test]
fn xtgettcap_boolean_has_no_value() {
    let mut t = Terminal::new(10, 3, 0);
    // "Tc" -> 5463.
    t.feed(b"\x1bP+q5463\x1b\\");
    assert_eq!(t.take_output(), b"\x1bP1+r5463\x1b\\");
}

#[test]
fn xtgettcap_unknown_is_zero() {
    let mut t = Terminal::new(10, 3, 0);
    // "zz" -> 7a7a.
    t.feed(b"\x1bP+q7a7a\x1b\\");
    assert_eq!(t.take_output(), b"\x1bP0+r7a7a\x1b\\");
}

#[test]
fn xtgettcap_multiple_caps() {
    let mut t = Terminal::new(10, 3, 0);
    // "Co;RGB" -> 436f;524742.
    t.feed(b"\x1bP+q436f;524742\x1b\\");
    assert_eq!(
        t.take_output(),
        b"\x1bP1+r436f=323536\x1b\\\x1bP1+r524742=382f382f38\x1b\\"
    );
}

#[test]
fn non_xtgettcap_dcs_ignored() {
    let mut t = Terminal::new(10, 3, 0);
    t.feed(b"\x1bPsomething\x1b\\ok");
    assert!(t.take_output().is_empty());
    assert_eq!(t.row_text(0), "ok");
}
