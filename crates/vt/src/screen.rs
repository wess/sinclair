//! A screen: grid + cursor + scroll region + tab stops + saved cursor.

use crate::cursor::{Cursor, SavedCursor};
use crate::grid::Grid;
use crate::kitty::KittyKeyboard;

/// Everything that is swapped wholesale between primary and alternate.
#[derive(Debug, Clone)]
pub struct Screen {
    pub grid: Grid,
    pub cursor: Cursor,
    /// DECSTBM top margin, 0-based, inclusive.
    pub scroll_top: usize,
    /// DECSTBM bottom margin, 0-based, inclusive.
    pub scroll_bottom: usize,
    /// `tabs[col]` is `true` when a tab stop is set at that column.
    pub tabs: Vec<bool>,
    /// DECSC state, if any.
    pub saved: Option<SavedCursor>,
    /// Kitty keyboard enhancement stack (per-screen, per the protocol).
    pub kitty: KittyKeyboard,
}

impl Screen {
    pub fn new(cols: usize, rows: usize, scrollback_limit: usize) -> Screen {
        let grid = Grid::new(cols, rows, scrollback_limit);
        let (cols, rows) = (grid.cols(), grid.rows());
        Screen {
            grid,
            cursor: Cursor::default(),
            scroll_top: 0,
            scroll_bottom: rows - 1,
            tabs: default_tabs(cols),
            saved: None,
            kitty: KittyKeyboard::default(),
        }
    }

    /// Next tab stop strictly after `col`, or the last column.
    pub fn next_tab(&self, col: usize) -> usize {
        let last = self.grid.cols() - 1;
        ((col + 1)..=last).find(|&c| self.tabs[c]).unwrap_or(last)
    }

    /// Previous tab stop strictly before `col`, or column 0.
    pub fn prev_tab(&self, col: usize) -> usize {
        (0..col).rev().find(|&c| self.tabs[c]).unwrap_or(0)
    }

    pub fn set_tab(&mut self, col: usize) {
        if col < self.tabs.len() {
            self.tabs[col] = true;
        }
    }

    pub fn clear_tab(&mut self, col: usize) {
        if col < self.tabs.len() {
            self.tabs[col] = false;
        }
    }

    pub fn clear_all_tabs(&mut self) {
        self.tabs.fill(false);
    }

    /// Simple resize: clamp the cursor, reset the scroll region to the full
    /// screen, and rebuild default tab stops.
    /// TODO: preserve custom tab stops and reflow content.
    pub fn resize(&mut self, cols: usize, rows: usize) {
        self.grid.resize(cols, rows);
        let (cols, rows) = (self.grid.cols(), self.grid.rows());
        self.cursor.row = self.cursor.row.min(rows - 1);
        self.cursor.col = self.cursor.col.min(cols - 1);
        self.cursor.pending_wrap = false;
        self.scroll_top = 0;
        self.scroll_bottom = rows - 1;
        self.tabs = default_tabs(cols);
    }
}

fn default_tabs(cols: usize) -> Vec<bool> {
    (0..cols).map(|c| c != 0 && c % 8 == 0).collect()
}

#[cfg(test)]
#[path = "../tests/screen.rs"]
mod tests;
