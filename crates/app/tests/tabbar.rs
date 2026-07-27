use super::*;

#[test]
fn blend_endpoints_and_midpoint() {
    let a = Rgb::new(0, 0, 0);
    let b = Rgb::new(255, 255, 255);
    assert_eq!(blend(a, b, 0.0), a);
    assert_eq!(blend(a, b, 1.0), b);
    assert_eq!(blend(a, b, 0.5), Rgb::new(128, 128, 128));
    // Out-of-range t clamps.
    assert_eq!(blend(a, b, -1.0), a);
    assert_eq!(blend(a, b, 2.0), b);
}

#[test]
fn blend_mixes_channels_independently() {
    let a = Rgb::new(10, 200, 0);
    let b = Rgb::new(20, 100, 255);
    let m = blend(a, b, 0.1);
    assert_eq!(m, Rgb::new(11, 190, 26));
}

#[test]
fn strips_a_shell_login_prefix() {
    assert_eq!(strip_host("wess@wess:~/Desktop/Dev/sinclair"), "~/Desktop/Dev/sinclair");
    assert_eq!(strip_host("wess@wess:~"), "~");
    assert_eq!(strip_host("root@1cffe899fb41:/work"), "/work");
}

#[test]
fn leaves_titles_that_only_look_similar() {
    // A URL: everything before the first colon is a scheme, not a login.
    assert_eq!(strip_host("https://example.com:8080"), "https://example.com:8080");
    // A program prefix.
    assert_eq!(strip_host("nvim: src/main.rs"), "nvim: src/main.rs");
    // A path before the colon.
    assert_eq!(strip_host("/srv/a@b:c"), "/srv/a@b:c");
    // No colon at all.
    assert_eq!(strip_host("wess@wess"), "wess@wess");
    // Nothing after the colon to keep.
    assert_eq!(strip_host("a@b:"), "a@b:");
    // Plain paths pass through.
    assert_eq!(strip_host("~/Desktop/Dev/sinclair"), "~/Desktop/Dev/sinclair");
    assert_eq!(strip_host(""), "");
}
