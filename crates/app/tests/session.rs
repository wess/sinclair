use super::*;

#[test]
fn options_default_is_login_shell() {
    let opts = config::Options::default();
    let session = options(&opts, 100, 30, None);
    assert_eq!((session.cols, session.rows), (100, 30));
    assert_eq!(session.scrollback_limit, 10_000);
    assert!(session.spawn.login);
    // With no inherit and no configured directory, defaults to home.
    assert_eq!(session.spawn.cwd, home());
}

#[test]
fn options_honors_command_and_cwd() {
    let mut opts = config::Options::default();
    opts.shell = Some("/bin/bash -i".to_string());
    opts.working_directory = Some("/tmp".to_string());
    opts.scrollback_limit = 42;
    let session = options(&opts, 80, 24, None);
    assert_eq!(session.spawn.argv, vec!["/bin/bash", "-i"]);
    assert!(!session.spawn.login);
    assert_eq!(session.spawn.cwd, Some(PathBuf::from("/tmp")));
    assert_eq!(session.scrollback_limit, 42);
}

#[test]
fn options_empty_command_falls_back_to_shell() {
    let mut opts = config::Options::default();
    opts.shell = Some("   ".to_string());
    let session = options(&opts, 80, 24, None);
    assert!(session.spawn.login);
    assert!(!session.spawn.argv.is_empty());
}

#[test]
fn options_inherited_cwd_beats_config() {
    let mut opts = config::Options::default();
    opts.working_directory = Some("/tmp".to_string());
    let session = options(&opts, 80, 24, Some(PathBuf::from("/work")));
    assert_eq!(session.spawn.cwd, Some(PathBuf::from("/work")));
}

#[test]
fn shellname_takes_basename_of_first_word() {
    assert_eq!(shellname(Some("/bin/bash -i")), "bash");
    assert_eq!(shellname(Some("zsh")), "zsh");
    assert_eq!(shellname(Some("/usr/local/bin/fish --login")), "fish");
}

#[test]
fn shellname_defaults_to_user_shell() {
    assert!(!shellname(None).is_empty());
    assert!(!shellname(Some("   ")).is_empty());
}

#[test]
fn cwdpath_parses_file_urls() {
    assert_eq!(
        cwdpath("file://host/Users/me"),
        Some(PathBuf::from("/Users/me"))
    );
    assert_eq!(cwdpath("file:///tmp"), Some(PathBuf::from("/tmp")));
    assert_eq!(
        cwdpath("file://host/a%20dir/b"),
        Some(PathBuf::from("/a dir/b"))
    );
}

#[test]
fn cwdpath_accepts_plain_paths_rejects_junk() {
    assert_eq!(cwdpath("/var/log"), Some(PathBuf::from("/var/log")));
    assert_eq!(cwdpath("relative/path"), None);
    assert_eq!(cwdpath(""), None);
    assert_eq!(cwdpath("file://hostonly"), None);
}

#[test]
fn percentdecode_handles_malformed_escapes() {
    assert_eq!(percentdecode("/a%2fb"), "/a/b");
    assert_eq!(percentdecode("/x%zz"), "/x%zz");
    assert_eq!(percentdecode("/trail%2"), "/trail%2");
    assert_eq!(percentdecode("/plain"), "/plain");
    // Multi-byte characters after % must not split a codepoint.
    assert_eq!(percentdecode("/x%éy"), "/x%éy");
}
