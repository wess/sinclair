//! State operations shared by the dispatch handlers.

use std::io::Write;

use unicode_width::UnicodeWidthChar;

use crate::cell::{Cell, CellFlags};
use crate::charset::Charsets;
use crate::cursor::SavedCursor;
use crate::mode::Modes;
use crate::screen::Screen;

use super::Inner;

impl Inner {
    // --- printing ---------------------------------------------------------

    /// Write one already-charset-mapped character at the cursor.
    pub(crate) fn write_char(&mut self, c: char) {
        let width = c.width().unwrap_or(0);
        if width == 0 {
            // TODO: combining characters should attach to the previous cell.
            return;
        }
        self.last_printed = Some(c);
        let autowrap = self.modes.contains(Modes::AUTOWRAP);
        let insert = self.modes.contains(Modes::INSERT);
        let cols = self.screen().grid.cols();

        if self.screen().cursor.pending_wrap && autowrap {
            self.wrap_line();
        }
        self.screen_mut().cursor.pending_wrap = false;

        // A wide char that cannot fit before the right margin.
        if width == 2 && self.screen().cursor.col + 1 >= cols {
            if autowrap {
                self.wrap_line();
            } else {
                self.screen_mut().cursor.col = cols.saturating_sub(2);
            }
        }

        if insert {
            self.insert_blank(width);
        }

        // Printing into a selected row invalidates the selection.
        let row = self.screen().cursor.row;
        self.selection_clear_row(row);

        let scr = self.screen_mut();
        let (row, col) = (scr.cursor.row, scr.cursor.col);
        let pen = scr.cursor.pen;

        clear_wide_at(scr, row, col);
        if width == 2 && col + 1 < cols {
            clear_wide_at(scr, row, col + 1);
        }

        let mut cell = pen;
        cell.ch = c;
        cell.flags.remove(CellFlags::WIDE | CellFlags::WIDE_SPACER);
        if width == 2 {
            cell.flags.insert(CellFlags::WIDE);
        }
        *scr.grid.cell_mut(row, col) = cell;
        if width == 2 && col + 1 < cols {
            let mut spacer = pen;
            spacer.ch = ' ';
            spacer.flags.remove(CellFlags::WIDE);
            spacer.flags.insert(CellFlags::WIDE_SPACER);
            *scr.grid.cell_mut(row, col + 1) = spacer;
        }

        let next = col + width;
        if next >= cols {
            // The last cell was overwritten: any previous soft-wrap
            // continuation is stale. `wrap_line` re-sets the flag if the
            // line actually continues.
            scr.grid.row_mut(row).wrapped = false;
            scr.cursor.col = cols - 1;
            scr.cursor.pending_wrap = autowrap;
        } else {
            scr.cursor.col = next;
        }
    }

    /// REP: repeat the last printed character.
    pub(crate) fn repeat_last(&mut self, n: usize) {
        if let Some(c) = self.last_printed {
            for _ in 0..n {
                self.write_char(c);
            }
        }
    }

    fn wrap_line(&mut self) {
        let row = self.screen().cursor.row;
        self.screen_mut().grid.row_mut(row).wrapped = true;
        self.screen_mut().cursor.col = 0;
        self.linefeed();
    }

    // --- line movement / scrolling ----------------------------------------

    /// LF/VT/FF/IND: move down, scrolling at the bottom margin.
    pub(crate) fn linefeed(&mut self) {
        let row = self.screen().cursor.row;
        if row == self.screen().scroll_bottom {
            self.scroll_up_region(1);
        } else if row + 1 < self.screen().grid.rows() {
            self.screen_mut().cursor.row += 1;
        }
        self.screen_mut().cursor.pending_wrap = false;
    }

    /// RI: move up, scrolling down at the top margin.
    pub(crate) fn reverse_index(&mut self) {
        let row = self.screen().cursor.row;
        if row == self.screen().scroll_top {
            self.scroll_down_region(1);
        } else if row > 0 {
            self.screen_mut().cursor.row -= 1;
        }
        self.screen_mut().cursor.pending_wrap = false;
    }

