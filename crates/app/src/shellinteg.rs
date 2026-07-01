//! Optional shell integration. Injects OSC 133 prompt marks and OSC 7 cwd
//! reporting into the spawned shell so jump-to-prompt and cwd inheritance work
//! out of the box, with no edits to the user's shell rc.
//!
//! The hook scripts are generated under `<config>/shell-integration/` and wired
//! in purely through the environment (no argv changes):
//! - **zsh**: `ZDOTDIR` points at our dir; our startup files re-source the
//!   user's real ones, then install `precmd`/`preexec` hooks.
//! - **fish**: `XDG_DATA_DIRS` gains our `vendor_conf.d`, which fish auto-loads.
//! - **bash**: `PROMPT_COMMAND` sources our snippet each prompt (best-effort,
//!   a `PROMPT_COMMAND` set later in `.bashrc` can still win).

use std::path::{Path, PathBuf};

const ZSHENV: &str = "\
# Prompt shell integration. Re-source the user's zshenv (env/PATH), keeping
# ZDOTDIR pointed at Prompt's dir so Prompt's .zshrc loads next.
PROMPT_INTEG_DIR=\"$ZDOTDIR\"
[[ -f \"${PROMPT_ZDOTDIR:-$HOME}/.zshenv\" ]] && source \"${PROMPT_ZDOTDIR:-$HOME}/.zshenv\"
ZDOTDIR=\"$PROMPT_INTEG_DIR\"
";

const ZPROFILE: &str = "\
PROMPT_INTEG_DIR=\"$ZDOTDIR\"
[[ -f \"${PROMPT_ZDOTDIR:-$HOME}/.zprofile\" ]] && source \"${PROMPT_ZDOTDIR:-$HOME}/.zprofile\"
ZDOTDIR=\"$PROMPT_INTEG_DIR\"
";

const ZSHRC: &str = "\
# Restore the user's ZDOTDIR for the rest of the session, source their zshrc,
# then install OSC 133 prompt marks + OSC 7 cwd reporting.
_prompt_zdotdir=\"$ZDOTDIR\"
ZDOTDIR=\"${PROMPT_ZDOTDIR:-$HOME}\"
# macOS's global /etc/zshrc runs before this file and sets
# HISTFILE=${ZDOTDIR:-$HOME}/.zsh_history while ZDOTDIR still points at our
# integration dir, so history would read/write there instead of the user's real
# file. If HISTFILE landed inside our dir, repoint it at the user's real dir
# before their rc runs (which may still override it). zsh reads the history file
# after rc processing, so this loads the correct history.
[[ \"$HISTFILE\" == \"$_prompt_zdotdir\"/* ]] && HISTFILE=\"$ZDOTDIR/.zsh_history\"
unset _prompt_zdotdir
[[ -f \"$ZDOTDIR/.zshrc\" ]] && source \"$ZDOTDIR/.zshrc\"
_prompt_precmd() {
  local ret=$?
  printf '\\e]133;D;%d\\e\\\\' \"$ret\"
  printf '\\e]133;A\\e\\\\'
  printf '\\e]7;file://%s%s\\e\\\\' \"${HOST}\" \"${PWD}\"
}
_prompt_preexec() { printf '\\e]133;C\\e\\\\'; }
autoload -Uz add-zsh-hook 2>/dev/null
if (( $+functions[add-zsh-hook] )); then
  add-zsh-hook precmd _prompt_precmd
  add-zsh-hook preexec _prompt_preexec
fi
";

const ZLOGIN: &str = "\
[[ -f \"${PROMPT_ZDOTDIR:-$HOME}/.zlogin\" ]] && source \"${PROMPT_ZDOTDIR:-$HOME}/.zlogin\"
";

const FISH: &str = "\
# Prompt shell integration (fish): OSC 133 prompt marks + OSC 7 cwd.
function _prompt_mark_prompt --on-event fish_prompt
    printf '\\e]133;A\\e\\\\'
    printf '\\e]7;file://%s%s\\e\\\\' (hostname) \"$PWD\"
end
function _prompt_mark_preexec --on-event fish_preexec
    printf '\\e]133;C\\e\\\\'
end
function _prompt_mark_postexec --on-event fish_postexec
    printf '\\e]133;D;%d\\e\\\\' $status
end
";

const BASH: &str = "\
# Prompt shell integration (bash): emit prompt marks + cwd each prompt.
_prompt_ret=$?
printf '\\e]133;D;%d\\e\\\\' \"$_prompt_ret\"
printf '\\e]133;A\\e\\\\'
printf '\\e]7;file://%s%s\\e\\\\' \"${HOSTNAME}\" \"${PWD}\"
";

/// Shells we know how to wire up.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Shell {
    Zsh,
    Bash,
    Fish,
}

/// Classify a shell program path by basename, tolerating a login `-` prefix.
fn detect(program: &str) -> Option<Shell> {
    let base = Path::new(program).file_name()?.to_str()?;
    let base = base.strip_prefix('-').unwrap_or(base);
    match base {
        "zsh" => Some(Shell::Zsh),
        "bash" => Some(Shell::Bash),
        "fish" => Some(Shell::Fish),
        _ => None,
    }
}

/// Directory holding the generated scripts, beside the config file.
fn dir() -> Option<PathBuf> {
    config::default_path().and_then(|p| p.parent().map(|d| d.join("shell-integration")))
}

/// Write the script set to disk (idempotent). Returns the dir on success.
pub fn install() -> Option<PathBuf> {
    let dir = dir()?;
    let fishconf = dir.join("fish-data/fish/vendor_conf.d");
    std::fs::create_dir_all(&fishconf).ok()?;
    write(&dir.join(".zshenv"), ZSHENV);
    write(&dir.join(".zprofile"), ZPROFILE);
    write(&dir.join(".zshrc"), ZSHRC);
    write(&dir.join(".zlogin"), ZLOGIN);
    write(&fishconf.join("prompt.fish"), FISH);
    write(&dir.join("integration.bash"), BASH);
    Some(dir)
}

fn write(path: &Path, contents: &str) {
    let _ = std::fs::write(path, contents);
}

/// Environment overrides that wire integration into `program`, given the
/// script `dir` and a lookup into the current environment. Empty for shells we
/// don't recognize, so the spawn is untouched.
fn env_overrides(
    program: &str,
    dir: &Path,
    env: impl Fn(&str) -> Option<String>,
) -> Vec<(String, String)> {
    let Some(shell) = detect(program) else {
        return Vec::new();
    };
    let d = dir.to_string_lossy().into_owned();
    match shell {
        Shell::Zsh => {
            let mut v = vec![("ZDOTDIR".to_string(), d)];
            if let Some(orig) = env("ZDOTDIR").filter(|s| !s.is_empty()) {
                v.push(("PROMPT_ZDOTDIR".to_string(), orig));
            }
            v
        }
        Shell::Fish => {
            let mut val = dir.join("fish-data").to_string_lossy().into_owned();
            if let Some(existing) = env("XDG_DATA_DIRS").filter(|s| !s.is_empty()) {
                val.push(':');
                val.push_str(&existing);
            }
            vec![("XDG_DATA_DIRS".to_string(), val)]
        }
        Shell::Bash => vec![(
            "PROMPT_COMMAND".to_string(),
            format!("source '{d}/integration.bash'"),
        )],
    }
}

/// Ensure scripts exist and return the env overrides for `program`. A no-op
/// (empty) when the shell is unsupported or the dir can't be created.
pub fn overrides_for(program: &str) -> Vec<(String, String)> {
    let Some(dir) = install() else {
        return Vec::new();
    };
    env_overrides(program, &dir, |k| std::env::var(k).ok())
}

#[cfg(test)]
#[path = "../tests/shellinteg.rs"]
mod tests;
