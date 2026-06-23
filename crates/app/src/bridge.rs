//! Bridge the session's blocking std event channel into an async stream
//! that a gpui foreground task can poll.

use std::sync::mpsc::Receiver;

use futures::channel::mpsc::UnboundedReceiver;
use terminal::Event;

/// Spawn a thread that blocks on the std receiver and forwards every
/// event into an async channel. The thread ends when either side closes:
/// the session drops its sender (reader thread exits) or the consumer
/// drops the returned receiver.
pub fn forward(events: Receiver<Event>) -> UnboundedReceiver<Event> {
    let (tx, rx) = futures::channel::mpsc::unbounded();
    std::thread::Builder::new()
        .name("eventbridge".to_string())
        .spawn(move || {
            while let Ok(event) = events.recv() {
                if tx.unbounded_send(event).is_err() {
                    break;
                }
            }
        })
        .expect("spawn event bridge thread");
    rx
}

#[cfg(test)]
#[path = "../tests/bridge.rs"]
mod tests;
