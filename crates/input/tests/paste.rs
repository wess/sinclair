use super::*;

#[test]
fn bracketed_wraps_text() {
    assert_eq!(encode_paste("hello", true), b"\x1b[200~hello\x1b[201~");
    assert_eq!(encode_paste("", true), b"\x1b[200~\x1b[201~");
}

#[test]
fn bracketed_preserves_newlines_verbatim() {
    assert_eq!(encode_paste("a\nb", true), b"\x1b[200~a\nb\x1b[201~");
}

#[test]
fn bracketed_strips_embedded_end_marker() {
    assert_eq!(
        encode_paste("evil\x1b[201~rm -rf\x1b[201~ /", true),
        b"\x1b[200~evilrm -rf /\x1b[201~"
    );
    // Other escapes pass through; only the end marker is removed.
    assert_eq!(
        encode_paste("ok\x1b[200~still", true),
        b"\x1b[200~ok\x1b[200~still\x1b[201~"
    );
}

#[test]
fn plain_normalizes_newlines_to_cr() {
    let cases: &[(&str, &[u8])] = &[
        ("hello", b"hello"),
        ("a\nb", b"a\rb"),
        ("a\r\nb", b"a\rb"),
        ("a\rb", b"a\rb"),
        ("\n\n", b"\r\r"),
        ("", b""),
    ];
    for (text, want) in cases {
        assert_eq!(encode_paste(text, false), *want, "{text:?}");
    }
}
