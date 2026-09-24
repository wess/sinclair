//! Sinclair's VM state, all under one root it owns:
//!
//! ```text
//! blobs/<sha256>             verified layer and config downloads
//! images/<sha256>/rootfs     an image unpacked once, never booted directly
//! images/<sha256>/config     its OCI config
//! refs/<image>               the last digest a reference resolved to
//! machines/<name>/rootfs     a tab's writable copy of an image
//! machines/<name>/owner      `<pid> persist|ephemeral`
//! ```
//!
//! A machine is a copy-on-write clone of its image (APFS `clonefile`), so it
//! costs no space until the guest writes and starts instantly.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::layer;
use crate::reference::Reference;
use crate::registry::Client;

/// An unpacked image ready to clone.
pub struct Image {
  pub rootfs: PathBuf,
  pub config: Value,
}

/// Resolve `raw` and make sure its rootfs is unpacked, pulling what is
/// missing. Offline, falls back to the digest this reference last resolved to.
pub fn image(root: &Path, raw: &str, arch: &str) -> Result<Image, String> {
  let reference = Reference::parse(raw)?;
  let ref_file = root.join("refs").join(slug(raw));
  let mut client = Client::new(reference);

  let resolved = match client.resolve(arch) {
    Ok(resolved) => resolved,
    Err(e) => {
      let digest = fs::read_to_string(&ref_file).map_err(|_| e.clone())?;
      let dir = image_dir(root, digest.trim());
      if dir.join(".ready").exists() {
        eprintln!("sinclair: {e}; using the cached {raw}");
        return load(&dir);
      }
      return Err(e);
    }
  };

  let dir = image_dir(root, &resolved.digest);
  if !dir.join(".ready").exists() {
    let _ = fs::remove_dir_all(&dir);
    let rootfs = dir.join("rootfs");
    fs::create_dir_all(&rootfs).map_err(|e| format!("create {}: {e}", rootfs.display()))?;
    let blobs = root.join("blobs");
    fs::create_dir_all(&blobs).map_err(|e| format!("create {}: {e}", blobs.display()))?;

    let total = resolved.layers.len();
    for (i, (digest, media_type)) in resolved.layers.iter().enumerate() {
      let blob = blobs.join(hex(digest));
      if !blob.exists() {
        eprintln!("Pulling layer {}/{total}", i + 1);
        client.blob(digest, &blob)?;
      }
      layer::apply(&blob, media_type, &rootfs)?;
    }
    write(&dir.join("config"), resolved.config.to_string().as_bytes())?;
    write(&dir.join(".ready"), b"")?;
  }
  let _ = fs::create_dir_all(root.join("refs"));
  let _ = fs::write(&ref_file, &resolved.digest);
  load(&dir)
}

fn load(dir: &Path) -> Result<Image, String> {
  let config = fs::read(dir.join("config")).map_err(|e| format!("read image config: {e}"))?;
  Ok(Image {
    rootfs: dir.join("rootfs"),
    config: serde_json::from_slice(&config).map_err(|e| format!("image config: {e}"))?,
  })
}

/// The machine `name`'s rootfs: reused when it persists and already exists,
/// otherwise a fresh clone of `image`.
pub fn machine(root: &Path, name: &str, image: &Image, persist: bool) -> Result<PathBuf, String> {
  let dir = machine_dir(root, name);
  let rootfs = dir.join("rootfs");
  // Two VMs on one rootfs would each think they own it (package databases,
  // lock files), so a machine runs in one tab at a time.
  if owner(&dir).is_some_and(|(pid, _)| pid != std::process::id() as i32 && alive(pid)) {
    return Err(format!("{name} is already running in another tab"));
  }
  if !(persist && rootfs.exists()) {
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    clone(&image.rootfs, &rootfs)?;
  }
  let kind = if persist { "persist" } else { "ephemeral" };
  write(&dir.join("owner"), format!("{} {kind}", std::process::id()).as_bytes())?;
  Ok(rootfs)
}

/// Remove machine `name` entirely.
pub fn remove(root: &Path, name: &str) {
  let _ = fs::remove_dir_all(machine_dir(root, name));
}

/// Remove ephemeral machines whose owning process is gone: a tab closed while
/// its VM was being torn down, or Sinclair was killed.
pub fn sweep(root: &Path) {
  let Ok(entries) = fs::read_dir(root.join("machines")) else {
    return;
  };
  for entry in entries.flatten() {
    if let Some((pid, false)) = owner(&entry.path()) {
      if !alive(pid) {
        let _ = fs::remove_dir_all(entry.path());
      }
    }
  }
}

/// A machine's `owner` file: the pid running it and whether it persists.
fn owner(dir: &Path) -> Option<(i32, bool)> {
  let text = fs::read_to_string(dir.join("owner")).ok()?;
  let mut parts = text.split_whitespace();
  let pid = parts.next()?.parse().ok()?;
  Some((pid, parts.next() == Some("persist")))
}

pub fn machine_dir(root: &Path, name: &str) -> PathBuf {
  root.join("machines").join(slug(name))
}

fn image_dir(root: &Path, digest: &str) -> PathBuf {
  root.join("images").join(hex(digest))
}

fn hex(digest: &str) -> &str {
  digest.strip_prefix("sha256:").unwrap_or(digest)
}

/// A reference or name as a single safe path component.
pub(crate) fn slug(raw: &str) -> String {
  raw
    .chars()
    .map(|c| if c.is_ascii_alphanumeric() || c == '.' { c } else { '+' })
    .collect()
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
  fs::write(path, bytes).map_err(|e| format!("write {}: {e}", path.display()))
}

#[cfg(target_os = "macos")]
fn clone(from: &Path, to: &Path) -> Result<(), String> {
  use std::ffi::CString;
  use std::os::unix::ffi::OsStrExt;

  let src = CString::new(from.as_os_str().as_bytes()).map_err(|_| "nul in path".to_string())?;
  let dst = CString::new(to.as_os_str().as_bytes()).map_err(|_| "nul in path".to_string())?;
  // `CLONE_NOFOLLOW` from <sys/clonefile.h>; the libc crate does not export it.
  const NOFOLLOW: u32 = 0x0001;
  // Clones the whole tree, xattrs included, in one call.
  if unsafe { libc::clonefile(src.as_ptr(), dst.as_ptr(), NOFOLLOW) } == 0 {
    Ok(())
  } else {
    Err(format!("clone rootfs: {}", std::io::Error::last_os_error()))
  }
}

#[cfg(not(target_os = "macos"))]
fn clone(_: &Path, _: &Path) -> Result<(), String> {
  Err("built-in OS Tabs are macOS-only for now".into())
}

#[cfg(unix)]
fn alive(pid: i32) -> bool {
  let rc = unsafe { libc::kill(pid, 0) };
  rc == 0 || std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
}

#[cfg(not(unix))]
fn alive(_: i32) -> bool {
  true
}

#[cfg(test)]
#[path = "../tests/store.rs"]
mod tests;
