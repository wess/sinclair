//! Transmission media: where a graphics command's bytes actually come from.
//!
//! `t=d` carries the data inline and needs nothing from this module. The other
//! three name something outside the stream — a file (`t=f`), a temporary file
//! the terminal consumes (`t=t`), or a POSIX shared-memory object (`t=s`) —
//! and reading those is the only I/O the `vt` crate does. It is confined here,
//! bounded by the same size cap as an inline payload, and every failure comes
//! back as the error code the protocol's response reports.

use std::path::Path;

use super::decode::MAX_DECODED_BYTES;
use super::{Control, GfxError};

/// Fetch the bytes a transmission command refers to. `payload` is the
/// already-base64-decoded payload: the pixel data itself for `t=d`, otherwise
/// the name of the file or shared-memory object to read.
pub(crate) fn fetch(control: &Control, payload: &[u8]) -> Result<Vec<u8>, GfxError> {
  match control.medium {
    b'd' => Ok(payload.to_vec()),
    b'f' | b't' => {
      let path = name(payload)?;
      let data = read_file(Path::new(path), control.offset, control.size)?;
      if control.medium == b't' && is_temporary(Path::new(path)) {
        // `t=t` hands the terminal ownership of the file. Deleting it is
        // part of the contract, but only inside a temp directory — a client
        // that misnames a real file should not lose it.
        let _ = std::fs::remove_file(path);
      }
      Ok(data)
    }
    b's' => {
      let name = name(payload)?;
      read_shm(name, control.offset, control.size)
    }
    _ => Err(GfxError("EMEDIUM")),
  }
}

/// The payload as a path or shared-memory name.
fn name(payload: &[u8]) -> Result<&str, GfxError> {
  std::str::from_utf8(payload)
    .map(str::trim)
    .ok()
    .filter(|s| !s.is_empty())
    .ok_or(GfxError("ENOENT"))
}

/// Read `size` bytes (0 = to the end) starting at `offset`, capped.
fn read_file(path: &Path, offset: u32, size: u32) -> Result<Vec<u8>, GfxError> {
  use std::io::{Read, Seek, SeekFrom};

  let mut file = std::fs::File::open(path).map_err(|_| GfxError("ENOENT"))?;
  let meta = file.metadata().map_err(|_| GfxError("ENOENT"))?;
  if !meta.is_file() {
    // A directory or a device would either fail or block; neither is data.
    return Err(GfxError("ENOENT"));
  }
  if offset > 0 {
    file
      .seek(SeekFrom::Start(offset as u64))
      .map_err(|_| GfxError("ESIZE"))?;
  }
  let remaining = meta.len().saturating_sub(offset as u64);
  let want = if size == 0 {
    remaining
  } else {
    (size as u64).min(remaining)
  };
  if want > MAX_DECODED_BYTES {
    return Err(GfxError("ESIZE"));
  }
  let mut out = Vec::with_capacity(want as usize);
  file
    .take(want)
    .read_to_end(&mut out)
    .map_err(|_| GfxError("ENOENT"))?;
  Ok(out)
}

/// Whether a `t=t` path sits somewhere the terminal may delete from.
fn is_temporary(path: &Path) -> bool {
  let temp = std::env::temp_dir();
  let roots: [&Path; 4] = [
    temp.as_path(),
    Path::new("/tmp"),
    Path::new("/var/tmp"),
    Path::new("/dev/shm"),
  ];
  roots.iter().any(|root| path.starts_with(root))
}

/// Read a POSIX shared-memory object and unlink it, as `t=s` requires.
///
/// The fd cannot be `read(2)` on every platform — macOS only allows mapping a
/// shared-memory object, and only as a *shared* mapping — so it is always
/// mapped `MAP_SHARED` read-only, copied out, and unmapped. `fstat` on a
/// shared-memory object reports the page-rounded size on some platforms, so a
/// client that cares about the exact length sends `S=`.
#[cfg(unix)]
fn read_shm(name: &str, offset: u32, size: u32) -> Result<Vec<u8>, GfxError> {
  use rustix::mm::{mmap, munmap, MapFlags, ProtFlags};
  use rustix::shm;

  let fd = shm::open(name, shm::OFlags::RDONLY, shm::Mode::empty())
    .map_err(|_| GfxError("ENOENT"))?;
  // The name is consumed whether or not the read succeeds: the client handed
  // the object over, and leaving it behind leaks a name until reboot.
  let _unlink = Unlink(name);
  let len = rustix::fs::fstat(&fd)
    .map_err(|_| GfxError("ENOENT"))?
    .st_size as u64;
  let remaining = len.saturating_sub(offset as u64);
  let want = if size == 0 {
    remaining
  } else {
    (size as u64).min(remaining)
  };
  if want == 0 {
    return Ok(Vec::new());
  }
  if want > MAX_DECODED_BYTES {
    return Err(GfxError("ESIZE"));
  }
  // Map from a page-aligned base and index in, since `mmap` rejects an
  // arbitrary offset.
  let page = rustix::param::page_size() as u64;
  let base = (offset as u64 / page) * page;
  let skew = (offset as u64 - base) as usize;
  let span = skew + want as usize;

  // SAFETY: a fresh read-only private mapping of a file descriptor we own,
  // independent of every other allocation; it is copied out and unmapped
  // before this function returns.
  let ptr = unsafe {
    mmap(
      std::ptr::null_mut(),
      span,
      ProtFlags::READ,
      MapFlags::SHARED,
      &fd,
      base,
    )
    .map_err(|_| GfxError("EBADF"))?
  };
  // SAFETY: `ptr` maps `span` readable bytes, and `skew + want <= span`.
  let out = unsafe { std::slice::from_raw_parts(ptr.cast::<u8>().add(skew), want as usize) }.to_vec();
  // SAFETY: unmapping exactly the mapping created above.
  unsafe {
    let _ = munmap(ptr, span);
  }
  Ok(out)
}

#[cfg(not(unix))]
fn read_shm(_name: &str, _offset: u32, _size: u32) -> Result<Vec<u8>, GfxError> {
  Err(GfxError("EMEDIUM"))
}

/// Unlinks a shared-memory name on drop, so an early return still consumes it.
#[cfg(unix)]
struct Unlink<'a>(&'a str);

#[cfg(unix)]
impl Drop for Unlink<'_> {
  fn drop(&mut self) {
    let _ = rustix::shm::unlink(self.0);
  }
}

#[cfg(test)]
#[path = "../../tests/graphics/media.rs"]
mod tests;
