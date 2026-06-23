//! Options describing a terminal session.

/// How to size the terminal and what to run inside it.
///
/// `cols`/`rows` are authoritative: [`crate::Session::spawn`] copies them
/// into `spawn.winsize` so the emulation grid and the kernel pty always
/// start out the same size.
#[derive(Debug, Clone)]
pub struct SessionOptions {
    /// Grid width in cells.
    pub cols: usize,
    /// Grid height in cells.
    pub rows: usize,
    /// Maximum primary-screen history rows kept for scrollback.
    pub scrollback_limit: usize,
    /// What to run on the pty slave: argv, login flag, env, cwd.
    pub spawn: pty::SpawnOptions,
}

impl Default for SessionOptions {
    /// An 80x24 login shell with the default scrollback limit.
    fn default() -> Self {
        Self {
            cols: 80,
            rows: 24,
            scrollback_limit: vt::DEFAULT_SCROLLBACK,
            spawn: pty::SpawnOptions::default(),
        }
    }
}

impl SessionOptions {
    /// Session running an explicit argv directly (not a login shell).
    pub fn command(argv: Vec<String>) -> Self {
        Self {
            spawn: pty::SpawnOptions::command(argv),
            ..Self::default()
        }
    }
}

#[cfg(test)]
#[path = "../tests/options.rs"]
mod tests;
