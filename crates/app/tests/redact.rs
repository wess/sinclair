use super::*;

fn bullets(n: usize) -> String {
    "\u{2022}".repeat(n)
}

#[test]
fn masks_matches_with_bullets() {
    let r = Redactor::compile(&["sk-[A-Za-z0-9]{4,}".to_string()]);
    let secret = "sk-abcd1234";
    assert_eq!(
        r.mask("key sk-abcd1234 end"),
        format!("key {} end", bullets(secret.len()))
    );
}

#[test]
fn multiple_patterns() {
    let r = Redactor::compile(&["ghp_[a-z]+".to_string(), "AKIA[0-9]+".to_string()]);
    let out = r.mask("t ghp_abc and AKIA9");
    assert_eq!(
        out,
        format!(
            "t {} and {}",
            bullets("ghp_abc".len()),
            bullets("AKIA9".len())
        )
    );
}

#[test]
fn empty_redactor_is_identity() {
    let r = Redactor::compile(&[]);
    assert!(r.is_empty());
    assert_eq!(r.mask("nothing to hide"), "nothing to hide");
}

#[test]
fn invalid_pattern_is_skipped() {
    // The bad pattern is dropped; the good one still masks.
    let r = Redactor::compile(&["(".to_string(), "secret".to_string()]);
    assert_eq!(
        r.mask("a secret b"),
        format!("a {} b", bullets("secret".len()))
    );
}
