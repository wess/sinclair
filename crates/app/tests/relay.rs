use super::*;

#[test]
fn plain_values_are_single_quoted() {
    assert_eq!(sh_quote("claude"), "'claude'");
    assert_eq!(sh_quote("my agent"), "'my agent'");
}

#[test]
fn shell_metacharacters_are_neutralized() {
    // Each stays inside one quoted word — no command runs.
    assert_eq!(sh_quote("a; rm -rf /"), "'a; rm -rf /'");
    assert_eq!(sh_quote("$(whoami)"), "'$(whoami)'");
    assert_eq!(sh_quote("a && b"), "'a && b'");
}

#[test]
fn embedded_single_quotes_are_escaped() {
    // The close/escape/reopen dance keeps the value a single shell token.
    assert_eq!(sh_quote("x'; rm -rf /;'"), "'x'\\''; rm -rf /;'\\'''");
}
