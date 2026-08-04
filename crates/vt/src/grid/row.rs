//! A single row of cells.

use crate::cell::Cell;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_REVISION: AtomicU64 = AtomicU64::new(1);

fn next_revision() -> u64 {
    NEXT_REVISION.fetch_add(1, Ordering::Relaxed)
}

/// One grid line.
#[derive(Debug, Clone)]
pub struct Row {
    pub cells: Vec<Cell>,
    /// `true` when the line soft-wrapped into the next one (used by a
    /// future reflow-on-resize pass).
    pub wrapped: bool,
    /// `true` when a shell-integration prompt starts here (OSC 133;A),
    /// used as a jump-to-prompt target. Travels with the row into
    /// scrollback.
    pub prompt: bool,
    /// Process-local content identity used by renderers to recognize a row
    /// after the grid rotates it during scrolling.
    revision: u64,
}

impl PartialEq for Row {
    fn eq(&self, other: &Self) -> bool {
        self.cells == other.cells && self.wrapped == other.wrapped && self.prompt == other.prompt
    }
}

impl Row {
    /// A blank row of `cols` default cells.
    pub fn new(cols: usize) -> Row {
        Row::filled(cols, Cell::default())
    }

    /// A row of `cols` copies of `cell`.
    pub fn filled(cols: usize, cell: Cell) -> Row {
        Row {
            cells: vec![cell; cols],
            wrapped: false,
            prompt: false,
            revision: next_revision(),
        }
    }

    pub(crate) fn from_parts(cells: Vec<Cell>, wrapped: bool, prompt: bool) -> Row {
        Row {
            cells,
            wrapped,
            prompt,
            revision: next_revision(),
        }
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub(crate) fn touch(&mut self) {
        self.revision = next_revision();
    }

    pub fn len(&self) -> usize {
        self.cells.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    /// Overwrite every cell and clear the wrap/prompt flags.
    pub fn fill(&mut self, cell: Cell) {
        self.touch();
        self.cells.fill(cell);
        self.wrapped = false;
        self.prompt = false;
    }

    /// Truncate or pad with `blank` to `cols` cells. A truncation that
    /// slices a wide pair leaves its stranded head blanked.
    pub fn resize(&mut self, cols: usize, blank: Cell) {
        self.touch();
        let shrunk = cols < self.cells.len();
        self.cells.resize(cols, blank);
        if shrunk && self.cells.last().is_some_and(|c| c.is_wide()) {
            self.cells[cols - 1] = blank;
        }
    }

    /// Overwrite this row's contents with `src`, reusing the existing cell
    /// buffer's allocation instead of replacing it. `Vec::clone_from` keeps
    /// the current capacity when it can, so a recycled row takes no new heap
    /// allocation. The result is identical to cloning `src`.
    pub(crate) fn copy_from(&mut self, src: &Row) {
        self.cells.clone_from(&src.cells);
        self.wrapped = src.wrapped;
        self.prompt = src.prompt;
    }

    /// Row contents as text, skipping wide spacers, right-trimmed.
    /// Includes any combining marks. Primarily for tests and debugging.
    pub fn text(&self) -> String {
        let mut s = String::with_capacity(self.cells.len());
        for c in self.cells.iter().filter(|c| !c.is_wide_spacer()) {
            c.write_grapheme(&mut s);
        }
        s.truncate(s.trim_end().len());
        s
    }
}

#[cfg(test)]
#[path = "../../tests/grid/row.rs"]
mod tests;
