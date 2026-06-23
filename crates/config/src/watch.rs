//! Config file reload support: a background thread polls the file's
//! mtime and fires a callback when it changes. No external dependencies.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime};

/// Stops the watcher thread when dropped.
pub struct WatchHandle {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for WatchHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// Watch `path` for mtime changes, polling every `interval`, and call
/// `on_change` whenever it differs (including the file appearing or
/// disappearing). Dropping the returned handle stops the thread promptly,
/// even with a long interval.
pub fn watch(
    path: PathBuf,
    interval: Duration,
    on_change: impl Fn() + Send + 'static,
) -> WatchHandle {
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    let thread = std::thread::spawn(move || {
        let slice = Duration::from_millis(20);
        let mut last = mtime(&path);
        loop {
            // Sleep in short slices so drop is responsive.
            let mut waited = Duration::ZERO;
            while waited < interval {
                if flag.load(Ordering::Relaxed) {
                    return;
                }
                let step = slice.min(interval - waited);
                std::thread::sleep(step);
                waited += step;
            }
            if flag.load(Ordering::Relaxed) {
                return;
            }
            let now = mtime(&path);
            if now != last {
                last = now;
                on_change();
            }
        }
    });
    WatchHandle {
        stop,
        thread: Some(thread),
    }
}

#[cfg(test)]
#[path = "../tests/watch.rs"]
mod tests;
