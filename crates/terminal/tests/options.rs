use super::*;

#[test]
fn default_is_80x24_login_shell() {
    let opts = SessionOptions::default();
    assert_eq!(opts.cols, 80);
    assert_eq!(opts.rows, 24);
    assert_eq!(opts.scrollback_limit, vt::DEFAULT_SCROLLBACK);
    assert!(opts.spawn.login);
}

#[test]
fn command_sets_argv_without_login() {
    let opts = SessionOptions::command(vec!["/bin/echo".to_string(), "hi".to_string()]);
    assert_eq!(opts.spawn.argv[0], "/bin/echo");
    assert!(!opts.spawn.login);
    assert_eq!(opts.cols, 80);
}
