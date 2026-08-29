use super::*;
use crate::graphics::parse_control;

fn ctrl(s: &str) -> Control {
  parse_control(s.as_bytes())
}

/// A scratch file under the temp dir, removed on drop.
struct Scratch(std::path::PathBuf);

impl Scratch {
  fn new(tag: &str, bytes: &[u8]) -> Self {
    let path = std::env::temp_dir().join(format!(
      "vt-media-{tag}-{}-{:?}",
      std::process::id(),
      std::thread::current().id()
    ));
    std::fs::write(&path, bytes).unwrap();
    Scratch(path)
  }

  fn payload(&self) -> Vec<u8> {
    self.0.to_str().unwrap().as_bytes().to_vec()
  }
}

impl Drop for Scratch {
  fn drop(&mut self) {
    let _ = std::fs::remove_file(&self.0);
  }
}

#[test]
fn direct_medium_passes_the_payload_through() {
  assert_eq!(fetch(&ctrl("t=d"), &[1, 2, 3]).unwrap(), vec![1, 2, 3]);
}

#[test]
fn file_medium_reads_the_named_path() {
  let f = Scratch::new("read", b"abcdef");
  assert_eq!(fetch(&ctrl("t=f"), &f.payload()).unwrap(), b"abcdef");
  assert!(f.0.exists(), "t=f must not consume the file");
}

#[test]
fn offset_and_size_window_the_file() {
  let f = Scratch::new("window", b"abcdef");
  assert_eq!(fetch(&ctrl("t=f,O=2,S=3"), &f.payload()).unwrap(), b"cde");
  // A size past the end clamps rather than failing.
  assert_eq!(fetch(&ctrl("t=f,O=4,S=99"), &f.payload()).unwrap(), b"ef");
}

#[test]
fn temp_medium_consumes_the_file() {
  let f = Scratch::new("consume", b"gone");
  assert_eq!(fetch(&ctrl("t=t"), &f.payload()).unwrap(), b"gone");
  assert!(!f.0.exists(), "t=t owns the file and must delete it");
}

#[test]
fn temp_medium_leaves_files_outside_a_temp_dir_alone() {
  // A client that misnames a real file must not lose it.
  let dir = std::env::current_dir().unwrap();
  let path = dir.join(format!("vt-media-keep-{}", std::process::id()));
  std::fs::write(&path, b"keep").unwrap();
  let payload = path.to_str().unwrap().as_bytes().to_vec();
  assert_eq!(fetch(&ctrl("t=t"), &payload).unwrap(), b"keep");
  assert!(path.exists(), "only temp-dir files may be deleted");
  std::fs::remove_file(&path).unwrap();
}

#[test]
fn a_missing_file_reports_enoent() {
  let missing = std::env::temp_dir().join("vt-media-does-not-exist");
  let payload = missing.to_str().unwrap().as_bytes().to_vec();
  assert_eq!(fetch(&ctrl("t=f"), &payload), Err(GfxError("ENOENT")));
}

#[test]
fn a_directory_is_not_data() {
  let payload = std::env::temp_dir().to_str().unwrap().as_bytes().to_vec();
  assert_eq!(fetch(&ctrl("t=f"), &payload), Err(GfxError("ENOENT")));
}

#[test]
fn an_empty_name_reports_enoent() {
  assert_eq!(fetch(&ctrl("t=f"), b""), Err(GfxError("ENOENT")));
  assert_eq!(fetch(&ctrl("t=s"), b"   "), Err(GfxError("ENOENT")));
}

#[test]
fn an_unknown_medium_is_refused() {
  assert_eq!(fetch(&ctrl("t=z"), &[0; 4]), Err(GfxError("EMEDIUM")));
}

#[cfg(unix)]
#[test]
fn shared_memory_is_read_and_unlinked() {
  use rustix::mm::{mmap, munmap, MapFlags, ProtFlags};
  use rustix::shm;

  let name = format!("/vt-shm-{}", std::process::id());
  let payload = b"shared bytes";
  let fd = shm::open(
    name.as_str(),
    shm::OFlags::CREATE | shm::OFlags::EXCL | shm::OFlags::RDWR,
    shm::Mode::RUSR | shm::Mode::WUSR,
  )
  .unwrap();
  rustix::fs::ftruncate(&fd, payload.len() as u64).unwrap();
  // SAFETY: a fresh writable mapping of an object this test just created.
  unsafe {
    let ptr = mmap(
      std::ptr::null_mut(),
      payload.len(),
      ProtFlags::READ | ProtFlags::WRITE,
      MapFlags::SHARED,
      &fd,
      0,
    )
    .unwrap();
    std::ptr::copy_nonoverlapping(payload.as_ptr(), ptr.cast::<u8>(), payload.len());
    let _ = munmap(ptr, payload.len());
  }

  // `S=` gives the exact length: `fstat` on a shared-memory object reports a
  // page-rounded size on macOS, which is why real clients always send it.
  let control = ctrl(&format!("t=s,S={}", payload.len()));
  assert_eq!(fetch(&control, name.as_bytes()).unwrap(), payload);

  // The name is consumed: a second read finds nothing.
  assert_eq!(fetch(&control, name.as_bytes()), Err(GfxError("ENOENT")));
}

