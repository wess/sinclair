//! Parent-side control of a spawned pty: resize, foreground checks, and
//! process-group signals. Holds its own dup of the master and the child's
//! pid, so the [`crate::Pty`] (and with it the child handle) can live on a
//! reaper thread while the embedder keeps control.

use std::io;
use std::os::fd::OwnedFd;

use rustix::process::{kill_process_group, Pid, Signal};

use crate::winsize::Winsize;

/// Control handle for a running pty session.
pub struct Control {
  master: OwnedFd,
  /// Child pid; also its process-group id (the child called `setsid`).
  pid: u32,
}

impl Control {
  pub(crate) fn new(master: OwnedFd, pid: u32) -> Self {
    Self { master, pid }
  }

  /// Resize the terminal (TIOCSWINSZ on the master). The kernel delivers
  /// SIGWINCH to the pty's foreground process group.
  pub fn resize(&self, size: Winsize) -> io::Result<()> {
    rustix::termios::tcsetwinsize(&self.master, size.to_termios()).map_err(io::Error::from)
  }

  /// Whether a foreground job other than the shell itself is running on the
  /// pty. The shell is its own session/group leader (pgid == pid via
  /// `setsid`); when it runs a command it makes that command's group the
  /// terminal foreground group, so a differing `tcgetpgrp` means "busy".
  ///
  /// Once the child has exited, macOS reports a foreground group of 0, which
  /// rustix's `tcgetpgrp` asserts against (aborting the process); the raw
  /// call lets a 0 or an error read as "nothing running".
  pub fn foreground_running(&self) -> bool {
    use std::os::fd::AsRawFd;
    // SAFETY: tcgetpgrp only reads the terminal's foreground group from an
    // fd we own for the lifetime of `self`.
    let pgrp = unsafe { libc::tcgetpgrp(self.master.as_raw_fd()) };
    pgrp > 0 && pgrp as u32 != self.pid
  }

  /// SIGHUP the child's whole process group: the polite "terminal closing".
  pub fn hangup(&self) -> io::Result<()> {
    self.signal_group(Signal::Hup)
  }

  /// SIGKILL the child's whole process group.
  pub fn kill(&self) -> io::Result<()> {
    self.signal_group(Signal::Kill)
  }

  fn signal_group(&self, sig: Signal) -> io::Result<()> {
    let pid = Pid::from_raw(self.pid as i32)
      .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "pid 0"))?;
    kill_process_group(pid, sig).map_err(io::Error::from)
  }
}

#[cfg(all(test, unix))]
#[path = "../tests/control.rs"]
mod tests;
