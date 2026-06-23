use super::*;

fn run(needle: &str, row: &str, fold: bool) -> Vec<(usize, usize)> {
    let chars: Vec<char> = row.chars().collect();
    let cols: Vec<usize> = (0..chars.len()).collect();
    let needle: Vec<char> = needle.chars().collect();
    in_row(&needle, &chars, &cols, 0, fold, |_| false)
        .into_iter()
        .map(|m| (m.start_col, m.end_col))
        .collect()
}

#[test]
fn finds_all_occurrences() {
    assert_eq!(run("ab", "abXabYab", false), [(0, 1), (3, 4), (6, 7)]);
}

#[test]
fn case_insensitive() {
    assert_eq!(run("hi", "HI there hi", true), [(0, 1), (9, 10)]);
    assert_eq!(run("hi", "HI there hi", false), [(9, 10)]);
}

#[test]
fn no_match_and_empty() {
    assert!(run("zz", "abc", false).is_empty());
    assert!(run("", "abc", false).is_empty());
    assert!(run("abcd", "abc", false).is_empty());
}

#[test]
fn non_overlapping() {
    // "aa" in "aaaa" yields two matches, not three.
    assert_eq!(run("aa", "aaaa", false), [(0, 1), (2, 3)]);
}
