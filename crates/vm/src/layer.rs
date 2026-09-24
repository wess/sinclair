//! Apply OCI layer tarballs onto a rootfs directory.
//!
//! The rootfs is written by the (unprivileged) user, so files cannot carry
//! their real owner on the host. libkrun's macOS virtiofs reads the owner and
//! mode the guest should see from the `user.containers.override_stat` xattr
//! instead, so every entry records `uid:gid:mode` there while the host copy
//! stays readable and writable by the user.
//!
//! Every path is resolved inside the rootfs: a layer's symlink pointing at
//! `/etc` means the guest's `/etc`, never the host's.

use std::fs;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

use flate2::read::GzDecoder;
use tar::{Archive, EntryType};

const WHITEOUT: &str = ".wh.";
const OPAQUE: &str = ".wh..wh..opq";

/// Symlink hops allowed while resolving one path.
const MAX_HOPS: usize = 40;

/// Unpack the layer at `blob` (gzip or plain tar) onto `root`.
pub fn apply(blob: &Path, media_type: &str, root: &Path) -> Result<(), String> {
  let file = fs::File::open(blob).map_err(|e| format!("open layer: {e}"))?;
  if media_type.ends_with("zstd") {
    return Err("zstd-compressed layers are not supported yet".into());
  }
  let reader: Box<dyn Read> = if media_type.ends_with("tar") {
    Box::new(file)
  } else {
    Box::new(GzDecoder::new(file))
  };
  unpack(reader, root).map_err(|e| format!("unpack layer: {e}"))
}

pub(crate) fn unpack(reader: impl Read, root: &Path) -> io::Result<()> {
  let mut archive = Archive::new(reader);
  for entry in archive.entries()? {
    let mut entry = entry?;
    let Some(rel) = clean(&entry.path()?) else {
      continue;
    };
    let Some(name) = rel.file_name().and_then(|n| n.to_str()).map(str::to_string) else {
      continue;
    };
    let parent = resolve(root, rel.parent().unwrap_or(Path::new("")))?;

    if name == OPAQUE {
      if let Ok(read) = fs::read_dir(&parent) {
        for child in read.flatten() {
          remove(&child.path())?;
        }
      }
      continue;
    }
    if let Some(hidden) = name.strip_prefix(WHITEOUT) {
      remove(&parent.join(hidden))?;
      continue;
    }

    fs::create_dir_all(&parent)?;
    let dest = parent.join(&name);
    let header = entry.header();
    let mode = header.mode()? & 0o7777;
    let uid = header.uid()? as u32;
    let gid = header.gid()? as u32;

    match header.entry_type() {
      EntryType::Directory => {
        if fs::symlink_metadata(&dest).is_ok_and(|m| !m.is_dir()) {
          remove(&dest)?;
        }
        fs::create_dir_all(&dest)?;
        chmod(&dest, (mode & 0o777) | 0o700)?;
      }
      EntryType::Regular | EntryType::Continuous => {
        remove(&dest)?;
        let mut out = fs::File::create(&dest)?;
        io::copy(&mut entry, &mut out)?;
        chmod(&dest, (mode & 0o777) | 0o600)?;
      }
      EntryType::Symlink => {
        let target = entry.link_name()?.ok_or_else(|| bad("symlink without target"))?;
        remove(&dest)?;
        symlink(&target, &dest)?;
      }
      EntryType::Link => {
        let target = entry.link_name()?.ok_or_else(|| bad("hardlink without target"))?;
        let target = clean(&target).ok_or_else(|| bad("hardlink outside the rootfs"))?;
        // Link the entry itself, not what it points at when it is a symlink.
        let source = resolve(root, target.parent().unwrap_or(Path::new("")))?
          .join(target.file_name().unwrap_or_default());
        remove(&dest)?;
        fs::hard_link(source, &dest)?;
        // Same inode as its target, which already carries its ownership.
        continue;
      }
      // /dev is a devtmpfs in the guest; device nodes and fifos in a layer
      // are never what a shell needs.
      _ => continue,
    }
    own(&dest, uid, gid, mode)?;
  }
  Ok(())
}

