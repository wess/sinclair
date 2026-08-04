//! Adapt a session event receiver into the async stream consumed by gpui.

use futures::Stream;
use terminal::{Event, EventReceiver};

/// Convert the session's dual blocking/async receiver into an owned stream.
/// No forwarding thread or second unbounded queue is needed.
pub fn forward(events: EventReceiver) -> impl Stream<Item = Event> {
    events.into_stream()
}

#[cfg(test)]
#[path = "../tests/bridge.rs"]
mod tests;
