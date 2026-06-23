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

    /// Row contents as text, skipping wide spacers, right-trimmed.
    /// Primarily for tests and debugging.
    pub fn text(&self) -> String {
        let s: String = self
            .cells
            .iter()
            .filter(|c| !c.is_wide_spacer())
            .map(|c| c.ch)
            .collect();
        s.trim_end().to_string()
    }
}

#[cfg(test)]
#[path = "../../tests/grid/row.rs"]
mod tests;
