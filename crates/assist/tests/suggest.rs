use super::*;

fn v(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

#[test]
fn complete_prefix_dedups_and_keeps_order() {
    let c = v(&["git status", "git stash", "git status", "cargo test"]);
    assert_eq!(complete("git st", &c, 10), v(&["git status", "git stash"]));
    assert_eq!(complete("", &c, 10), Vec::<String>::new());
    // A candidate equal to the input is not a completion.
    assert_eq!(complete("git status", &c, 10), Vec::<String>::new());
}

#[test]
fn ghost_returns_suffix_of_best() {
    let c = v(&["cargo test", "cargo build"]);
    assert_eq!(ghost("car", &c).as_deref(), Some("go test"));
    assert_eq!(ghost("zzz", &c), None);
}

#[test]
fn rank_puts_closest_completion_first() {
    let c = v(&["git status --short", "git st", "git stash"]);
    // "git st" is already the input, so it's excluded; shortest suffix wins.
    assert_eq!(
        rank("git st", &c, 10),
        v(&["git stash", "git status --short"])
    );
}
