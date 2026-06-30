use super::*;

#[test]
fn builtins_present() {
    let b = builtin();
    assert!(b.iter().any(|p| p.label == "Debian" && p.image == "debian:latest"));
    assert!(b.iter().any(|p| p.label == "Alpine" && p.command == "sh"));
}

#[test]
fn parse_minimal() {
    let p = parse_profile("Debian | debian:latest").unwrap();
    assert_eq!(p.label, "Debian");
    assert_eq!(p.image, "debian:latest");
    assert_eq!(p.command, "bash");
    assert_eq!(p.persist, None);
}

#[test]
fn parse_full() {
    let p = parse_profile("Tools | ghcr.io/me/tools:1 | zsh | persist").unwrap();
    assert_eq!(p.image, "ghcr.io/me/tools:1");
    assert_eq!(p.command, "zsh");
    assert_eq!(p.persist, Some(true));

    let e = parse_profile("X | y | sh | ephemeral").unwrap();
    assert_eq!(e.persist, Some(false));
}

#[test]
fn parse_errors() {
    assert!(parse_profile("").is_err());
    assert!(parse_profile("OnlyLabel").is_err());
    assert!(parse_profile("L | img | sh | bogus").is_err());
}

#[test]
fn user_overrides_builtin_by_label() {
    let (merged, errors) = profiles(&[
        "debian | debian:bookworm | bash".to_string(),
        "Custom | my/img:1 | sh".to_string(),
    ]);
    assert!(errors.is_empty());
    // Override replaces in place, keeps count = builtin + 1 custom.
    assert_eq!(merged.len(), builtin().len() + 1);
    let debian = merged.iter().find(|p| p.label.eq_ignore_ascii_case("debian")).unwrap();
    assert_eq!(debian.image, "debian:bookworm");
    assert!(merged.iter().any(|p| p.label == "Custom"));
}

#[test]
fn bad_entries_collected_not_fatal() {
    let (merged, errors) = profiles(&["nope".to_string()]);
    assert_eq!(merged.len(), builtin().len());
    assert_eq!(errors.len(), 1);
}
