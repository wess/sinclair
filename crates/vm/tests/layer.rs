use super::*;
use tar::{Builder, Header};

enum Item<'a> {
  Dir(&'a str),
  File(&'a str, &'a str),
  Link(&'a str, &'a str),
  Hard(&'a str, &'a str),
}

fn layer(items: &[Item]) -> Vec<u8> {
  let mut b = Builder::new(Vec::new());
  for item in items {
    let mut h = Header::new_gnu();
    h.set_uid(0);
    h.set_gid(0);
    match item {
      Item::Dir(p) => {
        h.set_entry_type(EntryType::Directory);
        h.set_mode(0o755);
        h.set_size(0);
        b.append_data(&mut h, p, io::empty()).unwrap();
      }
      Item::File(p, body) => {
        h.set_entry_type(EntryType::Regular);
        h.set_mode(0o4755);
        h.set_size(body.len() as u64);
        b.append_data(&mut h, p, body.as_bytes()).unwrap();
      }
      Item::Link(p, target) => {
        h.set_entry_type(EntryType::Symlink);
        h.set_mode(0o777);
        h.set_size(0);
        b.append_link(&mut h, p, target).unwrap();
      }
      Item::Hard(p, target) => {
        h.set_entry_type(EntryType::Link);
        h.set_mode(0o644);
        h.set_size(0);
        b.append_link(&mut h, p, target).unwrap();
      }
    }
  }
  b.into_inner().unwrap()
}

fn tmp() -> PathBuf {
  static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
  let dir = std::env::temp_dir().join(format!(
    "sinclairvmlayer{}x{}",
    std::process::id(),
    N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
  ));
  let _ = fs::remove_dir_all(&dir);
  fs::create_dir_all(&dir).unwrap();
  dir
}

fn apply_items(root: &Path, items: &[Item]) {
  unpack(&layer(items)[..], root).unwrap();
}

#[test]
fn writes_files_dirs_and_symlinks() {
  let root = tmp();
  apply_items(
    &root,
    &[
      Item::Dir("etc/"),
      Item::File("etc/hostname", "vm\n"),
      Item::Link("etc/alias", "hostname"),
    ],
  );
  assert_eq!(fs::read_to_string(root.join("etc/hostname")).unwrap(), "vm\n");
  assert_eq!(fs::read_link(root.join("etc/alias")).unwrap(), Path::new("hostname"));
}

#[test]
fn whiteout_removes_a_lower_file() {
  let root = tmp();
  apply_items(&root, &[Item::File("a/gone", "x"), Item::File("a/kept", "y")]);
  apply_items(&root, &[Item::File("a/.wh.gone", "")]);
  assert!(!root.join("a/gone").exists());
  assert!(root.join("a/kept").exists());
}

#[test]
fn opaque_whiteout_clears_the_directory() {
  let root = tmp();
  apply_items(&root, &[Item::File("a/old", "x")]);
  apply_items(&root, &[Item::File("a/.wh..wh..opq", ""), Item::File("a/new", "y")]);
  assert!(!root.join("a/old").exists());
  assert!(root.join("a/new").exists());
}

#[test]
fn absolute_symlink_resolves_inside_the_rootfs() {
  let root = tmp();
  // A merged-usr style link, then a later layer writing through it.
  apply_items(&root, &[Item::Dir("usr/lib/"), Item::Link("lib", "/usr/lib")]);
  apply_items(&root, &[Item::File("lib/libx.so", "elf")]);
  assert_eq!(fs::read_to_string(root.join("usr/lib/libx.so")).unwrap(), "elf");
}

#[test]
fn escaping_symlink_stays_inside_the_rootfs() {
  let root = tmp();
  apply_items(&root, &[Item::Link("evil", "../../../../../../tmp")]);
  apply_items(&root, &[Item::File("evil/owned", "x")]);
  assert!(root.join("tmp/owned").exists());
}

#[test]
fn parent_dir_entries_are_skipped() {
  let root = tmp();
  let mut b = Builder::new(Vec::new());
  let mut h = Header::new_gnu();
  h.set_entry_type(EntryType::Regular);
  h.set_size(1);
  h.set_mode(0o644);
  // `append_data` refuses `..`, so write the name straight into the header.
  h.as_gnu_mut().unwrap().name[..9].copy_from_slice(b"../escape");
  h.set_cksum();
  b.append(&h, &b"x"[..]).unwrap();
  unpack(&b.into_inner().unwrap()[..], &root).unwrap();
  assert!(!root.parent().unwrap().join("escape").exists());
}

#[test]
fn hardlink_shares_the_target() {
  let root = tmp();
  apply_items(&root, &[Item::File("bin/a", "same"), Item::Hard("bin/b", "bin/a")]);
  assert_eq!(fs::read_to_string(root.join("bin/b")).unwrap(), "same");
}

#[cfg(target_os = "macos")]
#[test]
fn records_the_guest_owner_and_mode() {
  use std::ffi::CString;
  use std::os::unix::ffi::OsStrExt;

  let root = tmp();
  apply_items(&root, &[Item::File("bin/su", "x")]);
  let path = CString::new(root.join("bin/su").as_os_str().as_bytes()).unwrap();
  let mut buf = [0u8; 64];
  let n = unsafe {
    libc::getxattr(
      path.as_ptr(),
      c"user.containers.override_stat".as_ptr(),
      buf.as_mut_ptr().cast(),
      buf.len(),
      0,
      libc::XATTR_NOFOLLOW,
    )
  };
  assert!(n > 0);
  assert_eq!(&buf[..n as usize], b"0:0:04755");
}
