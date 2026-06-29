//! The visible cell grid plus its scrollback.
//!
//! Damage policy: every mutable row/cell access marks that row dirty.
//! Scrolling (any region), resizing, and scrollback clearing escalate to
//! full damage - rows shift wholesale and renderers repaint everything on
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
                self.scrollback.push_recycled(&self.lines[i]);
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

    /// Resize the grid, returning the cursor's new `(row, col)`.
    ///
    /// When the width changes on a screen with scrollback (the primary
    /// screen), soft-wrapped logical lines are rejoined and re-wrapped at the
    /// new width - content reflows instead of being truncated. The alternate
    /// screen (no scrollback) and height-only changes use a plain
    /// truncate/pad, matching what full-screen apps expect.
    pub fn resize(&mut self, cols: usize, rows: usize, cursor: (usize, usize)) -> (usize, usize) {
        let cols = cols.max(1);
        let rows = rows.max(1);
        if cols == self.cols || self.scrollback.limit() == 0 {
            self.simple_resize(cols, rows);
            return (cursor.0.min(rows - 1), cursor.1.min(cols - 1));
        }
        self.reflow(cols, rows, cursor)
    }

    /// Truncate or pad rows and columns without reflowing.
    fn simple_resize(&mut self, cols: usize, rows: usize) {
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

    /// Rejoin soft-wrapped lines and re-wrap them at `cols`, partitioning the
    /// result into `rows` live lines plus scrollback. Returns the cursor's new
    /// position. Wide-character pairs are not specially protected at the new
    /// wrap boundary (a rare split renders one column off).
    fn reflow(&mut self, cols: usize, rows: usize, cursor: (usize, usize)) -> (usize, usize) {
        let (cur_row, cur_col) = cursor;
        let last_content = self
            .lines
            .iter()
            .rposition(|r| r.wrapped || r.prompt || r.cells.iter().any(|c| *c != Cell::default()))
            .unwrap_or(0);
        let content_end = cur_row.max(last_content);

        let mut combined: Vec<Row> = self.scrollback.iter().cloned().collect();
        let cursor_combined = combined.len() + cur_row;
        combined.extend(self.lines[..=content_end].iter().cloned());

        let mut logicals: Vec<(Vec<Cell>, bool)> = Vec::new();
        let mut cur_logical = 0usize;
        let mut cur_offset = 0usize;
        let mut i = 0;
        while i < combined.len() {
            let start = i;
            let prompt = combined[start].prompt;
            let mut cells: Vec<Cell> = Vec::new();
            loop {
                if i == cursor_combined {
                    cur_logical = logicals.len();
                    cur_offset = cells.len() + cur_col;
                }
                let wrapped = combined[i].wrapped;
                cells.extend_from_slice(&combined[i].cells);
                i += 1;
                if !wrapped || i >= combined.len() {
                    break;
                }
            }
            let has_cursor = (start..i).contains(&cursor_combined);
            let mut end = cells.len();
            while end > 0 && cells[end - 1] == Cell::default() {
                end -= 1;
            }
            if has_cursor {
                end = end.max(cur_offset + 1);
            }
            if end < cells.len() {
                cells.truncate(end);
            } else if end > cells.len() {
                cells.resize(end, Cell::default());
            }
            logicals.push((cells, prompt));
        }

        let mut out: Vec<Row> = Vec::new();
        let mut cur_flat_row = 0usize;
        let mut cur_new_col = 0usize;
        for (li, (cells, prompt)) in logicals.iter().enumerate() {
            let base = out.len();
            if cells.is_empty() {
                let mut row = Row::new(cols);
                row.prompt = *prompt;
                out.push(row);
            } else {
                let nseg = cells.len().div_ceil(cols);
                for (si, chunk) in cells.chunks(cols).enumerate() {
                    let mut row = Row::filled(cols, Cell::default());
                    row.cells[..chunk.len()].copy_from_slice(chunk);
                    row.wrapped = si + 1 < nseg;
                    row.prompt = si == 0 && *prompt;
                    out.push(row);
                }
            }
            if li == cur_logical {
                cur_flat_row = base + cur_offset / cols;
                cur_new_col = cur_offset % cols;
            }
        }

        let live_start = out.len().saturating_sub(rows);
        self.scrollback.clear();
        for row in out.drain(..live_start) {
            self.scrollback.push(row);
        }
        while out.len() < rows {
            out.push(Row::new(cols));
        }
        self.lines = out;
        self.cols = cols;
        self.rows = rows;
        self.damage.mark_full();

        let live_row = cur_flat_row.saturating_sub(live_start).min(rows - 1);
        (live_row, cur_new_col.min(cols - 1))
    }
}

#[cfg(test)]
#[path = "../../tests/grid/mod.rs"]
mod tests;
