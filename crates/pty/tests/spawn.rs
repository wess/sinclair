use super::*;

#[test]
fn default_shell_falls_back_to_zsh() {
    let shell = default_shell();
    assert!(!shell.is_empty());
    if std::env::var("SHELL").map(|s| s.is_empty()).unwrap_or(true) {
        assert_eq!(shell, "/bin/zsh");
    }
}

#[test]
fn default_env_sets_term_and_colorterm() {
    let env = default_env();
    assert!(env.contains(&("TERM".to_string(), "xterm-256color".to_string())));
    assert!(env.contains(&("COLORTERM".to_string(), "truecolor".to_string())));
}

#[test]
fn default_options_are_login_shell() {
    let opts = SpawnOptions::default();
    assert!(opts.login);
    assert_eq!(opts.argv.len(), 1);
    assert!(opts.argv0().starts_with('-'));
}

#[test]
fn login_argv0_is_dash_basename() {
    let opts = SpawnOptions {
        argv: vec!["/bin/zsh".to_string()],
        login: true,
        ..SpawnOptions::default()
    };
    assert_eq!(opts.argv0(), "-zsh");
}

#[test]
fn command_argv0_is_unchanged() {
    let opts = SpawnOptions::command(vec!["/bin/echo".to_string(), "hi".to_string()]);
    assert!(!opts.login);
    assert_eq!(opts.argv0(), "/bin/echo");
}

#[test]
fn empty_argv0_is_safe() {
    let opts = SpawnOptions {
        argv: vec![],
        ..SpawnOptions::default()
    };
    assert_eq!(opts.argv0(), "-");
}