    /// Scroll the region up `n` lines, feeding scrollback when eligible.
    /// Keeps the display offset stable (the viewport must not shift while
    /// the user is scrolled back) and the selection content-anchored on
    /// whole-screen scrolls; any other scroll clears the selection.
    pub(crate) fn scroll_up_region(&mut self, n: usize) {
        let alt = self.modes.contains(Modes::ALT_SCREEN);
        let scr = self.screen_mut();
        let blank = Cell::erased(scr.cursor.pen);
        let (top, bottom) = (scr.scroll_top, scr.scroll_bottom);
        let rows = scr.grid.rows();
        let save = !alt && top == 0;
        let pushed = n.min(bottom - top + 1);
        scr.grid.scroll_up(top, bottom, n, save, blank);
        if pushed == 0 {
            return;
        }
        if save {
            let len = self.screen().grid.scrollback().len();
            if self.display_offset > 0 {
                self.display_offset = (self.display_offset + pushed).min(len);
            }
            if bottom == rows - 1 {
                self.selection_shift_up(pushed, len);
                return;
            }
        }
        self.selection_clear_all();
    }

    pub(crate) fn scroll_down_region(&mut self, n: usize) {
        let scr = self.screen_mut();
        let blank = Cell::erased(scr.cursor.pen);
        let (top, bottom) = (scr.scroll_top, scr.scroll_bottom);
        scr.grid.scroll_down(top, bottom, n, blank);
        if n > 0 {
            self.selection_clear_all();
        }
    }

    // --- cursor movement ---------------------------------------------------

    pub(crate) fn cursor_up(&mut self, n: usize) {
        let scr = self.screen_mut();
        let top = if scr.cursor.row >= scr.scroll_top {
            scr.scroll_top
        } else {
            0
        };
        scr.cursor.row = scr.cursor.row.saturating_sub(n).max(top);
        scr.cursor.pending_wrap = false;
    }

    pub(crate) fn cursor_down(&mut self, n: usize) {
        let scr = self.screen_mut();
        let bottom = if scr.cursor.row <= scr.scroll_bottom {
            scr.scroll_bottom
        } else {
            scr.grid.rows() - 1
        };
        scr.cursor.row = (scr.cursor.row + n).min(bottom);
        scr.cursor.pending_wrap = false;
    }

    pub(crate) fn cursor_left(&mut self, n: usize) {
        let scr = self.screen_mut();
        scr.cursor.col = scr.cursor.col.saturating_sub(n);
        scr.cursor.pending_wrap = false;
    }

    pub(crate) fn cursor_right(&mut self, n: usize) {
        let scr = self.screen_mut();
        scr.cursor.col = (scr.cursor.col + n).min(scr.grid.cols() - 1);
        scr.cursor.pending_wrap = false;
    }

    pub(crate) fn carriage_return(&mut self) {
        let scr = self.screen_mut();
        scr.cursor.col = 0;
        scr.cursor.pending_wrap = false;
    }

    /// CUP/HVP with 0-based coordinates, honoring origin mode.
    pub(crate) fn cursor_to(&mut self, row: usize, col: usize) {
        let origin = self.modes.contains(Modes::ORIGIN);
        let scr = self.screen_mut();
        let (top, bottom) = if origin {
            (scr.scroll_top, scr.scroll_bottom)
        } else {
            (0, scr.grid.rows() - 1)
        };
        scr.cursor.row = (top + row).clamp(top, bottom);
        scr.cursor.col = col.min(scr.grid.cols() - 1);
        scr.cursor.pending_wrap = false;
    }

    pub(crate) fn set_column(&mut self, col: usize) {
        let scr = self.screen_mut();
        scr.cursor.col = col.min(scr.grid.cols() - 1);
        scr.cursor.pending_wrap = false;
    }

