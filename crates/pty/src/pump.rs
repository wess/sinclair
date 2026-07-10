//! Poll-driven master-side IO for a reader thread: block until the pty has
//! output, can take input, or another thread asks for attention via a
//! [`Waker`]. All reads and writes are nonblocking, so the thread that owns
//! the [`Pump`] can never wedge on a stalled child.

use std::io;
use std::os::fd::OwnedFd;

use rustix::event::{poll, PollFd, PollFlags};
use rustix::fs::OFlags;
use rustix::io::FdFlags;

/// What [`Pump::wait`] observed.
#[derive(Debug, Clone, Copy, Default)]
pub struct Ready {
    /// Child output (or EOF/EIO) is available to [`Pump::read`].
    pub readable: bool,
    /// The pty can take at least one byte via [`Pump::write`].
    pub writable: bool,
    /// A [`Waker`] fired since the last wait.
    pub woken: bool,
}

/// The reader thread's end: a dup of the pty master plus the wake pipe.
pub struct Pump {
    master: OwnedFd,
    wake: OwnedFd,
}

/// Interrupts a thread blocked in [`Pump::wait`]. Any thread may call it.
pub struct Waker {
    pipe: OwnedFd,
}

impl Pump {
    /// Wrap a dup of the master. Sets `O_NONBLOCK`, which lives on the shared
    /// open file description: every dup of this master follows suit, so all
    /// IO on the pty must go through the pump from here on.
    pub(crate) fn new(master: OwnedFd) -> io::Result<(Pump, Waker)> {
        set_nonblocking(&master)?;
        let (rx, tx) = rustix::pipe::pipe()?;
        for fd in [&rx, &tx] {
            rustix::io::fcntl_setfd(fd, FdFlags::CLOEXEC)?;
            set_nonblocking(fd)?;
        }
        Ok((Pump { master, wake: rx }, Waker { pipe: tx }))
    }

    /// Block until the master is readable, writable (only checked when
    /// `want_write`), or a waker fired. Hangup/error on the master counts as
    /// readable so the caller reads through to EOF/EIO.
    pub fn wait(&self, want_write: bool) -> io::Result<Ready> {
        let mut master_flags = PollFlags::IN;
        if want_write {
            master_flags |= PollFlags::OUT;
        }
        loop {
            let mut fds = [
                PollFd::new(&self.master, master_flags),
                PollFd::new(&self.wake, PollFlags::IN),
            ];
            match poll(&mut fds, -1) {
                Ok(_) => {
                    let gone = PollFlags::HUP | PollFlags::ERR;
                    let master = fds[0].revents();
                    let woken = fds[1].revents().intersects(PollFlags::IN | gone);
                    if woken {
                        self.drain_wake();
                    }
                    return Ok(Ready {
                        readable: master.intersects(PollFlags::IN | gone),
                        writable: master.intersects(PollFlags::OUT),
                        woken,
                    });
                }
                Err(rustix::io::Errno::INTR) => continue,
                Err(e) => return Err(e.into()),
            }
        }
    }

    /// Nonblocking read of child output. `Ok(0)` at EOF; `WouldBlock` and
    /// `EIO` (Linux, child side fully closed) pass through for the caller.
    pub fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        rustix::io::read(&self.master, buf).map_err(io::Error::from)
    }

    /// Nonblocking write of input; returns how many bytes the pty took.
    pub fn write(&self, buf: &[u8]) -> io::Result<usize> {
        rustix::io::write(&self.master, buf).map_err(io::Error::from)
    }

    fn drain_wake(&self) {
        let mut buf = [0u8; 64];
        while matches!(rustix::io::read(&self.wake, &mut buf), Ok(n) if n > 0) {}
    }
}

impl Waker {
    /// Wake the pump. A full pipe already guarantees a pending wake, so the
    /// write result is irrelevant.
    pub fn wake(&self) {
        let _ = rustix::io::write(&self.pipe, &[1]);
    }
}

fn set_nonblocking(fd: &OwnedFd) -> io::Result<()> {
    let flags = rustix::fs::fcntl_getfl(fd)?;
    rustix::fs::fcntl_setfl(fd, flags | OFlags::NONBLOCK)?;
    Ok(())
}

#[cfg(all(test, unix))]
#[path = "../tests/pump.rs"]
mod tests;
