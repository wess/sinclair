use super::*;

#[test]
fn lookup() {
    assert_eq!(kind("font-size"), Some(Kind::Float));
    assert_eq!(kind("copy-on-select"), Some(Kind::Bool));
    assert_eq!(kind("keybind"), Some(Kind::List));
    assert_eq!(kind("scrollback-limit"), Some(Kind::Int));
    assert_eq!(kind("theme"), Some(Kind::Str));
    assert_eq!(kind("bogus"), None);
}

#[test]
fn every_key_is_known_to_apply() {
    // An empty value resets a key to its default, so it must be accepted for
    // every key in the table — anything else means the table and `apply`
    // drifted apart.
    let defaults = crate::Options::default();
    let mut opts = defaults.clone();
    for (key, _) in KEYS {
        assert!(
            crate::apply::apply(&mut opts, &defaults, key, "").is_ok(),
            "`{key}` is in the kind table but unknown to apply()"
        );
    }
}

#[test]
fn keys_are_unique() {
    for (i, (a, _)) in KEYS.iter().enumerate() {
        assert!(
            !KEYS[i + 1..].iter().any(|(b, _)| a == b),
            "duplicate key `{a}`"
        );
    }
}