    /// VPA: absolute row, origin-aware, column unchanged.
    pub(crate) fn set_row(&mut self, row: usize) {
        let origin = self.modes.contains(Modes::ORIGIN);
        let scr = self.screen_mut();
        let (top, bottom) = if origin {
            (scr.scroll_top, scr.scroll_bottom)
        } else {
            (0, scr.grid.rows() - 1)
        };
        scr.cursor.row = (top + row).clamp(top, bottom);
        scr.cursor.pending_wrap = false;
    }

    // --- tabs ---------------------------------------------------------------

    pub(crate) fn tab_forward(&mut self, n: usize) {
        let scr = self.screen_mut();
        for _ in 0..n {
            scr.cursor.col = scr.next_tab(scr.cursor.col);
        }
    }

    pub(crate) fn tab_backward(&mut self, n: usize) {
        let scr = self.screen_mut();
        for _ in 0..n {
            scr.cursor.col = scr.prev_tab(scr.cursor.col);
        }
    }

    // --- erase / edit --------------------------------------------------------

    /// ED: erase in display (0 below, 1 above, 2 all, 3 scrollback).
    /// Clears the selection when the erased band intersects it; ED 3 also
    /// drops the display offset (the history it pointed into is gone).
    pub(crate) fn erase_display(&mut self, mode: u16) {
        let rows = self.screen().grid.rows() as isize;
        let crow = self.screen().cursor.row as isize;
        match mode {
            0 => self.selection_clear_range(crow, rows - 1),
            1 => self.selection_clear_range(0, crow),
            2 => self.selection_clear_range(0, rows - 1),
            3 => {
                self.selection_clear_range(isize::MIN, -1);
                self.display_offset = 0;
            }
            _ => {}
        }
        let scr = self.screen_mut();
        let blank = Cell::erased(scr.cursor.pen);
        let (rows, cols) = (scr.grid.rows(), scr.grid.cols());
        let (crow, ccol) = (scr.cursor.row, scr.cursor.col);
        match mode {
            0 => {
                for c in ccol..cols {
                    *scr.grid.cell_mut(crow, c) = blank;
                }
                // Blanked to the end: the row no longer continues.
                scr.grid.row_mut(crow).wrapped = false;
                for r in crow + 1..rows {
                    scr.grid.row_mut(r).fill(blank);
                }
            }
            1 => {
                for r in 0..crow {
                    scr.grid.row_mut(r).fill(blank);
                }
                for c in 0..=ccol {
                    *scr.grid.cell_mut(crow, c) = blank;
                }
            }
            2 => {
                for r in 0..rows {
                    scr.grid.row_mut(r).fill(blank);
                }
                scr.grid.damage_full();
            }
            // clear_scrollback escalates to full damage itself.
            3 => scr.grid.clear_scrollback(),
            _ => {}
        }
    }

    /// EL: erase in line (0 right, 1 left, 2 all). Erasing through the
    /// last column breaks any soft-wrap continuation.
    pub(crate) fn erase_line(&mut self, mode: u16) {
        let crow = self.screen().cursor.row;
        self.selection_clear_row(crow);
        let scr = self.screen_mut();
        let blank = Cell::erased(scr.cursor.pen);
        let cols = scr.grid.cols();
        let ccol = scr.cursor.col;
        let range = match mode {
            0 => ccol..cols,
            1 => 0..ccol + 1,
            2 => 0..cols,
            _ => return,
        };
        for c in range.clone() {
            *scr.grid.cell_mut(crow, c) = blank;
        }
        if range.end == cols {
            scr.grid.row_mut(crow).wrapped = false;
        }
    }

    /// ICH and insert-mode shifting: insert `n` blanks at the cursor.
    /// Content is pushed off the right edge, so the continuation breaks.
    pub(crate) fn insert_blank(&mut self, n: usize) {
        let crow = self.screen().cursor.row;
        self.selection_clear_row(crow);
        let scr = self.screen_mut();
        let blank = Cell::erased(scr.cursor.pen);
        let cols = scr.grid.cols();
        let ccol = scr.cursor.col;
        let n = n.min(cols - ccol);
        if n == 0 {
            return;
        }
        let row = scr.grid.row_mut(crow);
        row.cells[ccol..].rotate_right(n);
        for c in ccol..ccol + n {
            row.cells[c] = blank;
        }
        row.wrapped = false;
    }

