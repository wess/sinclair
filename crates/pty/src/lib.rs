//! Pseudo-terminal management for the Sinclair terminal emulator.
//!
//! Open a pty, spawn a shell (or any argv) attached to it, and drive it. On
//! Unix a spawned [`Pty`] splits three ways: a [`Control`] the embedder keeps
//! (resize, foreground checks, group signals), a [`Pump`]/[`Waker`] pair for
//! a reader thread's nonblocking IO, and the `Pty` itself, which owns the
//! child handle for reaping. A Windows pseudoconsole backend (ConPTY, via the
//! `windows` crate) covers the same spawn entry point.
//!
//! ```no_run
//! let opts = pty::SpawnOptions::default(); // user's login shell
//! let session = pty::Pty::spawn(&opts).unwrap();
//! let control = session.control().unwrap();
//! control.resize(pty::Winsize::new(120, 40)).unwrap();
//! ```

mod spawn;
mod winsize;

#[cfg(unix)]
mod control;
#[cfg(unix)]
mod pump;
#[cfg(unix)]
mod session;
#[cfg(unix)]
mod unix;

#[cfg(windows)]
mod windows;

pub use spawn::{default_env, default_shell, SpawnOptions};
pub use winsize::Winsize;

#[cfg(unix)]
pub use control::Control;
#[cfg(unix)]
pub use pump::{Pump, Ready, Waker};
#[cfg(unix)]
pub use session::Pty;

#[cfg(windows)]
pub use windows::Pty;
