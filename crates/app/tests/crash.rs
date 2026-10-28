use super::*;
use std::process::Command;

#[test]
fn saves_original_panic_without_requiring_backtrace_env() {
  let dir = std::env::temp_dir().join(format!("sinclair-crash-{}", std::process::id()));
  std::fs::create_dir_all(&dir).unwrap();
  let path = dir.join("crash.log");
  std::fs::write(&path, "old report").unwrap();
  let output = Command::new(std::env::current_exe().unwrap())
    .args(["--exact", "crash::tests::panic_child", "--nocapture"])
    .env("SINCLAIR_TEST_CRASH_PATH", &path)
    .env_remove("RUST_BACKTRACE")
    .output()
    .unwrap();
  assert!(
    output.status.success(),
    "{}",
    String::from_utf8_lossy(&output.stderr)
  );
  let report = std::fs::read_to_string(&path).unwrap();
  assert!(report.contains("original keyboard panic"), "{report}");
  assert!(report.contains("tests/crash.rs:"), "{report}");
  assert!(report.contains("crash::tests::panic_child"), "{report}");
  assert!(!report.contains("secondary panic"), "{report}");
  assert!(!report.contains("old report"), "{report}");
  // The normal stderr hook still runs for both panics.
  let stderr = String::from_utf8_lossy(&output.stderr);
  assert!(stderr.contains("original keyboard panic"));
  assert!(stderr.contains("secondary panic"));
  std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn panic_child() {
  let Some(path) = std::env::var_os("SINCLAIR_TEST_CRASH_PATH") else {
    return;
  };
  install_at(PathBuf::from(path));
  if std::env::var_os("SINCLAIR_TEST_CRASH_ABORT").is_some() {
    native_callback();
  }
  assert!(std::panic::catch_unwind(|| panic!("original keyboard panic")).is_err());
  assert!(std::panic::catch_unwind(|| panic!("secondary panic")).is_err());
}

extern "C" fn native_callback() {
  panic!("original native callback panic");
}

#[test]
fn saves_panic_before_native_callback_aborts() {
  let path = std::env::temp_dir().join(format!("sinclair-native-crash-{}.log", std::process::id()));
  let output = Command::new(std::env::current_exe().unwrap())
    .args(["--exact", "crash::tests::panic_child", "--nocapture"])
    .env("SINCLAIR_TEST_CRASH_PATH", &path)
    .env("SINCLAIR_TEST_CRASH_ABORT", "1")
    .env_remove("RUST_BACKTRACE")
    .output()
    .unwrap();
  use std::os::unix::process::ExitStatusExt;
  assert_eq!(output.status.signal(), Some(libc::SIGABRT));
  let report = std::fs::read_to_string(&path).unwrap();
  assert!(
    report.contains("original native callback panic"),
    "{report}"
  );
  assert!(report.contains("native_callback"), "{report}");
  assert!(
    !report.contains("panic in a function that cannot unwind"),
    "{report}"
  );
  std::fs::remove_file(path).unwrap();
}

#[test]
fn unwritable_child() {
  let Some(path) = std::env::var_os("SINCLAIR_TEST_CRASH_UNWRITABLE") else {
    return;
  };
  install_at(PathBuf::from(path));
  assert!(std::panic::catch_unwind(|| panic!("unwritable log")).is_err());
}

#[test]
fn logging_failure_preserves_normal_panic_handling() {
  let output = Command::new(std::env::current_exe().unwrap())
    .args(["--exact", "crash::tests::unwritable_child", "--nocapture"])
    .env("SINCLAIR_TEST_CRASH_UNWRITABLE", std::env::temp_dir())
    .output()
    .unwrap();
  assert!(
    output.status.success(),
    "{}",
    String::from_utf8_lossy(&output.stderr)
  );
  assert!(String::from_utf8_lossy(&output.stderr).contains("unwritable log"));
}
