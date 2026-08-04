//! Event-driven config file reload support with a polling fallback for files
//! whose parent directory cannot be watched.

use notify::{EventKind, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime};

enum Signal {
    Change,
    Stop,
}

/// Stops the watcher and debounce worker when dropped.
pub struct WatchHandle {
    signal: Sender<Signal>,
    watcher: Option<notify::RecommendedWatcher>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for WatchHandle {
    fn drop(&mut self) {
        // Stop native callbacks before asking the debounce worker to exit.
        self.watcher.take();
        let _ = self.signal.send(Signal::Stop);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

fn normalized(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| {
        let Some(parent) = path.parent() else {
            return path.to_path_buf();
        };
        let parent = parent
            .canonicalize()
            .unwrap_or_else(|_| parent.to_path_buf());
        path.file_name()
            .map(|name| parent.join(name))
            .unwrap_or(parent)
    })
}

/// Watch `path` and call `on_change` after native filesystem events settle for
/// `interval`. Atomic replacement is caught by watching the parent directory.
/// If a native watch cannot be installed, a sleeping mtime poll is used as a
/// backstop. Dropping the handle stops either worker promptly.
pub fn watch(
    path: PathBuf,
    interval: Duration,
    on_change: impl Fn() + Send + 'static,
) -> WatchHandle {
    let interval = interval.max(Duration::from_millis(1));
    let (signal, changes) = mpsc::channel();
    let event_signal = signal.clone();
    let target = normalized(&path);
    let watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        let Ok(event) = event else { return };
        if matches!(event.kind, EventKind::Access(_)) {
            return;
        }
        if event
            .paths
            .iter()
            .any(|changed| normalized(changed) == target)
        {
            let _ = event_signal.send(Signal::Change);
        }
    })
    .ok()
    .and_then(|mut watcher| {
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        watcher
            .watch(&normalized(parent), RecursiveMode::NonRecursive)
            .ok()
            .map(|_| watcher)
    });

    let native = watcher.is_some();
    let thread = std::thread::spawn(move || {
        if native {
            event_loop(changes, interval, on_change);
        } else {
            poll_loop(changes, path, interval, on_change);
        }
    });
    WatchHandle {
        signal,
        watcher,
        thread: Some(thread),
    }
}

fn event_loop(changes: Receiver<Signal>, interval: Duration, on_change: impl Fn()) {
    loop {
        match changes.recv() {
            Ok(Signal::Change) => {}
            Ok(Signal::Stop) | Err(_) => return,
        }
        // Trailing-edge debounce: reload only after the editor's last write or
        // rename event, so the callback observes the complete saved file.
        loop {
            match changes.recv_timeout(interval) {
                Ok(Signal::Change) => continue,
                Ok(Signal::Stop) | Err(RecvTimeoutError::Disconnected) => return,
                Err(RecvTimeoutError::Timeout) => {
                    on_change();
                    break;
                }
            }
        }
    }
}

fn poll_loop(changes: Receiver<Signal>, path: PathBuf, interval: Duration, on_change: impl Fn()) {
    let mut last = mtime(&path);
    loop {
        match changes.recv_timeout(interval) {
            Ok(Signal::Stop) | Err(RecvTimeoutError::Disconnected) => return,
            Ok(Signal::Change) => {}
            Err(RecvTimeoutError::Timeout) => {
                let now = mtime(&path);
                if now != last {
                    last = now;
                    on_change();
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "../tests/watch.rs"]
mod tests;
