//! A live terminal session: pty child + vt emulation + reader thread.
//!
//! Both backends expose the same [`Session`] surface. On Unix the reader
//! thread blocks in `poll()` on the master plus a wake pipe (nonblocking
//! writes, deterministic teardown via group SIGHUP/SIGKILL). On Windows the
//! reader blocks in ConPTY pipe reads and teardown closes the console,
//! which unblocks the reader at EOF.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::Event;

/// The UI normally drains events once per frame. Keep a small reserve for
/// metadata bursts, plus dedicated slots for the one coalesced wakeup and the
/// eventual exit event. Large OSC payloads make an item-count bound important:
/// eight maximum-sized clipboard events retain at most 64 MiB.
const MAX_QUEUED_METADATA_EVENTS: usize = 8;
const EVENT_CHANNEL_CAPACITY: usize = MAX_QUEUED_METADATA_EVENTS + 2;

/// Monotonic counters for profiling a live session without attaching a
/// sampler. Values are cumulative from spawn; the pending-input value is a
/// point-in-time gauge.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SessionStats {
  pub output_bytes: u64,
  pub output_chunks: u64,
  pub wakeups: u64,
  pub wakeup_acknowledgements: u64,
  pub resize_commits: u64,
  pub input_bytes: u64,
  pub pending_input_bytes: usize,
  pub dropped_events: u64,
}

#[derive(Default)]
struct Counters {
  output_bytes: AtomicU64,
  output_chunks: AtomicU64,
  wakeups: AtomicU64,
  wakeup_acknowledgements: AtomicU64,
  resize_commits: AtomicU64,
  input_bytes: AtomicU64,
  dropped_events: AtomicU64,
}

impl Counters {
  fn snapshot(&self, pending_input_bytes: usize) -> SessionStats {
    SessionStats {
      output_bytes: self.output_bytes.load(Ordering::Relaxed),
      output_chunks: self.output_chunks.load(Ordering::Relaxed),
      wakeups: self.wakeups.load(Ordering::Relaxed),
      wakeup_acknowledgements: self.wakeup_acknowledgements.load(Ordering::Relaxed),
      resize_commits: self.resize_commits.load(Ordering::Relaxed),
      input_bytes: self.input_bytes.load(Ordering::Relaxed),
      pending_input_bytes,
      dropped_events: self.dropped_events.load(Ordering::Relaxed),
    }
  }
}

/// Nonblocking sender for the bounded session event queue. Wakeup and exit
/// have reserved capacity; excess metadata is dropped instead of blocking the
/// pty reader thread or allowing a stalled UI to grow memory without bound.
struct EventSender {
  inner: flume::Sender<Event>,
  counters: Arc<Counters>,
}

impl EventSender {
  fn send(&self, event: Event) -> Result<(), flume::SendError<Event>> {
    let critical = matches!(event, Event::Wakeup | Event::Exit(_));
    if !critical && self.inner.len() >= MAX_QUEUED_METADATA_EVENTS {
      self.counters.dropped_events.fetch_add(1, Ordering::Relaxed);
      return Ok(());
    }

    match self.inner.try_send(event) {
      Ok(()) => Ok(()),
      Err(flume::TrySendError::Disconnected(event)) => Err(flume::SendError(event)),
      Err(flume::TrySendError::Full(_)) if !critical => {
        self.counters.dropped_events.fetch_add(1, Ordering::Relaxed);
        Ok(())
      }
      Err(flume::TrySendError::Full(event)) => Err(flume::SendError(event)),
    }
  }
}

fn event_channel(counters: Arc<Counters>) -> (EventSender, crate::EventReceiver) {
  let (inner, receiver) = flume::bounded(EVENT_CHANNEL_CAPACITY);
  (EventSender { inner, counters }, receiver)
}

/// Replay a saved buffer into the emulator before the reader thread can
/// deliver a single byte from the child, so restored history is always
/// *under* the new shell's first prompt rather than interleaved with it.
/// Anything the replay asks the host to write back is dropped: those replies
/// would belong to the session that ended, not this child.
///
/// A panic in here would take the app down on the way *up*, before a window
/// exists — and the buffer that caused it would still be on disk for the
/// next launch to trip over. So a failed replay is caught the way the reader
/// thread catches one: log it, start the pane from a clean terminal, and let
/// the session open without its history rather than not at all.
pub(crate) fn preload(term: &Mutex<vt::Terminal>, bytes: &[u8], scrollback_limit: usize) {
  if bytes.is_empty() {
    return;
  }
  let mut term = term.lock().unwrap_or_else(|e| e.into_inner());
  let fed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| term.feed(bytes)));
  if fed.is_err() {
    eprintln!(
      "vt: parser panicked replaying a {}-byte saved buffer; opening the pane empty",
      bytes.len()
    );
    *term = vt::Terminal::new(term.cols(), term.rows(), scrollback_limit);
  }
  term.take_output();
}

#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub use unix::Session;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::Session;

#[cfg(test)]
#[path = "../../tests/session_queue.rs"]
mod queue_tests;
