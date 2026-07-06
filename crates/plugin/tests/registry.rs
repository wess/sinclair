use super::*;

#[test]
fn sha256_is_stable_hex() {
    // Known vector: SHA-256("abc").
    assert_eq!(
        sha256_hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn verify_matches_and_rejects() {
    let bytes = b"plugin bytes";
    let good = sha256_hex(bytes);
    assert!(verify_sha256(bytes, &good));
    assert!(verify_sha256(bytes, &good.to_uppercase())); // case-insensitive
    assert!(!verify_sha256(bytes, "deadbeef"));
    assert!(verify_sha256(bytes, "")); // unpinned is accepted
}

#[test]
fn registry_index_parses() {
    let text = r#"
[plugins.git]
version = "0.2.0"
url = "https://example.com/git"
sha256 = "abc123"
description = "Git panel"
capabilities = ["commands", "filesystem"]
"#;
    let registry = Registry::parse(text).unwrap();
    assert_eq!(registry.names(), vec!["git".to_string()]);
    let entry = registry.get("git").unwrap();
    assert_eq!(entry.version, "0.2.0");
    assert_eq!(entry.capabilities, vec!["commands", "filesystem"]);
}
