//! Options describing what to run inside a pty.

use std::path::PathBuf;

use crate::winsize::Winsize;

/// How to spawn the child process attached to the pty slave.
#[derive(Debug, Clone)]
pub struct SpawnOptions {
    /// Program and arguments. `argv[0]` is the executable path.
    pub argv: Vec<String>,
    /// Run as a login shell: argv[0] becomes `-` + the program basename.
    pub login: bool,
    /// Environment overrides applied on top of the inherited environment.
    pub env: Vec<(String, String)>,
    /// Working directory for the child (inherited when `None`).
    pub cwd: Option<PathBuf>,
    /// Initial window size applied to the pty before the child starts.
    pub winsize: Winsize,
}

/// The user's shell from `$SHELL`, falling back to `/bin/zsh`.
pub fn default_shell() -> String {
    std::env::var("SHELL")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "/bin/zsh".to_string())
}

/// Default environment overrides for terminal children.
pub fn default_env() -> Vec<(String, String)> {
    vec![
        ("TERM".to_string(), "xterm-256color".to_string()),
        ("COLORTERM".to_string(), "truecolor".to_string()),
    ]
}

impl Default for SpawnOptions {
    /// The user's shell, run as a login shell, with default env overrides.
    fn default() -> Self {
        Self {
            argv: vec![default_shell()],
            login: true,
            env: default_env(),
            cwd: None,
            winsize: Winsize::default(),
        }
    }
}

impl SpawnOptions {
    /// Run an explicit argv directly (not as a login shell).
    pub fn command(argv: Vec<String>) -> Self {
        Self {
            argv,
            login: false,
            ..Self::default()
        }
    }

    /// The argv[0] the child will see: `-basename` for login shells,
    /// otherwise the program path unchanged.
    pub fn argv0(&self) -> String {
        let program = self.argv.first().map(String::as_str).unwrap_or("");
        if self.login {
            let base = program.rsplit('/').next().unwrap_or(program);
            format!("-{base}")
        } else {
            program.to_string()
        }
    }
}

#[cfg(all(test, unix))]
#[path = "../tests/spawn.rs"]
mod tests;
