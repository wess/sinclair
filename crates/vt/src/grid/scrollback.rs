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
