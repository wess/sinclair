//! The pseudoconsole guard and pipe wiring.
//!
//! [`create`] makes two anonymous pipes and an `HPCON`. ConPTY keeps the
//! child's stdin-read and stdout-write ends; the caller receives the opposite
//! ends — `input` to write to the child, `output` to read its bytes — plus a
//! [`Pcon`] guard that closes the pseudoconsole on drop.

use std::io;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};

use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::Console::{
    ClosePseudoConsole, CreatePseudoConsole, ResizePseudoConsole, COORD, HPCON,
};
use windows::Win32::System::Pipes::CreatePipe;

use crate::winsize::Winsize;

/// Owns an `HPCON`, closing it on drop.
pub struct Pcon(HPCON);

impl Pcon {
    /// The raw handle, for the process-creation attribute list.
    pub fn handle(&self) -> HPCON {
        self.0
    }

    /// Resize the pseudoconsole; the child sees a console-resize event.
    pub fn resize(&self, size: Winsize) -> io::Result<()> {
        unsafe { ResizePseudoConsole(self.0, coord(size)) }.map_err(io::Error::other)
    }
}

impl Drop for Pcon {
    fn drop(&mut self) {
        unsafe { ClosePseudoConsole(self.0) };
    }
}

/// Create a pseudoconsole sized to `size`. Returns the guard plus the input
/// (write) and output (read) pipe ends we keep.
pub fn create(size: Winsize) -> io::Result<(Pcon, OwnedHandle, OwnedHandle)> {
    let (in_read, in_write) = pipe()?;
    let (out_read, out_write) = pipe()?;

    let handle =
        unsafe { CreatePseudoConsole(coord(size), to_handle(&in_read), to_handle(&out_write), 0) }
            .map_err(io::Error::other)?;

    // ConPTY duplicates the ends it needs; drop our copies of them.
    drop(in_read);
    drop(out_write);

    Ok((Pcon(handle), in_write, out_read))
}

/// A pseudoconsole grid size as a console `COORD` (columns = X, rows = Y).
fn coord(size: Winsize) -> COORD {
    COORD {
        X: size.cols as i16,
        Y: size.rows as i16,
    }
}

/// Create an anonymous pipe, returning `(read, write)` owned ends.
fn pipe() -> io::Result<(OwnedHandle, OwnedHandle)> {
    let mut read = HANDLE::default();
    let mut write = HANDLE::default();
    unsafe { CreatePipe(&mut read, &mut write, None, 0) }.map_err(io::Error::other)?;
    Ok(unsafe { (own(read), own(write)) })
}

/// Borrow an owned handle as a Win32 `HANDLE`.
fn to_handle(h: &OwnedHandle) -> HANDLE {
    HANDLE(h.as_raw_handle())
}

/// Take ownership of a raw Win32 `HANDLE` as an `OwnedHandle`.
unsafe fn own(h: HANDLE) -> OwnedHandle {
    OwnedHandle::from_raw_handle(h.0)
}
