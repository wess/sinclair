//! Serializing grid rows back into the bytes that would redraw them.
//!
//! Used to carry a pane's buffer across a restart: the host captures a dump
//! on quit and feeds it into a fresh terminal on restore, so the scrollback
//! comes back instead of the pane opening blank. What a dump reproduces is
//! the *text and its styling* — characters, colors, attributes — and nothing
//! that only makes sense while the session it came from was alive:
//! hyperlink targets, images, and the cursor's position are dropped.
//!
//! Line breaks are preserved for what they were: a row the user ended with a
//! newline is written with one, while a row that soft-wrapped into the next
//! runs straight on, so the emulator re-wraps it at whatever width it is
//! replayed into. A window that comes back wider than it went away therefore
//! shows its history as unbroken lines rather than at the old wrap points.

use crate::cell::Cell;
use crate::grid::row::Row;
use crate::sgr;

/// Append `row` to `out` as text plus the SGR sequences that style it. A row
/// that soft-wrapped into the next one is left unterminated so the two
/// rejoin; any other row ends with `\r\n`. Trailing cells that are unstyled
/// blanks are dropped, so an 80-column row holding one word costs one word;
/// a trailing run with a background color is kept, since that background is
/// visible.
///
/// The pen starts and ends at the default, so rows are independent of each
/// other and of whatever was on screen before the dump is replayed.
pub fn write_row(row: &Row, out: &mut String) {
  let end = row
    .cells
    .iter()
    .rposition(|c| !is_plain_blank(c))
    .map_or(0, |i| i + 1);
  let mut pen = Cell::default();
  for cell in row.cells[..end].iter().filter(|c| !c.is_wide_spacer()) {
    if !same_style(&pen, cell) {
      // Leave the old style before setting the new one: `emit` only adds
      // attributes, so a run that goes back to plain text needs the reset
      // to say so — without it the color runs on to the end of the dump.
      if !is_default_style(&pen) {
        out.push_str("\x1b[0m");
      }
      sgr::emit(cell, out);
      pen = *cell;
    }
    cell.write_grapheme(out);
  }
  if !is_default_style(&pen) {
    out.push_str("\x1b[0m");
  }
  if !row.wrapped {
    out.push_str("\r\n");
  }
}

/// Serialize `rows` in order, dropping trailing blank ones so a mostly empty
/// screen does not restore as a wall of newlines. Returns an empty string
/// when every row is blank.
pub fn write_rows<'a>(rows: impl IntoIterator<Item = &'a Row>) -> String {
  let mut rows: Vec<&Row> = rows.into_iter().collect();
  while rows
    .last()
    .is_some_and(|r| r.cells.iter().all(is_plain_blank))
  {
    rows.pop();
  }
  let mut out = String::new();
  for row in rows {
    write_row(row, &mut out);
  }
  out
}

/// A cell holding nothing anyone can see: a space with no styling. The
/// trailing run of these is what makes a row's remainder droppable.
fn is_plain_blank(cell: &Cell) -> bool {
  cell.ch == ' ' && cell.combining().next().is_none() && is_default_style(cell)
}

fn is_default_style(cell: &Cell) -> bool {
  let d = Cell::default();
  cell.fg == d.fg
    && cell.bg == d.bg
    && cell.underline_color == d.underline_color
    && style_flags(cell) == style_flags(&d)
}

fn same_style(a: &Cell, b: &Cell) -> bool {
  a.fg == b.fg
    && a.bg == b.bg
    && a.underline_color == b.underline_color
    && style_flags(a) == style_flags(b)
}

/// The flags that `emit` can actually write. WIDE/WIDE_SPACER describe the
/// cell's geometry, not its pen, and come back from the character's own
/// width when the dump is replayed.
fn style_flags(cell: &Cell) -> crate::cell::CellFlags {
  cell.flags - (crate::cell::CellFlags::WIDE | crate::cell::CellFlags::WIDE_SPACER)
}

#[cfg(test)]
#[path = "../tests/dump.rs"]
mod tests;
