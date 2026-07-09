use super::*;
use std::path::Path;

#[test]
fn detects_known_shells_with_or_without_login_dash() {
    assert_eq!(detect("/bin/zsh"), Some(Shell::Zsh));
    assert_eq!(detect("-zsh"), Some(Shell::Zsh));
    assert_eq!(detect("/usr/bin/bash"), Some(Shell::Bash));
    assert_eq!(detect("/opt/homebrew/bin/fish"), Some(Shell::Fish));
    assert_eq!(detect("/bin/sh"), None);
    assert_eq!(detect("/usr/local/bin/nu"), None);
}

#[test]
fn zsh_sets_zdotdir_and_chains_original() {
    let dir = Path::new("/cfg/shell-integration");
    let env = |k: &str| (k == "ZDOTDIR").then(|| "/home/me/.zsh".to_string());
    let got = env_overrides("/bin/zsh", dir, env);
    assert!(got.contains(&("ZDOTDIR".to_string(), "/cfg/shell-integration".to_string())));
    assert!(got.contains(&("SINCLAIR_ZDOTDIR".to_string(), "/home/me/.zsh".to_string())));
}

#[test]
fn zsh_without_original_zdotdir_omits_chain_var() {
    let dir = Path::new("/cfg/shell-integration");
    let got = env_overrides("/bin/zsh", dir, |_| None);
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].0, "ZDOTDIR");
}

#[test]
fn fish_prepends_data_dir() {
    let dir = Path::new("/cfg/shell-integration");
    let env = |k: &str| (k == "XDG_DATA_DIRS").then(|| "/usr/share".to_string());
    let got = env_overrides("/usr/bin/fish", dir, env);
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].0, "XDG_DATA_DIRS");
    assert_eq!(got[0].1, "/cfg/shell-integration/fish-data:/usr/share");
}

#[test]
fn bash_sources_snippet_from_prompt_command() {
    let dir = Path::new("/cfg/shell-integration");
    let got = env_overrides("/bin/bash", dir, |_| None);
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].0, "PROMPT_COMMAND");
    assert!(got[0].1.contains("integration.bash"));
}

#[test]
fn unknown_shell_injects_nothing() {
    let dir = Path::new("/cfg/shell-integration");
    assert!(env_overrides("/bin/sh", dir, |_| None).is_empty());
}
