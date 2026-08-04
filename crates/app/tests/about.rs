use super::*;

/// A build made from the version's tag is the only one that may claim a
/// release date — that is what the whole distinction is for.
#[test]
fn a_tagged_build_reports_its_release_date() {
    assert_eq!(dateline(true, "2026-07-27"), "Released 2026-07-27");
}

/// The bug this replaced: any commit after the release carries the same version
/// number, and the panel presented that commit's date as the release date. A
/// version released weeks ago read as released today.
#[test]
fn a_later_commit_does_not_claim_to_be_the_release() {
    let line = dateline(false, "2026-08-04");
    assert!(!line.contains("Released"), "{line}");
    assert!(line.contains("2026-08-04"), "{line}");
    assert!(line.starts_with("Development build"), "{line}");
}

/// Built outside a git checkout (a source tarball): there is no date worth
/// qualifying, so the line is just what the build is rather than
/// "Development build · unknown".
#[test]
fn a_build_with_no_date_says_only_what_it_is() {
    assert_eq!(dateline(false, "unknown"), "Development build");
}
