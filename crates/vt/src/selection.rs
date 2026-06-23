//! Selection geometry: pure grid logic, no terminal state.
//!
//! # Coordinates
//!
//! A [`Point`] is `(line, col)` where `line: isize` is absolute to content:
//! line 0 is the top row of the live grid, positive lines go down the live
//! grid, and negative lines reach into scrollback (-1 is the newest history
//! row, `-scrollback_len` the oldest) — the same scheme as
//! [`Grid::absolute_row`]. A viewport row `i` rendered at display offset
//! `o` therefore has line `i as isize - o as isize`.
//!
//! Points stay anchored to content as the terminal scrolls: when `n` rows
//! enter scrollback every line shifts by `-n` ([`Selection::shift_lines`]),
//! and this holds even while the ring buffer evicts old rows. A selection
//! whose start drops below `-scrollback_len` has scrolled off the ring and
//! must be invalidated (the terminal layer does this).
//!
//! # Shape
//!
//! A [`Selection`] stores the *expanded* span under the anchor (where the
//! gesture started) and under the extent (where it currently is). For cell
//! mode both spans are single points; word/line modes expand them via
//! [`expand_word`] / [`expand_line`]. `start()`/`end()` normalize across
//! the two, and `contains` treats the selection as a reading-order stream
//! range (full rows between the endpoints).

use crate::grid::Grid;

/// Absolute content coordinate; see the module docs for the line scheme.
/// Ordering is reading order: by line, then by column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Point {
    pub line: isize,
    pub col: usize,
}

impl Point {
    pub fn new(line: isize, col: usize) -> Point {
        Point { line, col }
    }
}

/// How selection gestures expand points.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionMode {
    /// Exact cells.
    Cell,
    /// Whole words (alphanumerics plus configured extra characters).
    Word,
    /// Whole logical lines, following soft-wrap flags both directions.
    Line,
}

/// An active selection: mode plus the expanded anchor and extent spans.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    mode: SelectionMode,
    anchor: (Point, Point),
    extent: (Point, Point),
}

impl Selection {
    /// New selection whose anchor and extent are the same expanded `span`.
    pub fn new(mode: SelectionMode, span: (Point, Point)) -> Selection {
        Selection {
            mode,
            anchor: span,
            extent: span,
        }
    }

    pub fn mode(&self) -> SelectionMode {
        self.mode
    }

    /// Replace the extent with a new expanded span.
    pub fn update(&mut self, span: (Point, Point)) {
        self.extent = span;
    }

    /// Normalized first selected cell.
    pub fn start(&self) -> Point {
        self.anchor.0.min(self.extent.0)
    }

    /// Normalized last selected cell (inclusive).
    pub fn end(&self) -> Point {
        self.anchor.1.max(self.extent.1)
    }

    /// Whether `p` falls inside the selection, treating it as a stream
    /// range: partial first/last rows, full rows in between.
    pub fn contains(&self, p: Point) -> bool {
        self.start() <= p && p <= self.end()
    }

    /// Shift every line by `delta` (negative when rows enter scrollback)
    /// so the selection stays anchored to its content.
    pub fn shift_lines(&mut self, delta: isize) {
        for p in [
            &mut self.anchor.0,
            &mut self.anchor.1,
            &mut self.extent.0,
            &mut self.extent.1,
        ] {
            p.line += delta;
        }
    }
}

/// Clamp a point onto addressable content (scrollback top to grid bottom).
pub fn clamp_point(grid: &Grid, p: Point) -> Point {
    let min_line = -(grid.scrollback().len() as isize);
    let max_line = grid.rows() as isize - 1;
    Point::new(p.line.clamp(min_line, max_line), p.col.min(grid.cols() - 1))
}

/// Expand `p` to the word around it: a run of word characters
/// (alphanumeric or in `extra`), crossing soft-wrap boundaries. A
/// non-word starting cell expands to just itself.
pub fn expand_word(grid: &Grid, p: Point, extra: &[char]) -> (Point, Point) {
    let p = clamp_point(grid, p);
    if !is_word(grid, p, extra) {
        return (p, p);
    }
    let mut start = p;
    while let Some(q) = prev_point(grid, start) {
        if !is_word(grid, q, extra) {
            break;
        }
        start = q;
    }
    let mut end = p;
    while let Some(q) = next_point(grid, end) {
        if !is_word(grid, q, extra) {
            break;
        }
        end = q;
    }
    (start, end)
}

/// Expand `p` to the full logical line: walk up while the row above
/// soft-wraps into this one, down while this row soft-wraps onward.
pub fn expand_line(grid: &Grid, p: Point) -> (Point, Point) {
    let p = clamp_point(grid, p);
    let mut top = p.line;
    while let Some(above) = grid.absolute_row(top - 1) {
        if !above.wrapped {
            break;
        }
        top -= 1;
    }
    let mut bottom = p.line;
    while grid.absolute_row(bottom).is_some_and(|r| r.wrapped)
        && grid.absolute_row(bottom + 1).is_some()
    {
        bottom += 1;
    }
    (Point::new(top, 0), Point::new(bottom, grid.cols() - 1))
}

/// Selected text. Rows are joined without a newline at soft wraps
/// (per the row wrap flag) and with `\n` at hard breaks; each hard line
/// (including the final segment) is right-trimmed; wide-char spacer
/// cells are skipped.
pub fn text(grid: &Grid, sel: &Selection) -> String {
    let (start, end) = (sel.start(), sel.end());
    let mut out = String::new();
    let mut logical = String::new();
    for line in start.line..=end.line {
        let Some(row) = grid.absolute_row(line) else {
            continue;
        };
        let first = if line == start.line { start.col } else { 0 };
        let last = if line == end.line {
            end.col.min(row.len() - 1)
        } else {
            row.len() - 1
        };
        for c in first..=last {
            let cell = &row.cells[c];
            if !cell.is_wide_spacer() {
                logical.push(cell.ch);
            }
        }
        if row.wrapped && line != end.line {
            continue; // soft wrap: the logical line continues
        }
        logical.truncate(logical.trim_end().len());
        out.push_str(&logical);
        logical.clear();
        if line != end.line {
            out.push('\n');
        }
    }
    out
}

fn is_word(grid: &Grid, p: Point, extra: &[char]) -> bool {
    let Some(row) = grid.absolute_row(p.line) else {
        return false;
    };
    let Some(cell) = row.cells.get(p.col) else {
        return false;
    };
    // A wide spacer belongs to the wide char before it.
    let cell = if cell.is_wide_spacer() && p.col > 0 {
        &row.cells[p.col - 1]
    } else {
        cell
    };
    cell.ch.is_alphanumeric() || extra.contains(&cell.ch)
}

/// Reading-order predecessor, crossing a soft wrap from the row above.
fn prev_point(grid: &Grid, p: Point) -> Option<Point> {
    if p.col > 0 {
        return Some(Point::new(p.line, p.col - 1));
    }
    let above = grid.absolute_row(p.line - 1)?;
    above
        .wrapped
        .then(|| Point::new(p.line - 1, grid.cols() - 1))
}

/// Reading-order successor, crossing a soft wrap onto the row below.
fn next_point(grid: &Grid, p: Point) -> Option<Point> {
    if p.col + 1 < grid.cols() {
        return Some(Point::new(p.line, p.col + 1));
    }
    if !grid.absolute_row(p.line)?.wrapped {
        return None;
    }
    grid.absolute_row(p.line + 1)
        .map(|_| Point::new(p.line + 1, 0))
}

#[cfg(test)]
#[path = "../tests/selection.rs"]
mod tests;
