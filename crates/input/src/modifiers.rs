//! Turning a windowing system's modifier *state* into the key *events* the
//! kitty keyboard protocol reports in all-keys mode.
//!
//! Platforms hand terminals a snapshot — "shift and control are down now" —
//! rather than "shift was pressed". A program that asked to see every key
//! wants the events, so the difference between two snapshots is turned back
//! into presses and releases here, once, for every host.

use crate::{KeyEvent, Mods};

/// One modifier: the key name the protocol spells it by, and how to read it
/// out of a snapshot.
type Modifier = (&'static str, fn(&Mods) -> bool);

/// The modifier keys. Left variants: a state snapshot cannot say which side
/// was used, and left is what the protocol treats as unqualified.
const KEYS: [Modifier; 6] = [
  ("left_shift", |m| m.shift),
  ("left_control", |m| m.ctrl),
  ("left_alt", |m| m.alt),
  ("left_super", |m| m.cmd),
  ("left_hyper", |m| m.hyper),
  ("left_meta", |m| m.meta),
];

/// The key events between two modifier snapshots, releases first so a chord
/// that swaps one modifier for another reads in the order it happened.
///
/// A lock key reports a press *and* a release on every change: it is a key
/// that was struck, and the state only tells us that it was.
pub fn modifier_events(before: Mods, after: Mods) -> Vec<(&'static str, KeyEvent)> {
  let mut out = Vec::new();
  for (name, held) in KEYS {
    if held(&before) && !held(&after) {
      out.push((name, KeyEvent::Release));
    }
  }
  for (name, held) in KEYS {
    if !held(&before) && held(&after) {
      out.push((name, KeyEvent::Press));
    }
  }
  for (name, was, is) in [
    ("capslock", before.caps_lock, after.caps_lock),
    ("numlock", before.num_lock, after.num_lock),
  ] {
    if was != is {
      out.push((name, KeyEvent::Press));
      out.push((name, KeyEvent::Release));
    }
  }
  out
}

#[cfg(test)]
#[path = "../tests/modifiers.rs"]
mod tests;
