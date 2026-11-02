//! A short memory of what happened in every pane, for clients that wait on or
//! follow terminal events (`sinclair wait`, `sinclair events`, the MCP `wait`
//! and `events` tools).
//!
//! The socket is one request and one reply per connection, served one at a
//! time, so a client can't hold a connection open to be pushed events — it
//! would block everyone else. Instead every event gets a sequence number, and
//! a client asks for what came after the last one it saw. Old entries fall off
//! the front; a client that falls further behind than that is told so.

use std::collections::VecDeque;

use serde_json::{json, Value};

/// Entries kept.
pub const CAPACITY: usize = 2048;

/// One recorded event.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
  pub seq: u64,
  /// Milliseconds since the Unix epoch.
  pub at: u64,
  /// The pane token it happened in.
  pub pane: u64,
  /// `bell`, `title_changed`, `notify`, `exit`, `command_finished`,
  /// `dir_changed`, `status_changed`.
  pub kind: &'static str,
  pub data: Value,
}

impl Entry {
  pub fn to_json(&self) -> Value {
    json!({
      "seq": self.seq,
      "at": self.at,
      "pane": crate::target::id(self.pane),
      "event": self.kind,
      "data": self.data,
    })
  }
}

#[derive(Debug)]
pub struct Journal {
  entries: VecDeque<Entry>,
  next: u64,
}

impl Default for Journal {
  fn default() -> Self {
    Journal {
      entries: VecDeque::new(),
      next: 1,
    }
  }
}

impl Journal {
  pub fn push(&mut self, pane: u64, kind: &'static str, data: Value, at: u64) {
    if self.entries.len() >= CAPACITY {
      self.entries.pop_front();
    }
    self.entries.push_back(Entry {
      seq: self.next,
      at,
      pane,
      kind,
      data,
    });
    self.next += 1;
  }

  /// The cursor a new client starts from: everything before now is history.
  pub fn head(&self) -> u64 {
    self.next - 1
  }

  /// Entries after `after`, optionally only for `pane`. Also returns the new
  /// cursor and whether entries were lost between `after` and the oldest kept.
  pub fn since(&self, after: u64, pane: Option<u64>) -> (Vec<Entry>, u64, bool) {
    let lost = self
      .entries
      .front()
      .is_some_and(|first| first.seq > after + 1);
    let events = self
      .entries
      .iter()
      .filter(|e| e.seq > after && pane.is_none_or(|p| e.pane == p))
      .cloned()
      .collect();
    (events, self.head(), lost)
  }
}

pub fn now_ms() -> u64 {
  std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .map(|d| d.as_millis() as u64)
    .unwrap_or(0)
}

/// The app's journal, shared by every window.
#[derive(Default)]
pub struct Global(pub Journal);

impl gpui::Global for Global {}

#[cfg(test)]
#[path = "../tests/journal.rs"]
mod tests;
