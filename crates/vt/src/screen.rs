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
    /// Decoded sixel images anchored to this screen's grid, oldest first.
    pub images: Vec<crate::sixel::Placement>,
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
            images: Vec::new(),
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

    /// Resize the screen: reflow content (primary) or truncate/pad (alt),
    /// follow the cursor to its new position, and reset the scroll region to
    /// the full screen. Tab stops on surviving columns are kept; columns
    /// beyond the old width get the default every-8 pattern.
    pub fn resize(&mut self, cols: usize, rows: usize) {
        let (row, col) = self
            .grid
            .resize(cols, rows, (self.cursor.row, self.cursor.col));
        self.cursor.row = row;
        self.cursor.col = col;
        self.cursor.pending_wrap = false;
        self.scroll_top = 0;
        self.scroll_bottom = self.grid.rows() - 1;
        let old = std::mem::take(&mut self.tabs);
        self.tabs = (0..self.grid.cols())
            .map(|c| old.get(c).copied().unwrap_or_else(|| default_tab(c)))
            .collect();
    }
}

fn default_tab(col: usize) -> bool {
    col != 0 && col.is_multiple_of(8)
}

fn default_tabs(cols: usize) -> Vec<bool> {
    (0..cols).map(default_tab).collect()
}

#[cfg(test)]
#[path = "../tests/screen.rs"]
mod tests;
