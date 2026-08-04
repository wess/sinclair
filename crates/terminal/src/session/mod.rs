//! A live terminal session: pty child + vt emulation + reader thread.
//!
//! Both backends expose the same [`Session`] surface. On Unix the reader
//! thread blocks in `poll()` on the master plus a wake pipe (nonblocking
//! writes, deterministic teardown via group SIGHUP/SIGKILL). On Windows the
//! reader blocks in ConPTY pipe reads and teardown closes the console,
//! which unblocks the reader at EOF.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

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
