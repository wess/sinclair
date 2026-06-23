//! Kitty keyboard protocol: progressive-enhancement flags and the
//! per-screen stack that `CSI > u` / `CSI = u` / `CSI < u` manipulate and
//! `CSI ? u` queries. The flag *meaning* is consumed by the input encoder;
//! this module only tracks negotiation state.

/// Progressive enhancement flag bits.
pub mod flags {
    /// Disambiguate escape codes (report Esc, ctrl-keys, etc. as CSI u).
    pub const DISAMBIGUATE: u8 = 0b0_0001;
    /// Report press/repeat/release event types.
    pub const REPORT_EVENT_TYPES: u8 = 0b0_0010;
    /// Report shifted/base key codes alongside the key.
    pub const REPORT_ALTERNATE_KEYS: u8 = 0b0_0100;
    /// Report every key (including plain text) as an escape code.
    pub const REPORT_ALL_KEYS_AS_ESCAPE_CODES: u8 = 0b0_1000;
    /// Report associated text as code points.
    pub const REPORT_ASSOCIATED_TEXT: u8 = 0b1_0000;
    /// Every defined bit.
    pub const ALL: u8 = 0b1_1111;
}

/// Stack depth cap, matching kitty's bound.
const MAX_DEPTH: usize = 16;

/// A screen's stack of enhancement flag sets. The current flags are the
/// top of the stack (0 — legacy mode — when empty).
#[derive(Debug, Clone, Default)]
pub struct KittyKeyboard {
    stack: Vec<u8>,
}

impl KittyKeyboard {
    /// The currently active flags (0 when the stack is empty).
    pub fn current(&self) -> u8 {
        self.stack.last().copied().unwrap_or(0)
    }

    /// `CSI > flags u`: push a new entry. When full, the oldest entry is
    /// dropped (kitty behavior).
    pub fn push(&mut self, flags: u8) {
        if self.stack.len() >= MAX_DEPTH {
            self.stack.remove(0);
        }
        self.stack.push(flags & flags::ALL);
    }

    /// `CSI < number u`: pop `n` entries (at least one, clamped to depth).
    pub fn pop(&mut self, n: usize) {
        let n = n.max(1).min(self.stack.len());
        self.stack.truncate(self.stack.len() - n);
    }

    /// `CSI = flags ; mode u`: modify the current entry. mode 1 (default)
    /// sets, 2 adds bits, 3 clears bits. An empty stack first gains a 0 entry.
    pub fn set(&mut self, flags: u8, mode: u8) {
        let flags = flags & flags::ALL;
        if self.stack.is_empty() {
            self.stack.push(0);
        }
        let top = self.stack.last_mut().expect("nonempty");
        match mode {
            3 => *top &= !flags,
            2 => *top |= flags,
            _ => *top = flags,
        }
    }
}

#[cfg(test)]
#[path = "../tests/kitty.rs"]
mod tests;
