use super::*;

#[test]
fn arg_shapes() {
    assert_eq!(Mount::rw("/a", "/b").arg(), "/a:/b");
    assert_eq!(Mount::ro("/a", "/b").arg(), "/a:/b:ro");
}

#[test]
fn identity_mounts_at_itself() {
    let m = Mount::identity("/Users/wess/code");
    assert_eq!(m.source, m.target);
    assert!(m.is_identity());
    assert_eq!(m.arg(), "/Users/wess/code:/Users/wess/code");
}

#[test]
fn parse_forms() {
    assert_eq!(Mount::parse("/repo").unwrap(), Mount::identity("/repo"));
    assert_eq!(Mount::parse("/a:/b").unwrap(), Mount::rw("/a", "/b"));
    assert_eq!(Mount::parse(" /a : /b : ro ").unwrap(), Mount::ro("/a", "/b"));
    assert_eq!(Mount::parse("cache:/work/.cache").unwrap().source, "cache");
}

#[test]
fn parse_rejects_bad_entries() {
    assert!(Mount::parse("").is_err());
    assert!(Mount::parse("/a:rel").is_err());
    assert!(Mount::parse("/a:/b:wat").is_err());
    assert!(Mount::parse("/a:/b:ro:extra").is_err());
    assert!(Mount::parse(":/b").is_err());
}
