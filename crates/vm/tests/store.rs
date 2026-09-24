use super::*;

fn tmp() -> PathBuf {
  static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
  let dir = std::env::temp_dir().join(format!(
    "sinclairvmstore{}x{}",
    std::process::id(),
    N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
  ));
  let _ = fs::remove_dir_all(&dir);
  fs::create_dir_all(&dir).unwrap();
  dir
}

#[test]
fn slug_is_one_path_component() {
  assert_eq!(slug("ghcr.io/org/tools:1"), "ghcr.io+org+tools+1");
  assert_eq!(slug("../../etc"), "..+..+etc");
  assert!(!slug("a/b").contains('/'));
}

#[test]
fn sweep_removes_dead_ephemeral_machines_only() {
  let root = tmp();
  let dead = i32::MAX - 1;
  for (name, owner) in [
    ("gone", format!("{dead} ephemeral")),
    ("kept", format!("{dead} persist")),
    ("live", format!("{} ephemeral", std::process::id())),
  ] {
    let dir = machine_dir(&root, name);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("owner"), owner).unwrap();
  }
  sweep(&root);
  assert!(!machine_dir(&root, "gone").exists());
  assert!(machine_dir(&root, "kept").exists());
  assert!(machine_dir(&root, "live").exists());
}

#[cfg(target_os = "macos")]
#[test]
fn machine_clones_and_persists() {
  let root = tmp();
  let base = root.join("base");
  fs::create_dir_all(base.join("etc")).unwrap();
  fs::write(base.join("etc/os-release"), "ID=test\n").unwrap();
  let image = Image {
    rootfs: base,
    config: Value::Null,
  };

  let rootfs = machine(&root, "m", &image, true).unwrap();
  assert_eq!(fs::read_to_string(rootfs.join("etc/os-release")).unwrap(), "ID=test\n");
  fs::write(rootfs.join("etc/state"), "kept").unwrap();

  // Persistent: the second start sees what the first wrote.
  let again = machine(&root, "m", &image, true).unwrap();
  assert!(again.join("etc/state").exists());

  // Ephemeral: always a fresh clone.
  let fresh = machine(&root, "m", &image, false).unwrap();
  assert!(!fresh.join("etc/state").exists());
}

#[cfg(target_os = "macos")]
#[test]
fn a_machine_runs_in_one_tab_at_a_time() {
  let root = tmp();
  let base = root.join("base");
  fs::create_dir_all(&base).unwrap();
  let image = Image {
    rootfs: base,
    config: Value::Null,
  };
  let dir = machine_dir(&root, "busy");
  fs::create_dir_all(&dir).unwrap();
  // pid 1 (launchd) is always alive and never us.
  fs::write(dir.join("owner"), "1 persist").unwrap();
  assert!(machine(&root, "busy", &image, true).is_err());
}
