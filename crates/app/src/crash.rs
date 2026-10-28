//! Preserve the original panic before a native callback abort hides its stack.

use std::backtrace::Backtrace;
use std::fs::OpenOptions;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn install() {
  let dir = crate::paths::data_dir("logs");
  if std::fs::create_dir_all(&dir).is_err() {
    return;
  }
  install_at(dir.join(format!("{}.crash.log", crate::appid::id())));
}

fn install_at(path: PathBuf) {
  let previous = std::panic::take_hook();
  let recorded = AtomicBool::new(false);
  std::panic::set_hook(Box::new(move |info| {
    // The subsequent "panic cannot unwind" hook must not replace the cause.
    if !recorded.swap(true, Ordering::Relaxed) {
      let thread = std::thread::current();
      let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
      let backtrace = Backtrace::force_capture();
      let mut options = OpenOptions::new();
      options.create(true).write(true).truncate(true);
      // Backtraces can carry paths and arguments: keep the log owner-only.
      #[cfg(unix)]
      options.mode(0o600);
      if let Ok(mut file) = options.open(&path) {
        let _ = writeln!(
          file,
          "Sinclair {} ({} {})\nunix time: {timestamp}\n{info}\nthread: {}\n{backtrace}",
          env!("CARGO_PKG_VERSION"),
          std::env::consts::OS,
          std::env::consts::ARCH,
          thread.name().unwrap_or("unnamed"),
        );
        let _ = file.flush();
      }
    }
    previous(info);
  }));
}

#[cfg(test)]
#[path = "../tests/crash.rs"]
mod tests;
