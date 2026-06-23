use super::*;

fn urls(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    find(&chars)
        .into_iter()
        .map(|(a, b)| chars[a..b].iter().collect())
        .collect()
}

#[test]
fn finds_basic_urls() {
    assert_eq!(urls("see https://example.com now"), ["https://example.com"]);
    assert_eq!(urls("http://a.b/c?d=1&e=2"), ["http://a.b/c?d=1&e=2"]);
    assert_eq!(urls("mailto:me@x.io"), ["mailto:me@x.io"]);
}

#[test]
fn trims_trailing_punctuation() {
    assert_eq!(urls("go to https://x.io."), ["https://x.io"]);
    assert_eq!(urls("(see https://x.io)"), ["https://x.io"]);
    assert_eq!(urls("\"https://x.io\","), ["https://x.io"]);
}

#[test]
fn keeps_balanced_parens() {
    assert_eq!(
        urls("https://en.wikipedia.org/wiki/Foo_(bar)"),
        ["https://en.wikipedia.org/wiki/Foo_(bar)"]
    );
}

#[test]
fn multiple_and_none() {
    assert_eq!(
        urls("a https://1.com b http://2.com"),
        ["https://1.com", "http://2.com"]
    );
    assert!(urls("no links here").is_empty());
    assert!(urls("https://").is_empty()); // scheme only
    assert!(urls("nothttp://x").len() == 1); // still matches the embedded one
}

#[test]
fn case_insensitive_scheme() {
    assert_eq!(urls("HTTPS://X.IO/p"), ["HTTPS://X.IO/p"]);
}
