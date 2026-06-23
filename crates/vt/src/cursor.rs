//! Cursor state: position, pen, pending-wrap, saved cursor, and style.

use crate::cell::Cell;
use crate::charset::Charsets;

/// The live cursor of a screen.
#[derive(Debug, Clone, Copy, Default)]
pub struct Cursor {
    /// 0-based row in grid coordinates.
    pub row: usize,
    /// 0-based column.
    pub col: usize,
    /// Template cell carrying the current SGR attributes.
    pub pen: Cell,
    /// Set after printing in the last column with autowrap on; the next
    /// printable character wraps to the start of the following line.
    pub pending_wrap: bool,
}

/// State captured by DECSC and restored by DECRC.
#[derive(Debug, Clone, Copy, Default)]
pub struct SavedCursor {
    pub row: usize,
    pub col: usize,
    pub pen: Cell,
    pub charsets: Charsets,
    pub origin: bool,
    pub pending_wrap: bool,
}

/// Cursor shape as selected by DECSCUSR (`CSI Ps SP q`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CursorStyle {
    #[default]
    BlinkingBlock,
    SteadyBlock,
    BlinkingUnderline,
    SteadyUnderline,
    BlinkingBar,
    SteadyBar,
}

impl CursorStyle {
    /// Style for a DECSCUSR parameter; `None` for out-of-range values.
    pub fn from_decscusr(param: u16) -> Option<CursorStyle> {
        match param {
            0 | 1 => Some(CursorStyle::BlinkingBlock),
            2 => Some(CursorStyle::SteadyBlock),
            3 => Some(CursorStyle::BlinkingUnderline),
            4 => Some(CursorStyle::SteadyUnderline),
            5 => Some(CursorStyle::BlinkingBar),
            6 => Some(CursorStyle::SteadyBar),
            _ => None,
        }
    }
}

#[cfg(test)]
#[path = "../tests/cursor.rs"]
mod tests;