/// A layer path made relative, or `None` for the root itself or anything
/// that climbs out with `..`.
fn clean(path: &Path) -> Option<PathBuf> {
  let mut out = PathBuf::new();
  for part in path.components() {
    match part {
      Component::Normal(p) => out.push(p),
      Component::CurDir | Component::RootDir | Component::Prefix(_) => {}
      Component::ParentDir => return None,
    }
  }
  (!out.as_os_str().is_empty()).then_some(out)
}

/// Resolve `rel` under `root`, following symlinks as the guest would see them:
/// an absolute target restarts at `root`, and `..` never climbs above it.
pub(crate) fn resolve(root: &Path, rel: &Path) -> io::Result<PathBuf> {
  let mut pending: Vec<PathBuf> = rel
    .components()
    .rev()
    .filter_map(|c| match c {
      Component::Normal(p) => Some(PathBuf::from(p)),
      Component::ParentDir => Some(PathBuf::from("..")),
      _ => None,
    })
    .collect();
  let mut parts: Vec<PathBuf> = Vec::new();
  let mut hops = 0;

  while let Some(part) = pending.pop() {
    if part.as_os_str() == ".." {
      parts.pop();
      continue;
    }
    let candidate: PathBuf = std::iter::once(root.to_path_buf()).chain(parts.iter().cloned()).chain([part.clone()]).collect();
    match fs::symlink_metadata(&candidate) {
      Ok(meta) if meta.file_type().is_symlink() => {
        hops += 1;
        if hops > MAX_HOPS {
          return Err(bad("too many symlinks"));
        }
        let target = fs::read_link(&candidate)?;
        if target.is_absolute() {
          parts.clear();
        }
        for c in target.components().rev() {
          match c {
            Component::Normal(p) => pending.push(PathBuf::from(p)),
            Component::ParentDir => pending.push(PathBuf::from("..")),
            _ => {}
          }
        }
      }
      _ => parts.push(part),
    }
  }
  Ok(std::iter::once(root.to_path_buf()).chain(parts).collect())
}

fn remove(path: &Path) -> io::Result<()> {
  match fs::symlink_metadata(path) {
    Ok(meta) if meta.is_dir() => fs::remove_dir_all(path),
    Ok(_) => fs::remove_file(path),
    Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
    Err(e) => Err(e),
  }
}

fn bad(msg: &str) -> io::Error {
  io::Error::new(io::ErrorKind::InvalidData, msg)
}

#[cfg(unix)]
fn chmod(path: &Path, mode: u32) -> io::Result<()> {
  use std::os::unix::fs::PermissionsExt;
  fs::set_permissions(path, fs::Permissions::from_mode(mode))
}

#[cfg(not(unix))]
fn chmod(_: &Path, _: u32) -> io::Result<()> {
  Ok(())
}

#[cfg(unix)]
fn symlink(target: &Path, link: &Path) -> io::Result<()> {
  std::os::unix::fs::symlink(target, link)
}

#[cfg(not(unix))]
fn symlink(_: &Path, _: &Path) -> io::Result<()> {
  Err(io::Error::new(io::ErrorKind::Unsupported, "symlinks need a unix host"))
}

/// Record the guest-visible owner and mode (see the module docs).
#[cfg(target_os = "macos")]
fn own(path: &Path, uid: u32, gid: u32, mode: u32) -> io::Result<()> {
  use std::ffi::CString;
  use std::os::unix::ffi::OsStrExt;

  let c_path = CString::new(path.as_os_str().as_bytes()).map_err(|_| bad("nul in path"))?;
  let value = format!("{uid}:{gid}:0{mode:o}");
  let rc = unsafe {
    libc::setxattr(
      c_path.as_ptr(),
      c"user.containers.override_stat".as_ptr(),
      value.as_ptr().cast(),
      value.len(),
      0,
      libc::XATTR_NOFOLLOW,
    )
  };
  if rc == 0 {
    Ok(())
  } else {
    Err(io::Error::last_os_error())
  }
}

#[cfg(not(target_os = "macos"))]
fn own(_: &Path, _: u32, _: u32, _: u32) -> io::Result<()> {
  Ok(())
}

#[cfg(test)]
#[path = "../tests/layer.rs"]
mod tests;
