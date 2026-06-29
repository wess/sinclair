use super::*;

#[test]
fn web_and_comms_schemes_are_openable() {
    assert!(openable("https://example.com"));
    assert!(openable("http://example.com/path"));
    assert!(openable("ftp://host/file"));
    assert!(openable("mailto:me@example.com"));
    assert!(openable("file:///etc/hosts"));
    // Scheme matching is case-insensitive.
    assert!(openable("HTTPS://example.com"));
}

#[test]
fn dangerous_and_unknown_schemes_are_refused() {
    assert!(!openable("javascript:alert(1)"));
    assert!(!openable("data:text/html,<script>"));
    assert!(!openable("vbscript:msgbox"));
    assert!(!openable("customapp://do-something"));
}

#[test]
fn schemeless_or_pathlike_targets_are_refused() {
    assert!(!openable("example.com"));
    assert!(!openable("/usr/local/bin"));
    assert!(!openable("./relative/path:with-colon"));
    assert!(!openable(""));
}
