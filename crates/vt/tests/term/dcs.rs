use crate::term::Terminal;

#[test]
fn xtgettcap_reports_colors() {
    let mut t = Terminal::new(10, 3, 0);
    // "Co" -> 436f; value "256" -> 323536.
    t.feed(b"\x1bP+q436f\x1b\\");
    assert_eq!(t.take_output(), b"\x1bP1+r436f=323536\x1b\\");
}

#[test]
fn sixel_dcs_decodes_and_anchors_an_image() {
    let mut t = Terminal::new(10, 6, 100);
    t.set_cell_pixels(8, 16);
    // DCS q <red sixel> ST: one red pixel column, six pixels tall.
    t.feed(b"\x1bPq#0;2;100;0;0@\x1b\\");
    let images = t.images();
    assert_eq!(images.len(), 1);
    assert_eq!(images[0].image.width, 1);
    assert_eq!(images[0].image.height, 6);
    assert_eq!(images[0].line, 0); // anchored at the starting row
    // A 6px-tall image is one 16px row; the cursor moved to the next line.
    assert_eq!(t.cursor_pos().0, 1);
}

#[test]
fn sixel_image_scrolls_into_history() {
    let mut t = Terminal::new(4, 2, 100);
    t.set_cell_pixels(8, 16);
    t.feed(b"\x1bPq#0;2;0;100;0@\x1b\\");
    assert_eq!(t.images()[0].line, 0);
    // Push two lines: the image anchor follows the text up into scrollback.
    t.feed(b"\n\n");
    assert!(t.images()[0].line < 0);
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
fn xtgettcap_non_hex_query_is_not_echoed() {
    let mut t = Terminal::new(10, 3, 0);
    // A non-hex payload (here containing a newline) must not be reflected back
    // into the pty: a reply carrying control bytes would be read as shell input.
    t.feed(b"\x1bP+qZZ\nrm\x1b\\");
    assert!(t.take_output().is_empty());
}

#[test]
fn non_xtgettcap_dcs_ignored() {
    let mut t = Terminal::new(10, 3, 0);
    t.feed(b"\x1bPsomething\x1b\\ok");
    assert!(t.take_output().is_empty());
    assert_eq!(t.row_text(0), "ok");
}