    /// DCH: delete `n` cells at the cursor, shifting the rest left. The
    /// tail blanks out, so the continuation breaks.
    pub(crate) fn delete_chars(&mut self, n: usize) {
        let crow = self.screen().cursor.row;
        self.selection_clear_row(crow);
        let scr = self.screen_mut();
        let blank = Cell::erased(scr.cursor.pen);
        let cols = scr.grid.cols();
        let ccol = scr.cursor.col;
        let n = n.min(cols - ccol);
        if n == 0 {
            return;
        }
        let row = scr.grid.row_mut(crow);
        row.cells[ccol..].rotate_left(n);
        for c in cols - n..cols {
            row.cells[c] = blank;
        }
        row.wrapped = false;
    }

    /// ECH: blank `n` cells from the cursor without shifting; reaching the
    /// last column breaks the continuation.
    pub(crate) fn erase_chars(&mut self, n: usize) {
        let crow = self.screen().cursor.row;
        self.selection_clear_row(crow);
        let scr = self.screen_mut();
        let blank = Cell::erased(scr.cursor.pen);
        let cols = scr.grid.cols();
        let ccol = scr.cursor.col;
        let end = (ccol + n).min(cols);
        for c in ccol..end {
            *scr.grid.cell_mut(crow, c) = blank;
        }
        if end == cols && n > 0 {
            scr.grid.row_mut(crow).wrapped = false;
        }
    }

    /// IL: insert `n` lines at the cursor (inside the scroll region only).
    pub(crate) fn insert_lines(&mut self, n: usize) {
        let scr = self.screen_mut();
        let row = scr.cursor.row;
        if row < scr.scroll_top || row > scr.scroll_bottom {
            return;
        }
        let blank = Cell::erased(scr.cursor.pen);
        let bottom = scr.scroll_bottom;
        scr.grid.scroll_down(row, bottom, n, blank);
        scr.cursor.col = 0;
        scr.cursor.pending_wrap = false;
        if n > 0 {
            self.selection_clear_all();
        }
    }

    /// DL: delete `n` lines at the cursor (inside the scroll region only).
    pub(crate) fn delete_lines(&mut self, n: usize) {
        let scr = self.screen_mut();
        let row = scr.cursor.row;
        if row < scr.scroll_top || row > scr.scroll_bottom {
            return;
        }
        let blank = Cell::erased(scr.cursor.pen);
        let bottom = scr.scroll_bottom;
        scr.grid.scroll_up(row, bottom, n, false, blank);
        scr.cursor.col = 0;
        scr.cursor.pending_wrap = false;
        if n > 0 {
            self.selection_clear_all();
        }
    }

    // --- saved cursor / regions / reset ---------------------------------------

    /// DECSC.
    pub(crate) fn save_cursor(&mut self) {
        let charsets = self.charsets;
        let origin = self.modes.contains(Modes::ORIGIN);
        let scr = self.screen_mut();
        scr.saved = Some(SavedCursor {
            row: scr.cursor.row,
            col: scr.cursor.col,
            pen: scr.cursor.pen,
            charsets,
            origin,
            pending_wrap: scr.cursor.pending_wrap,
        });
    }

    /// DECRC; defaults to home/defaults when nothing was saved.
    pub(crate) fn restore_cursor(&mut self) {
        let saved = self.screen().saved.unwrap_or_default();
        self.charsets = saved.charsets;
        self.modes.set(Modes::ORIGIN, saved.origin);
        let scr = self.screen_mut();
        scr.cursor.row = saved.row.min(scr.grid.rows() - 1);
        scr.cursor.col = saved.col.min(scr.grid.cols() - 1);
        scr.cursor.pen = saved.pen;
        scr.cursor.pending_wrap = saved.pending_wrap;
    }

