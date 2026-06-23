//! A live pty session: master fd plus the attached child process.

use std::fs::File;
use std::io;
use std::os::fd::OwnedFd;
use std::process::{Child, ExitStatus};

use crate::spawn::SpawnOptions;
use crate::unix;
use crate::winsize::Winsize;

/// A spawned child attached to a pty. Owns the master fd (closed on drop)
/// and the child handle.
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

    /// Write bytes to the child's input. Blocking.
    pub fn write(&self, buf: &[u8]) -> io::Result<usize> {
        rustix::io::write(&self.master, buf).map_err(io::Error::from)
    }

    /// Read bytes of child output. Blocking; returns `Ok(0)` at EOF and
    /// `EIO` on Linux once the child side is fully closed.
    pub fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        rustix::io::read(&self.master, buf).map_err(io::Error::from)
    }

    /// A `File` over a duplicate of the master fd, for a reader thread.
    pub fn try_clone_reader(&self) -> io::Result<File> {
        Ok(File::from(self.master.try_clone()?))
    }

    /// A `File` over a duplicate of the master fd, for a writer.
    pub fn try_clone_writer(&self) -> io::Result<File> {
        Ok(File::from(self.master.try_clone()?))
    }

    /// Resize the terminal (TIOCSWINSZ on the master). The kernel delivers
    /// SIGWINCH to the child's process group.
    pub fn resize(&self, size: Winsize) -> io::Result<()> {
        rustix::termios::tcsetwinsize(&self.master, size.to_termios()).map_err(io::Error::from)
    }

    /// OS pid of the child.
    pub fn child_pid(&self) -> u32 {
        self.child.id()
    }

    /// Send SIGKILL to the child.
    pub fn kill(&mut self) -> io::Result<()> {
        self.child.kill()
    }

    /// Wait for the child to exit, reaping it.
    pub fn wait(&mut self) -> io::Result<ExitStatus> {
        self.child.wait()
    }

    /// Non-blocking check for child exit.
    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        self.child.try_wait()
    }
}

// The master OwnedFd closes itself when Pty drops; no explicit Drop needed.

#[cfg(all(test, unix))]
#[path = "../tests/session.rs"]
mod tests;
