use super::*;

fn cfg() -> SuggestConfig {
    SuggestConfig {
        ghost: true,
        popup: true,
        tab: true,
        ai: false,
        history: true,
        commands: true,
        paths: false,
        assist: true,
    }
}

#[test]
fn candidates_prefer_history_then_common() {
    let hist = vec!["git commit -m wip".to_string(), "cargo test".to_string()];
    let out = candidates(&cfg(), "car", &hist, None);
    assert!(out.iter().any(|c| c == "cargo test"));
    // "car" should surface cargo-* entries only.
    assert!(out.iter().all(|c| c.starts_with("car")));
}

#[test]
fn ghost_is_suffix_of_top_candidate() {
    // History-only, recency order: the one history entry completes the input.
    let hist = vec!["git status".to_string()];
    let only_hist = SuggestConfig {
        assist: false,
        commands: false,
        ..cfg()
    };
    let cands = candidates(&only_hist, "git st", &hist, None);
    assert_eq!(ghost(&cands, "git st").as_deref(), Some("atus"));
}

#[test]
fn empty_input_yields_nothing() {
    assert!(candidates(&cfg(), "   ", &[], None).is_empty());
    assert_eq!(ghost(&[], ""), None);
}