    /// DECSTBM with 1-based arguments (0 = default).
    pub(crate) fn set_scroll_region(&mut self, top: u16, bottom: u16) {
        let rows = self.screen().grid.rows();
        let top = (top.max(1) as usize - 1).min(rows - 1);
        let bottom = if bottom == 0 {
            rows - 1
        } else {
            (bottom as usize - 1).min(rows - 1)
        };
        if top >= bottom {
            return;
        }
        let scr = self.screen_mut();
        scr.scroll_top = top;
        scr.scroll_bottom = bottom;
        self.cursor_to(0, 0);
    }

    /// RIS: full reset. Keeps the window title and any unread output.
    pub(crate) fn full_reset(&mut self) {
        let cols = self.primary.grid.cols();
        let rows = self.primary.grid.rows();
        self.primary = Screen::new(cols, rows, self.scrollback_limit);
        self.alt = Screen::new(cols, rows, 0);
        self.modes = Modes::default();
        self.charsets = Charsets::default();
        self.cursor_style = Default::default();
        self.palette = [None; 256];
        self.cursor_color = None;
        self.last_printed = None;
        self.display_offset = 0;
        self.selection = None;
        self.hyperlinks.clear();
        self.dcs = super::dcs::Dcs::None;
        self.full_damage = true;
    }

    /// DECALN: fill the screen with `E`, reset margins, home the cursor.
    pub(crate) fn screen_alignment_test(&mut self) {
        let scr = self.screen_mut();
        let mut cell = Cell::default();
        cell.ch = 'E';
        let rows = scr.grid.rows();
        for r in 0..rows {
            scr.grid.row_mut(r).fill(cell);
        }
        scr.scroll_top = 0;
        scr.scroll_bottom = rows - 1;
        scr.cursor.row = 0;
        scr.cursor.col = 0;
        scr.cursor.pending_wrap = false;
    }

    // --- alternate screen ------------------------------------------------------

    /// Entering the alternate screen resets the display offset (alt has no
    /// scrollback) and drops the selection (it pointed at primary content).
    pub(crate) fn enter_alt(&mut self, clear: bool) {
        if self.modes.contains(Modes::ALT_SCREEN) {
            return;
        }
        self.alt.cursor = self.primary.cursor;
        if clear {
            let blank = Cell::erased(self.alt.cursor.pen);
            let rows = self.alt.grid.rows();
            for r in 0..rows {
                self.alt.grid.row_mut(r).fill(blank);
            }
        }
        self.modes.insert(Modes::ALT_SCREEN);
        self.display_offset = 0;
        self.selection = None;
        self.full_damage = true;
    }

    pub(crate) fn exit_alt(&mut self) {
        if self.modes.contains(Modes::ALT_SCREEN) {
            self.selection = None;
            self.full_damage = true;
        }
        self.modes.remove(Modes::ALT_SCREEN);
    }

    // --- responses ----------------------------------------------------------------

    /// DSR 6: cursor position report, origin-mode adjusted.
    pub(crate) fn report_cursor(&mut self) {
        let origin = self.modes.contains(Modes::ORIGIN);
        let scr = self.screen();
        let row = if origin {
            scr.cursor.row - scr.scroll_top
        } else {
            scr.cursor.row
        };
        let col = scr.cursor.col;
        let _ = write!(self.output, "\x1b[{};{}R", row + 1, col + 1);
    }
}

/// If the cell at (row, col) is half of a wide pair, blank its other half
/// so no orphaned spacer or head survives an overwrite.
fn clear_wide_at(scr: &mut Screen, row: usize, col: usize) {
    let cols = scr.grid.cols();
    let flags = scr.grid.cell(row, col).flags;
    if flags.contains(CellFlags::WIDE) && col + 1 < cols {
        let spacer = scr.grid.cell_mut(row, col + 1);
        if spacer.is_wide_spacer() {
            *spacer = Cell::default();
        }
    } else if flags.contains(CellFlags::WIDE_SPACER) && col > 0 {
        let head = scr.grid.cell_mut(row, col - 1);
        if head.is_wide() {
            *head = Cell::default();
        }
    }
}

#[cfg(test)]
#[path = "../../tests/term/ops.rs"]
mod tests;
