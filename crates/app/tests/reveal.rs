use super::*;

/// A directory that exists on every host the tests run on, with a file in it.
fn fixture() -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir().join("sinclair-reveal-test");
    let file = dir.join("hit.txt");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(&file, b"x").unwrap();
    // Both sides go through canonicalize so a symlinked temp dir (macOS puts
    // `/var` behind `/private/var`) compares equal.
    (
        std::fs::canonicalize(&dir).unwrap(),
        std::fs::canonicalize(&file).unwrap(),
    )
}

#[test]
fn resolves_an_absolute_path() {
    let (_, file) = fixture();
    assert_eq!(
        resolve(&file.display().to_string(), None).as_ref(),
        Some(&file)
    );
}

#[test]
fn resolves_a_relative_path_against_the_pane_cwd() {
    let (dir, file) = fixture();
    assert_eq!(resolve("hit.txt", Some(&dir)).as_ref(), Some(&file));
    assert_eq!(resolve("./hit.txt", Some(&dir)).as_ref(), Some(&file));
}

#[test]
fn a_relative_path_with_no_cwd_resolves_to_nothing() {
    // Without a working directory there is no question to answer; guessing
    // against the process cwd would reveal a file from somewhere else.
    assert!(resolve("hit.txt", None).is_none());
}

#[test]
fn text_that_only_looked_like_a_path_is_refused() {
    let (dir, _) = fixture();
    assert!(resolve("no/such/file", Some(&dir)).is_none());
    assert!(resolve("/definitely/not/here", None).is_none());
}

#[test]
fn expands_a_home_relative_path() {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return;
    };
    let want = std::fs::canonicalize(&home).unwrap();
    assert_eq!(resolve("~", None), Some(want.clone()));
    // `~/` with nothing after it is still the home directory.
    assert_eq!(resolve("~/", None), Some(want));
}

#[test]
fn climbs_out_of_the_cwd() {
    let (dir, file) = fixture();
    let below = dir.join("sub");
    std::fs::create_dir_all(&below).unwrap();
    assert_eq!(resolve("../hit.txt", Some(&below)).as_ref(), Some(&file));
}
