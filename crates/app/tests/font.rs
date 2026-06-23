use super::*;

fn feats(entries: &[&str]) -> Vec<(String, u32)> {
    let owned: Vec<String> = entries.iter().map(|s| s.to_string()).collect();
    features(&owned).0.as_ref().clone()
}

#[test]
fn parses_sign_and_value_forms() {
    assert_eq!(
        feats(&["liga", "+ss01", "-calt", "cv01=2"]),
        vec![
            ("liga".into(), 1),
            ("ss01".into(), 1),
            ("calt".into(), 0),
            ("cv01".into(), 2),
        ]
    );
}

#[test]
fn skips_invalid_entries() {
    // Too-long tag, empty, and non-numeric value are dropped.
    assert_eq!(feats(&["toolongtag", "", "x=abc"]), Vec::new());
}

#[test]
fn build_uses_primary_and_fallbacks() {
    let mut opts = config::Options::default();
    opts.font_family = vec!["JetBrains Mono".into(), "Menlo".into()];
    let font = build(&opts);
    assert_eq!(font.family.as_ref(), "JetBrains Mono");
    let fb = font.fallbacks.expect("fallbacks");
    assert_eq!(fb.fallback_list(), ["Menlo"]);
}

#[test]
fn build_defaults_to_menlo_without_config() {
    let font = build(&config::Options::default());
    assert_eq!(font.family.as_ref(), "Menlo");
    assert!(font.fallbacks.is_none());
}
