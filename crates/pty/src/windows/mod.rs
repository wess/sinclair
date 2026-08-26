//! Windows pseudoconsole (ConPTY) backend.
//!
//! Mirrors the Unix [`crate::session::Pty`] API: open a pseudoconsole, spawn a
//! child attached to it, and drive it — read output, write input, resize,
//! kill/wait.

mod conpty;
mod spawn;

use std::fs::File;
use std::io::{self, Read, Write};
use std::process::ExitStatus;

use crate::spawn::SpawnOptions;
use crate::winsize::Winsize;

use conpty::Pcon;
use spawn::Child;

/// A child attached to a pseudoconsole. Owns the console guard, the two pipe
/// ends (as `File`s), and the child handle.
pub struct Pty {
  // Field order is drop order: close the pipes, then the pseudoconsole, then
  // the child handle.
  input: File,
  output: File,
  pcon: Pcon,
  child: Child,
}

impl Pty {
  /// Open a pseudoconsole sized from `opts`, then spawn the child on it.
  pub fn spawn(opts: &SpawnOptions) -> io::Result<Self> {
    let (pcon, input, output) = conpty::create(opts.winsize)?;
    let child = spawn::spawn_child(opts, pcon.handle())?;
    Ok(Self {
      input: File::from(input),
      output: File::from(output),
      pcon,
      child,
    })
  }

  /// Write bytes to the child's input. Blocking.
  pub fn write(&self, buf: &[u8]) -> io::Result<usize> {
    (&self.input).write(buf)
  }

  /// Read bytes of child output. Blocking; returns `Ok(0)` at EOF.
  pub fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
    (&self.output).read(buf)
  }

  /// A `File` over a duplicate of the output pipe, for a reader thread.
  pub fn try_clone_reader(&self) -> io::Result<File> {
    self.output.try_clone()
  }

  /// A `File` over a duplicate of the input pipe, for a writer.
  pub fn try_clone_writer(&self) -> io::Result<File> {
    self.input.try_clone()
  }

  /// Resize the pseudoconsole. The child sees a console-resize event.
  pub fn resize(&self, size: Winsize) -> io::Result<()> {
    self.pcon.resize(size)
  }

  /// OS pid of the child.
  pub fn child_pid(&self) -> u32 {
    self.child.pid()
  }

  /// Whether a foreground job other than the shell is running. ConPTY exposes
  /// no controlling-terminal foreground-group query, so this is always
  /// `false` on Windows (callers treat it as "not busy").
  pub fn foreground_running(&self) -> bool {
    false
  }

  /// Force-terminate the child.
  pub fn kill(&mut self) -> io::Result<()> {
    self.child.kill()
  }

  /// Wait for the child to exit.
  pub fn wait(&mut self) -> io::Result<ExitStatus> {
    self.child.wait()
  }

  /// Non-blocking check for child exit.
  pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
    self.child.try_wait()
  }
}
