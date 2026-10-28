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

/// A directory whose name tries to end OSC 7 early and write the clipboard
/// with OSC 52, plus a space, a percent sign and multi-byte UTF-8.
const HOSTILE: &str = "a b\x1b\\\x1b]52;c;aGk=\x07%é✓";

/// A scratch dir holding `HOSTILE`, plus an empty dir standing in for the
/// user's real zsh config. Removed on drop.
struct Scratch(std::path::PathBuf);

impl Scratch {
  fn new(tag: &str) -> Self {
    let root = std::env::temp_dir().join(format!("sinclair-osc7-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join(HOSTILE)).unwrap();
    std::fs::create_dir_all(root.join("home")).unwrap();
    Scratch(root.canonicalize().unwrap())
  }
}

impl Drop for Scratch {
  fn drop(&mut self) {
    let _ = std::fs::remove_dir_all(&self.0);
  }
}

/// Run `script` under `shell` with `args` as `$1…`, returning stdout, or
/// `None` when the shell is not installed on this machine. Paths travel as
/// arguments, never quoted into the script, so the hostile name stays exact.
fn run(shell: &str, flags: &[&str], script: &str, args: &[&Path]) -> Option<Vec<u8>> {
  let out = std::process::Command::new(shell)
    .args(flags)
    .arg("-c")
    .arg(script)
    .arg(shell)
    .args(args)
    .output()
    .ok()?;
  assert!(
    out.status.success(),
    "{shell}: {}",
    String::from_utf8_lossy(&out.stderr)
  );
  Some(out.stdout)
}

/// Install the zsh hooks with an empty stand-in for the user's config, cd to
/// `dir`, and run one precmd.
fn zsh_precmd(s: &Scratch, dir: &Path) -> Option<Vec<u8>> {
  let rc = s.0.join("zshrc");
  std::fs::write(&rc, ZSHRC).unwrap();
  let script = r#"SINCLAIR_ZDOTDIR=$1; source "$2"; cd -- "$3" && _sinclair_precmd"#;
  run("zsh", &["-f"], script, &[&s.0.join("home"), &rc, dir])
}

/// The single OSC 7 payload in `out`, checked to be plain ASCII.
fn osc7(out: &[u8]) -> String {
  let text = String::from_utf8(out.to_vec()).unwrap();
  let start = text.find("\x1b]7;").expect("an OSC 7 report") + 4;
  let len = text[start..].find("\x1b\\").expect("an ST terminator");
  let payload = &text[start..start + len];
  assert!(
    payload.bytes().all(|b| b.is_ascii_graphic()),
    "OSC 7 payload must be printable ASCII: {payload:?}"
  );
  // Nothing after the report may be another escape the name smuggled in.
  assert!(
    !text.contains("\x1b]52"),
    "a directory name injected OSC 52"
  );
  payload.to_string()
}

fn assert_roundtrips(payload: &str, want: &Path) {
  assert!(payload.starts_with("file://"), "{payload}");
  assert_eq!(crate::session::cwdpath(payload).as_deref(), Some(want));
}

#[test]
fn zsh_percent_encodes_a_hostile_cwd() {
  let s = Scratch::new("zsh");
  let dir = s.0.join(HOSTILE);
  let Some(out) = zsh_precmd(&s, &dir) else {
    return;
  };
  assert_roundtrips(&osc7(&out), &dir);
}

#[test]
fn bash_percent_encodes_a_hostile_cwd() {
  let s = Scratch::new("bash");
  let snippet = s.0.join("integration.bash");
  std::fs::write(&snippet, BASH).unwrap();
  let dir = s.0.join(HOSTILE);
  let script = r#"cd -- "$1" && source "$2""#;
  let Some(out) = run(
    "bash",
    &["--norc", "--noprofile"],
    script,
    &[&dir, &snippet],
  ) else {
    return;
  };
  assert_roundtrips(&osc7(&out), &dir);
}

#[test]
fn zsh_plain_cwd_is_reported_verbatim() {
  let s = Scratch::new("zshplain");
  let dir = s.0.join("home");
  let Some(out) = zsh_precmd(&s, &dir) else {
    return;
  };
  let payload = osc7(&out);
  assert!(payload.ends_with(dir.to_str().unwrap()), "{payload}");
}

#[test]
fn fish_percent_encodes_a_hostile_cwd() {
  let s = Scratch::new("fish");
  let conf = s.0.join("sinclair.fish");
  std::fs::write(&conf, FISH).unwrap();
  let dir = s.0.join(HOSTILE);
  // fish takes `-c` arguments as $argv directly (no $0 slot).
  let out = std::process::Command::new("fish")
    .args([
      "--no-config",
      "-c",
      "source $argv[2]; cd $argv[1]; and emit fish_prompt",
    ])
    .arg(&dir)
    .arg(&conf)
    .output();
  let Ok(out) = out else { return };
  assert!(
    out.status.success(),
    "{}",
    String::from_utf8_lossy(&out.stderr)
  );
  assert_roundtrips(&osc7(&out.stdout), &dir);
}
