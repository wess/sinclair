//! The visible cell grid plus its scrollback.
//!
//! Damage policy: every mutable row/cell access marks that row dirty.
//! Scrolling (any region), resizing, and scrollback clearing escalate to
//! full damage — rows shift wholesale and renderers repaint everything on
//! those events anyway.

pub mod damage;
pub mod row;
pub mod scrollback;

use crate::cell::Cell;
use damage::{Damage, DamageTracker};
use row::Row;
use scrollback::Scrollback;

/// `cols` x `rows` of cells, with rows that scroll off the top landing in
/// the scrollback ring buffer. A fresh grid reports full damage.
#[derive(Debug, Clone)]
pub struct Grid {
    cols: usize,
    rows: usize,
    lines: Vec<Row>,
    scrollback: Scrollback,
    damage: DamageTracker,
}

impl Grid {
    pub fn new(cols: usize, rows: usize, scrollback_limit: usize) -> Grid {
        let cols = cols.max(1);
        let rows = rows.max(1);
        Grid {
            cols,
            rows,
            lines: (0..rows).map(|_| Row::new(cols)).collect(),
            scrollback: Scrollback::new(scrollback_limit),
            damage: DamageTracker::new(),
        }
    }

    pub fn cols(&self) -> usize {
        self.cols
    }

    pub fn rows(&self) -> usize {
        self.rows
    }

    pub fn row(&self, r: usize) -> &Row {
        &self.lines[r]
    }

    pub fn row_mut(&mut self, r: usize) -> &mut Row {
        self.damage.mark_row(r);
        &mut self.lines[r]
    }

    pub fn cell(&self, r: usize, c: usize) -> &Cell {
        &self.lines[r].cells[c]
    }

    pub fn cell_mut(&mut self, r: usize, c: usize) -> &mut Cell {
        self.damage.mark_row(r);
        &mut self.lines[r].cells[c]
    }

    pub fn scrollback(&self) -> &Scrollback {
        &self.scrollback
    }

    /// Row by absolute line: 0 is the top of the live grid, positive lines
    /// go down it, negative lines reach into scrollback (-1 is the newest
    /// history row, `-scrollback.len()` the oldest). `None` outside both.
    pub fn absolute_row(&self, line: isize) -> Option<&Row> {
        if line >= 0 {
            self.lines.get(line as usize)
        } else {
            let idx = self.scrollback.len() as isize + line;
            usize::try_from(idx)
                .ok()
                .and_then(|i| self.scrollback.get(i))
        }
    }

    /// Clears history; full damage because the viewport may show it.
    pub fn clear_scrollback(&mut self) {
        self.scrollback.clear();
        self.damage.mark_full();
    }

    /// Return accumulated damage and reset to clean.
    pub fn take_damage(&mut self) -> Damage {
        self.damage.take()
    }

    /// Escalate to full damage (whole-screen events).
    pub fn damage_full(&mut self) {
        self.damage.mark_full();
    }

    /// Scroll lines `top..=bottom` up by `n`. Rows leaving the top are
    /// pushed into scrollback when `save` is set; vacated rows at the
    /// bottom become `blank`.
    pub fn scroll_up(&mut self, top: usize, bottom: usize, n: usize, save: bool, blank: Cell) {
        if top > bottom || bottom >= self.rows {
            return;
        }
        let len = bottom - top + 1;
        let n = n.min(len);
        if n == 0 {
            return;
        }
        if save {
            for i in top..top + n {
                self.scrollback.push(self.lines[i].clone());
            }
        }
        self.lines[top..=bottom].rotate_left(n);
        for i in bottom + 1 - n..=bottom {
            self.lines[i].fill(blank);
        }
        self.damage.mark_full();
    }

    /// Scroll lines `top..=bottom` down by `n`; vacated rows at the top
    /// become `blank`. Nothing enters scrollback.
    pub fn scroll_down(&mut self, top: usize, bottom: usize, n: usize, blank: Cell) {
        if top > bottom || bottom >= self.rows {
            return;
        }
        let len = bottom - top + 1;
        let n = n.min(len);
        if n == 0 {
            return;
        }
        self.lines[top..=bottom].rotate_right(n);
        for i in top..top + n {
            self.lines[i].fill(blank);
        }
        self.damage.mark_full();
    }

    /// Simple resize: truncate or pad rows and columns.
    /// TODO: reflow soft-wrapped lines (the `Row::wrapped` flag is already
    /// tracked for this) instead of truncating/padding.
    pub fn resize(&mut self, cols: usize, rows: usize) {
        let cols = cols.max(1);
        let rows = rows.max(1);
        for line in &mut self.lines {
            line.resize(cols, Cell::default());
        }
        if rows < self.rows {
            self.lines.truncate(rows);
        } else {
            while self.lines.len() < rows {
                self.lines.push(Row::new(cols));
            }
        }
        self.scrollback.resize_rows(cols);
        self.cols = cols;
        self.rows = rows;
        self.damage.mark_full();
    }
}

#[cfg(test)]
#[path = "../../tests/grid/mod.rs"]
mod tests;
