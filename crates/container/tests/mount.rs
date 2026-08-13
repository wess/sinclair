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
    assert_eq!(
        Mount::parse(" /a : /b : ro ").unwrap(),
        Mount::ro("/a", "/b")
    );
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

#[test]
fn parses_the_devcontainer_mount_syntax() {
    let m = Mount::parse_mount_spec("type=bind,source=/host/cache,target=/work/.cache").unwrap();
    assert_eq!(m, Mount::rw("/host/cache", "/work/.cache"));
}

#[test]
fn mount_spec_fields_are_order_free_and_readonly_is_honoured() {
    let m = Mount::parse_mount_spec("target=/etc/x,readonly,source=/host/x,type=bind").unwrap();
    assert_eq!(m, Mount::ro("/host/x", "/etc/x"));
    assert!(
        !Mount::parse_mount_spec("source=/a,target=/b,readonly=false")
            .unwrap()
            .readonly
    );
    // Named volumes use the same shape.
    assert_eq!(
        Mount::parse_mount_spec("type=volume,source=node_modules,target=/work/node_modules")
            .unwrap()
            .source,
        "node_modules"
    );
}

#[test]
fn mount_spec_reports_what_is_missing() {
    assert!(Mount::parse_mount_spec("type=bind,target=/b")
        .unwrap_err()
        .contains("source"));
    assert!(Mount::parse_mount_spec("type=bind,source=/a")
        .unwrap_err()
        .contains("target"));
    assert!(Mount::parse_mount_spec("source=/a,target=rel").is_err());
}
