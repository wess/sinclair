//! Scrollback storage: a bounded ring buffer of rows.

use std::collections::VecDeque;

use crate::grid::row::Row;

/// Default maximum number of scrollback rows.
pub const DEFAULT_SCROLLBACK: usize = 10_000;

/// Ring buffer that rows scroll into off the top of the screen. The oldest
/// row is evicted once `limit` is reached. A limit of 0 disables storage
/// (used by the alternate screen).
#[derive(Debug, Clone)]
pub struct Scrollback {
    rows: VecDeque<Row>,
    limit: usize,
}

impl Scrollback {
    pub fn new(limit: usize) -> Scrollback {
        Scrollback {
            rows: VecDeque::new(),
            limit,
        }
    }

    pub fn limit(&self) -> usize {
        self.limit
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Append a row that scrolled off; evicts the oldest beyond the limit.
    pub fn push(&mut self, row: Row) {
        if self.limit == 0 {
            return;
        }
        if self.rows.len() == self.limit {
            self.rows.pop_front();
        }
        self.rows.push_back(row);
    }

    /// Append a copy of `row`, recycling the evicted front row's buffer when
    /// the ring is at its limit. Instead of allocating a clone of `row` and
    /// freeing the dropped front row, the front row is popped and overwritten
    /// in place (reusing its `Vec<Cell>` capacity), then pushed to the back -
    /// no per-line alloc/free once at steady state. Result is identical to
    /// `push(row.clone())`.
    pub(crate) fn push_recycled(&mut self, row: &Row) {
        if self.limit == 0 {
            return;
        }
        if self.rows.len() == self.limit {
            let mut recycled = self
                .rows
                .pop_front()
                .expect("len == limit > 0 implies a front row");
            recycled.copy_from(row);
            self.rows.push_back(recycled);
        } else {
            self.rows.push_back(row.clone());
        }
    }

    /// Row by age: index 0 is the oldest stored row.
    pub fn get(&self, index: usize) -> Option<&Row> {
        self.rows.get(index)
    }

    pub fn clear(&mut self) {
        self.rows.clear();
    }

    pub fn iter(&self) -> impl Iterator<Item = &Row> + '_ {
        self.rows.iter()
    }

    /// Resize every stored row (simple truncate/pad; no reflow yet).
    pub fn resize_rows(&mut self, cols: usize) {
        for row in &mut self.rows {
            row.resize(cols, crate::cell::Cell::default());
        }
    }
}

#[cfg(test)]
#[path = "../../tests/grid/scrollback.rs"]
mod tests;
