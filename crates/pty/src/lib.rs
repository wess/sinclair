//! Pseudo-terminal management for the Sinclair terminal emulator.
//!
//! Open a pty, spawn a shell (or any argv) attached to it, and drive it: read
//! child output, write input, resize, kill/wait. Two backends implement one
//! [`Pty`] API — a Unix pty pair (`rustix`) and a Windows pseudoconsole
//! (ConPTY, via the `windows` crate).
//!
//! ```no_run
//! let opts = pty::SpawnOptions::default(); // user's login shell
//! let session = pty::Pty::spawn(&opts).unwrap();
//! session.resize(pty::Winsize::new(120, 40)).unwrap();
//! ```

mod spawn;
mod winsize;

#[cfg(unix)]
mod session;
#[cfg(unix)]
mod unix;

#[cfg(windows)]
mod windows;

pub use spawn::{default_env, default_shell, SpawnOptions};
pub use winsize::Winsize;

#[cfg(unix)]
pub use session::Pty;
#[cfg(unix)]
pub use unix::{open_pair, spawn_child, PtyPair};

#[cfg(windows)]
pub use windows::Pty;
