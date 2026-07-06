use super::split_spec;

#[test]
fn spec_without_branch() {
    assert_eq!(split_spec("../feature"), ("../feature", None));
    assert_eq!(split_spec("/abs/path"), ("/abs/path", None));
}

#[test]
fn spec_with_branch() {
    assert_eq!(split_spec("../feature@my-branch"), ("../feature", Some("my-branch")));
    assert_eq!(split_spec("wt@wip"), ("wt", Some("wip")));
}

#[test]
fn trailing_at_is_not_a_branch() {
    assert_eq!(split_spec("path@"), ("path@", None));
    assert_eq!(split_spec("@branch"), ("@branch", None));
}
