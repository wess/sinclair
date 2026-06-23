use super::*;

#[test]
fn rgb_widens_by_replication() {
    assert_eq!(format_rgb((255, 0, 128)), "rgb:ffff/0000/8080");
    assert_eq!(format_rgb((16, 32, 48)), "rgb:1010/2020/3030");
}

#[test]
fn base64_roundtrips() {
    for s in [
        "",
        "f",
        "fo",
        "foo",
        "foob",
        "fooba",
        "foobar",
        "hello, world",
    ] {
        let enc = base64_encode(s.as_bytes());
        assert_eq!(base64_decode(enc.as_bytes()).unwrap(), s.as_bytes(), "{s}");
    }
}

#[test]
fn base64_known_vectors() {
    assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    assert_eq!(base64_decode(b"Zm9vYmFy").unwrap(), b"foobar");
    // Whitespace is tolerated.
    assert_eq!(base64_decode(b"Zm9v\nYmFy").unwrap(), b"foobar");
}

#[test]
fn base64_rejects_garbage() {
    assert_eq!(base64_decode(b"@@@"), None);
    assert_eq!(base64_decode(b"Zm9v!"), None);
}

#[test]
fn hex_roundtrips() {
    assert_eq!(hex_encode(b"Co"), "436f");
    assert_eq!(hex_decode(b"436f").unwrap(), b"Co");
    assert_eq!(hex_decode(b"524742").unwrap(), b"RGB");
    assert_eq!(hex_decode(b"abc"), None); // odd length
    assert_eq!(hex_decode(b"zz"), None); // non-hex
}
