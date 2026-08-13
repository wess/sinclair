use super::*;

fn scan(s: &str) -> Vec<Span> {
    let chars: Vec<char> = s.chars().collect();
    find(&chars)
}

fn texts(s: &str) -> Vec<String> {
    scan(s).into_iter().map(|p| p.text).collect()
}

#[test]
fn finds_a_relative_path() {
    assert_eq!(texts("see src/main.rs for details"), vec!["src/main.rs"]);
}

#[test]
fn finds_an_absolute_path() {
    assert_eq!(
        texts("/usr/local/bin/sinclair"),
        vec!["/usr/local/bin/sinclair"]
    );
}

#[test]
fn finds_a_home_relative_path() {
    assert_eq!(
        texts("~/.config/sinclair/settings.json"),
        vec!["~/.config/sinclair/settings.json"]
    );
    // A bare `~` is not worth a filesystem round trip.
    assert!(texts("~").is_empty());
}

#[test]
fn splits_a_line_and_column_suffix() {
    let hits = scan("error: crates/vt/src/path.rs:42:9: bad");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].text, "crates/vt/src/path.rs");
    assert_eq!(hits[0].line, Some(42));
    assert_eq!(hits[0].column, Some(9));
}

#[test]
fn splits_a_bare_line_suffix() {
    let hits = scan("at src/main.rs:7");
    assert_eq!(hits[0].text, "src/main.rs");
    assert_eq!(hits[0].line, Some(7));
    assert_eq!(hits[0].column, None);
}

#[test]
fn the_span_covers_the_line_suffix_too() {
    // The highlight has to cover what the user pointed at, suffix included,
    // or clicking the `:42` would miss the path it belongs to.
    let hits = scan("src/main.rs:42");
    assert_eq!(hits[0].start, 0);
    assert_eq!(hits[0].end, "src/main.rs:42".chars().count());
}

#[test]
fn a_bare_word_is_not_a_path() {
    assert!(texts("README makefile config").is_empty());
}

#[test]
fn urls_are_left_to_the_url_scanner() {
    assert!(texts("https://example.com/a/b").is_empty());
}

#[test]
fn does_not_restart_inside_a_longer_run() {
    // Without a boundary check the tail of a URL would come back as a path.
    assert!(texts("ftp://host/dir/file").is_empty());
}

#[test]
fn trailing_sentence_punctuation_is_dropped() {
    assert_eq!(texts("edit src/main.rs."), vec!["src/main.rs"]);
    assert_eq!(texts("in src/lib.rs, then"), vec!["src/lib.rs"]);
}

#[test]
fn finds_several_paths_in_a_line() {
    assert_eq!(
        texts("cp src/a.txt dst/b.txt"),
        vec!["src/a.txt", "dst/b.txt"]
    );
}

#[test]
fn keeps_a_path_with_a_dotted_name() {
    assert_eq!(
        texts("./target/debug/deps/vt-1a2b3c.d"),
        vec!["./target/debug/deps/vt-1a2b3c.d"]
    );
}

#[test]
fn a_windows_drive_letter_is_not_a_line_number() {
    let hits = scan("C:/src/main.rs");
    assert_eq!(hits[0].text, "C:/src/main.rs");
    assert_eq!(hits[0].line, None);
}
