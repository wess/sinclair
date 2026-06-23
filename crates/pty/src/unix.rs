//! Low-level pty pair creation and child process spawning.

use std::io;
use std::os::fd::{AsRawFd, BorrowedFd, OwnedFd};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command};

use rustix::fs::{Mode, OFlags};
use rustix::io::FdFlags;
use rustix::pty::{grantpt, openpt, ptsname, unlockpt, OpenptFlags};

use crate::spawn::SpawnOptions;

/// An open pseudo-terminal pair. The master stays in the parent; the slave
/// becomes the child's controlling terminal and stdio.
pub struct PtyPair {
    pub master: OwnedFd,
    pub slave: OwnedFd,
}

/// Open a master/slave pty pair. The master is close-on-exec; the slave is
/// not (it must survive into `pre_exec`, which closes it after dup2).
pub fn open_pair() -> io::Result<PtyPair> {
    let master = openpt(OpenptFlags::RDWR | OpenptFlags::NOCTTY)?;
    grantpt(&master)?;
    unlockpt(&master)?;
    rustix::io::fcntl_setfd(&master, FdFlags::CLOEXEC)?;
    let path = ptsname(&master, Vec::new())?;
    let slave = rustix::fs::open(&path, OFlags::RDWR | OFlags::NOCTTY, Mode::empty())?;
    Ok(PtyPair { master, slave })
}

/// Spawn the child described by `opts` attached to the pair's slave.
///
/// In the child (between fork and exec): start a new session, make the slave
/// the controlling tty, and dup it onto stdin/stdout/stderr.
pub fn spawn_child(opts: &SpawnOptions, pair: &PtyPair) -> io::Result<Child> {
    let program = opts
        .argv
        .first()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "empty argv"))?;

    let mut cmd = Command::new(program);
    cmd.arg0(opts.argv0());
    cmd.args(&opts.argv[1..]);
    for (key, value) in &opts.env {
        cmd.env(key, value);
    }
    if let Some(dir) = &opts.cwd {
        cmd.current_dir(dir);
    }

    let slave_fd = pair.slave.as_raw_fd();
    unsafe {
        cmd.pre_exec(move || {
            let slave = BorrowedFd::borrow_raw(slave_fd);
            rustix::process::setsid()?;
            rustix::process::ioctl_tiocsctty(slave)?;
            rustix::stdio::dup2_stdin(slave)?;
            rustix::stdio::dup2_stdout(slave)?;
            rustix::stdio::dup2_stderr(slave)?;
            if slave_fd > 2 {
                rustix::io::close(slave_fd);
            }
            Ok(())
        });
    }

    cmd.spawn()
}

#[cfg(all(test, unix))]
#[path = "../tests/unix.rs"]
mod tests;
