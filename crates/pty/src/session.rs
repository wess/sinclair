//! A live pty session: master fd plus the attached child process.

use std::io;
use std::os::fd::OwnedFd;
use std::process::{Child, ExitStatus};

use crate::control::Control;
use crate::pump::{Pump, Waker};
use crate::spawn::SpawnOptions;
use crate::unix;

/// A spawned child attached to a pty. Owns the master fd (closed on drop)
/// and the child handle; dropping it without an explicit [`Pty::wait`]
/// kills and reaps the child so no zombie survives an error path.
pub struct Pty {
    master: OwnedFd,
    child: Child,
}

impl Pty {
    /// Open a pty pair, apply the initial winsize, spawn the child on the
    /// slave, then close the slave in the parent.
    pub fn spawn(opts: &SpawnOptions) -> io::Result<Self> {
        let pair = unix::open_pair()?;
        rustix::termios::tcsetwinsize(&pair.slave, opts.winsize.to_termios())?;
        let child = unix::spawn_child(opts, &pair)?;
        drop(pair.slave);
        Ok(Self {
            master: pair.master,
            child,
        })
    }

    /// A [`Control`] over a dup of the master, for the embedder to keep
    /// while this `Pty` moves to the thread that pumps and reaps it.
    pub fn control(&self) -> io::Result<Control> {
        Ok(Control::new(self.master.try_clone()?, self.child.id()))
    }

    /// A [`Pump`] over a dup of the master for a reader thread, plus the
    /// [`Waker`] that interrupts it. Puts the master into nonblocking mode.
    pub fn pump(&self) -> io::Result<(Pump, Waker)> {
        Pump::new(self.master.try_clone()?)
    }

    /// OS pid of the child.
    pub fn child_pid(&self) -> u32 {
        self.child.id()
    }

    /// Send SIGKILL to the child.
    pub fn kill(&mut self) -> io::Result<()> {
        self.child.kill()
    }

    /// Wait for the child to exit, reaping it. Blocking.
    pub fn wait(&mut self) -> io::Result<ExitStatus> {
        self.child.wait()
    }
}

impl Drop for Pty {
    /// Kill and reap the child. Both are no-ops after [`Pty::wait`]: `Child`
    /// refuses to signal an already-reaped pid and returns the cached status.
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(all(test, unix))]
#[path = "../tests/session.rs"]
mod tests;
