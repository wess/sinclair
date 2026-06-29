//! A single row of cells.

use crate::cell::Cell;

/// One grid line.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub cells: Vec<Cell>,
    /// `true` when the line soft-wrapped into the next one (used by a
    /// future reflow-on-resize pass).
    pub wrapped: bool,
    /// `true` when a shell-integration prompt starts here (OSC 133;A),
    /// used as a jump-to-prompt target. Travels with the row into
    /// scrollback.
    pub prompt: bool,
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
        }
    }

    pub fn len(&self) -> usize {
        self.cells.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    /// Overwrite every cell and clear the wrap/prompt flags.
    pub fn fill(&mut self, cell: Cell) {
        self.cells.fill(cell);
        self.wrapped = false;
        self.prompt = false;
    }

    /// Truncate or pad with `blank` to `cols` cells.
    pub fn resize(&mut self, cols: usize, blank: Cell) {
        self.cells.resize(cols, blank);
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
        s.trim_end().to_string()
    }
}

#[cfg(test)]
#[path = "../../tests/grid/row.rs"]
mod tests;
