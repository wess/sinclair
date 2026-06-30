//! Selection geometry: pure grid logic, no terminal state.
//!
//! # Coordinates
//!
//! A [`Point`] is `(line, col)` where `line: isize` is absolute to content:
//! line 0 is the top row of the live grid, positive lines go down the live
//! grid, and negative lines reach into scrollback (-1 is the newest history
//! row, `-scrollback_len` the oldest) - the same scheme as
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

/// One step of keyboard selection adjustment, moving the selection's
/// extent (its moving end). Mirrors the directions a shift+navigation key
/// would move a text caret.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionAdjust {
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    /// Extend to the start of the previous word (⌘⇧← on macOS).
    WordLeft,
    /// Extend to the end of the next word (⌘⇧→ on macOS).
    WordRight,
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

    /// The moving end of the selection: the point keyboard adjustment
    /// nudges, leaving the anchor fixed. For cell mode the extent is a
    /// single point; for word/line modes this is the far edge of the
    /// expanded extent span.
    pub fn extent_caret(&self) -> Point {
        self.extent.1
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

/// One keyboard step of the selection caret in `dir`, clamped to content.
/// `page` is the viewport height in rows (for `PageUp`/`PageDown`); `extra`
/// is the word-character set (for `WordLeft`/`WordRight`). Left and Right
/// wrap across row edges; the rest move within their column or to the row
/// ends; the word motions jump to the next/previous word boundary.
pub fn adjust_point(
    grid: &Grid,
    caret: Point,
    dir: SelectionAdjust,
    page: usize,
    extra: &[char],
) -> Point {
    let cols = grid.cols().max(1);
    let page = page.max(1) as isize;
    let moved = match dir {
        SelectionAdjust::Left => {
            if caret.col > 0 {
                Point::new(caret.line, caret.col - 1)
            } else {
                Point::new(caret.line - 1, cols - 1)
            }
        }
        SelectionAdjust::Right => {
            if caret.col + 1 < cols {
                Point::new(caret.line, caret.col + 1)
            } else {
                Point::new(caret.line + 1, 0)
            }
        }
        SelectionAdjust::Up => Point::new(caret.line - 1, caret.col),
        SelectionAdjust::Down => Point::new(caret.line + 1, caret.col),
        SelectionAdjust::Home => Point::new(caret.line, 0),
        SelectionAdjust::End => Point::new(caret.line, cols - 1),
        SelectionAdjust::PageUp => Point::new(caret.line - page, caret.col),
        SelectionAdjust::PageDown => Point::new(caret.line + page, caret.col),
        SelectionAdjust::WordLeft => return word_step(grid, caret, false, extra),
        SelectionAdjust::WordRight => return word_step(grid, caret, true, extra),
    };
    clamp_point(grid, moved)
}

/// Move `caret` one word in `right`'s direction: skip any non-word cells,
/// then ride to the far edge of the word (its end going right, its start
/// going left). Crosses row boundaries within existing content, matching
/// the per-cell Left/Right wrap, so word selection flows across lines.
/// Returns `caret` unchanged at the content edge.
fn word_step(grid: &Grid, caret: Point, right: bool, extra: &[char]) -> Point {
    let cols = grid.cols().max(1);
    let step = |p: Point| -> Option<Point> {
        if right {
            if p.col + 1 < cols {
                Some(Point::new(p.line, p.col + 1))
            } else {
                grid.absolute_row(p.line + 1).map(|_| Point::new(p.line + 1, 0))
            }
        } else if p.col > 0 {
            Some(Point::new(p.line, p.col - 1))
        } else {
            grid.absolute_row(p.line - 1)
                .map(|_| Point::new(p.line - 1, cols - 1))
        }
    };
    let Some(mut q) = step(caret) else {
        return caret;
    };
    while !is_word(grid, q, extra) {
        match step(q) {
            Some(p) => q = p,
            None => return q,
        }
    }
    while let Some(p) = step(q) {
        if is_word(grid, p, extra) {
            q = p;
        } else {
            break;
        }
    }
    q
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
                cell.write_grapheme(&mut logical);
            }
        }
        if row.wrapped && line != end.line {
            continue;
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
