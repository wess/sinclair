use super::*;

fn chars(s: &str) -> Vec<char> {
    s.chars().collect()
}

fn tok(s: &str, col: usize) -> Option<String> {
    token_at(&chars(s), col).map(|(a, b)| s.chars().collect::<Vec<_>>()[a..b].iter().collect())
}

#[test]
fn selects_url() {
    let s = "see https://example.com/x here";
    assert_eq!(tok(s, 8).as_deref(), Some("https://example.com/x"));
}

#[test]
fn selects_email() {
    let s = "mail me@wess.io now";
    assert_eq!(tok(s, 9).as_deref(), Some("me@wess.io"));
}

#[test]
fn selects_path() {
    let s = "edit /usr/local/bin/prompt please";
    assert_eq!(tok(s, 10).as_deref(), Some("/usr/local/bin/prompt"));
}

#[test]
fn selects_relative_path() {
    let s = "open crates/vt/src/lib.rs";
    assert_eq!(tok(s, 12).as_deref(), Some("crates/vt/src/lib.rs"));
}

#[test]
fn selects_hash() {
    let s = "at a00103c7 fixed";
    assert_eq!(tok(s, 4).as_deref(), Some("a00103c7"));
}

#[test]
fn plain_word_is_not_a_token() {
    // A bare word (no @, /, and too short/not hex) yields nothing → caller
    // falls back to word selection.
    assert_eq!(tok("just a word", 5), None);
}

#[test]
fn short_hex_is_not_a_hash() {
    assert_eq!(tok("abc def", 0), None);
}

#[test]
fn out_of_range_is_none() {
    assert_eq!(token_at(&chars("hi"), 9), None);
}
