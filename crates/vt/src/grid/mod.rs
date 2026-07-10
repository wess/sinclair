//! The visible cell grid plus its scrollback.
//!
//! Damage policy: every mutable row/cell access marks that row dirty.
//! Scrolling (any region), resizing, and scrollback clearing escalate to
//! full damage - rows shift wholesale and renderers repaint everything on
//! those events anyway.

mod codec;
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

    /// Mutable scrollback access; row reads need it because compressed
    /// blocks decode through a cache on the ring.
    pub fn scrollback_mut(&mut self) -> &mut Scrollback {
        &mut self.scrollback
    }

    /// Row by absolute line: 0 is the top of the live grid, positive lines
    /// go down it, negative lines reach into scrollback (-1 is the newest
    /// history row, `-scrollback.len()` the oldest). `None` outside both.
    /// `&mut` because scrollback rows may decode out of a compressed block.
    pub fn absolute_row(&mut self, line: isize) -> Option<&Row> {
        if line >= 0 {
            self.lines.get(line as usize)
        } else {
            let idx = self.scrollback.len() as isize + line;
            usize::try_from(idx)
                .ok()
                .and_then(|i| self.scrollback.row(i))
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

    /// Mark one row damaged without touching its cells (e.g. an image
    /// placed over it).
    pub fn damage_row(&mut self, r: usize) {
        self.damage.mark_row(r);
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
    /// new width - content reflows instead of being truncated. Height-only
    /// changes on that screen shuffle rows through scrollback so the cursor
    /// line and bottom content survive. The alternate screen (no scrollback)
    /// uses a plain truncate/pad, matching what full-screen apps expect.
    pub fn resize(&mut self, cols: usize, rows: usize, cursor: (usize, usize)) -> (usize, usize) {
        let cols = cols.max(1);
        let rows = rows.max(1);
        if cols != self.cols && self.scrollback.limit() > 0 {
            return self.reflow(cols, rows, cursor);
        }
        let (mut cur_row, cur_col) = cursor;
        if self.scrollback.limit() > 0 && rows != self.rows {
            cur_row = self.shuffle_rows(rows, cur_row);
        }
        self.simple_resize(cols, rows);
        (cur_row.min(rows - 1), cur_col.min(cols - 1))
    }

    /// Height change on the primary screen: a shrink drops blank rows below
    /// the cursor first, then rotates the remaining excess top rows into
    /// scrollback; a grow pulls rows back out. Returns the cursor's new row.
    fn shuffle_rows(&mut self, rows: usize, cur_row: usize) -> usize {
        if rows < self.lines.len() {
            let mut excess = self.lines.len() - rows;
            while excess > 0
                && self.lines.len() - 1 > cur_row
                && self.lines.last().is_some_and(row_is_blank)
            {
                self.lines.pop();
                excess -= 1;
            }
            for row in self.lines.drain(..excess) {
                self.scrollback.push(row);
            }
            return cur_row.saturating_sub(excess);
        }
        let mut cur_row = cur_row;
        while self.lines.len() < rows {
            match self.scrollback.pop_newest() {
                Some(row) => {
                    self.lines.insert(0, row);
                    cur_row += 1;
                }
                None => break, // simple_resize pads the bottom
            }
        }
        cur_row
    }

    /// Truncate or pad rows and columns without reflowing.
    fn simple_resize(&mut self, cols: usize, rows: usize) {
        for line in &mut self.lines {
            line.resize(cols, Cell::default());
        }
        self.lines.truncate(rows);
        while self.lines.len() < rows {
            self.lines.push(Row::new(cols));
        }
        self.scrollback.resize_rows(cols);
        self.cols = cols;
        self.rows = rows;
        self.damage.mark_full();
    }

    /// Rejoin soft-wrapped lines and re-wrap them at `cols`, partitioning the
    /// result into `rows` live lines plus scrollback. Returns the cursor's new
    /// position. A wrap boundary that would split a wide pair backs off one
    /// column instead. The ring is drained (moved, not cloned) and rebuilt, so
    /// the committed counter is restored afterwards, adjusted only by the net
    /// row change - stable line numbers must not inflate on a width resize.
    fn reflow(&mut self, cols: usize, rows: usize, cursor: (usize, usize)) -> (usize, usize) {
        let (cur_row, cur_col) = cursor;
        let last_content = self
            .lines
            .iter()
            .rposition(|r| r.wrapped || r.prompt || r.cells.iter().any(|c| *c != Cell::default()))
            .unwrap_or(0);
        let content_end = cur_row.max(last_content);

        let committed = self.scrollback.committed();
        let old_len = self.scrollback.len();
        let mut combined: Vec<Row> = self.scrollback.take_rows().into();
        let cursor_combined = combined.len() + cur_row;
        combined.extend(self.lines.drain(..=content_end));

        let total = combined.len();
        let mut logicals: Vec<(Vec<Cell>, bool)> = Vec::new();
        let mut cur_logical = 0usize;
        let mut cur_offset = 0usize;
        let mut rows_it = combined.into_iter();
        let mut i = 0;
        while i < total {
            let start = i;
            let mut prompt = false;
            let mut cells: Vec<Cell> = Vec::new();
            loop {
                let row = rows_it.next().expect("i < total");
                if i == cursor_combined {
                    cur_logical = logicals.len();
                    cur_offset = cells.len() + cur_col;
                }
                let wrapped = row.wrapped;
                if i == start {
                    prompt = row.prompt;
                    cells = row.cells; // move the first segment's buffer
                } else {
                    cells.extend(row.cells);
                }
                i += 1;
                if !wrapped || i >= total {
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
            if cells.is_empty() {
                if li == cur_logical {
                    cur_flat_row = out.len();
                }
                let mut row = Row::new(cols);
                row.prompt = *prompt;
                out.push(row);
                continue;
            }
            let mut idx = 0;
            while idx < cells.len() {
                let mut take = cols.min(cells.len() - idx);
                // Never break between a wide head and its spacer: back off
                // one column and let the blank pad the gap.
                if take == cols
                    && take > 1
                    && idx + take < cells.len()
                    && cells[idx + take - 1].is_wide()
                {
                    take -= 1;
                }
                let mut row = Row::filled(cols, Cell::default());
                row.cells[..take].copy_from_slice(&cells[idx..idx + take]);
                row.wrapped = idx + take < cells.len();
                row.prompt = idx == 0 && *prompt;
                if li == cur_logical && (idx..idx + take).contains(&cur_offset) {
                    cur_flat_row = out.len();
                    cur_new_col = cur_offset - idx;
                }
                out.push(row);
                idx += take;
            }
        }

        let live_start = out.len().saturating_sub(rows);
        for row in out.drain(..live_start) {
            self.scrollback.push(row);
        }
        let new_len = self.scrollback.len() as u64;
        self.scrollback.set_committed(committed - old_len as u64 + new_len);
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

/// A row with no content and no flags; safe to drop off the bottom on a
/// height shrink.
fn row_is_blank(row: &Row) -> bool {
    !row.wrapped && !row.prompt && row.cells.iter().all(|c| *c == Cell::default())
}

#[cfg(test)]
#[path = "../../tests/grid/mod.rs"]
mod tests;
