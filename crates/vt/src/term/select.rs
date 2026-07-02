//! Terminal-level selection API plus the hooks the feed path uses to keep
//! the selection honest.
//!
//! Damage policy: any selection change (start, update, clear) escalates to
//! full damage. Selections change at human speed; per-row precision is not
//! worth the bookkeeping.
//!
//! Clearing policy (conservative, on purpose):
//! - printing into a row the selection covers clears it;
//! - a whole-screen scroll into history shifts the selection so it stays
//!   anchored to content, clearing it once its start falls off the
//!   scrollback ring;
//! - any other scroll (partial region, scroll-down, IL/DL, alt screen)
//!   clears it outright;
//! - erase-in-display clears it when the erased band intersects it;
//!   erase-in-line and char edits clear it only when the cursor row is
//!   inside it;
//! - alt-screen switches, resize, and RIS always clear it.

use crate::grid::Grid;
use crate::selection::{self, Point, Selection, SelectionAdjust, SelectionMode};

use super::{Inner, Terminal};

impl Terminal {
    /// Begin a selection at `point` (clamped to content), expanding it per
    /// `mode`. Replaces any existing selection.
    pub fn start_selection(&mut self, mode: SelectionMode, point: Point) {
        let span = expand(
            &self.inner.screen().grid,
            mode,
            point,
            &self.inner.word_chars,
        );
        self.inner.selection = Some(Selection::new(mode, span));
        self.inner.full_damage = true;
    }

    /// Move the selection's extent to `point`; no-op without a selection.
    pub fn update_selection(&mut self, point: Point) {
        let Some(mode) = self.inner.selection.as_ref().map(|s| s.mode()) else {
            return;
        };
        let span = expand(
            &self.inner.screen().grid,
            mode,
            point,
            &self.inner.word_chars,
        );
        if let Some(sel) = &mut self.inner.selection {
            sel.update(span);
            self.inner.full_damage = true;
        }
    }

    pub fn clear_selection(&mut self) {
        if self.inner.selection.take().is_some() {
            self.inner.full_damage = true;
        }
    }

    /// Nudge the selection's moving end one step in `dir`, keeping the
    /// anchor fixed, and scroll the view to keep the new end visible.
    /// Returns `false` (a no-op) when there is no selection, which is the
    /// host's cue to fall through to the key's normal escape sequence,
    /// the same "performable" semantics Ghostty uses for shift+navigation.
    ///
    /// The selection collapses to exact cells here (a fixed anchor and a
    /// moving caret): word/line expansion only shapes the initial mouse
    /// gesture, so keyboard motion stays symmetric — every extend has an
    /// equal-and-opposite retract.
    pub fn adjust_selection(&mut self, dir: SelectionAdjust) -> bool {
        let Some(sel) = self.inner.selection else {
            return false;
        };
        let (anchor, caret) = sel.caret_ends_for(dir);
        let page = self.rows();
        let point = selection::adjust_caret(
            &self.inner.screen().grid,
            anchor,
            caret,
            dir,
            page,
            &self.inner.word_chars,
        );
        self.inner.selection = Some(Selection::cell_pair(anchor, point));
        self.inner.full_damage = true;
        self.reveal_line(point.line);
        true
    }

    /// Extend the selection one step in `dir`, beginning one at the cursor
    /// first when none exists — so a keyboard combo can start a selection
    /// from the caret (the macOS convention for ⇧⌘ navigation), rather than
    /// no-op'ing like [`adjust_selection`].
    pub fn extend_selection(&mut self, dir: SelectionAdjust) {
        if self.inner.selection.is_none() {
            let (row, col) = self.cursor_pos();
            self.start_selection(SelectionMode::Cell, Point::new(row as isize, col));
        }
        self.adjust_selection(dir);
    }

    /// Scroll the view (display offset) the minimum amount needed for
    /// absolute `line` to fall inside the viewport.
    fn reveal_line(&mut self, line: isize) {
        let rows = self.rows() as isize;
        let offset = self.display_offset() as isize;
        // Visible content lines span [-offset, rows - 1 - offset].
        if line < -offset {
            self.set_display_offset((-line).max(0) as usize);
        } else if line > rows - 1 - offset {
            self.set_display_offset((rows - 1 - line).max(0) as usize);
        }
    }

    pub fn selection(&self) -> Option<&Selection> {
        self.inner.selection.as_ref()
    }

    /// Selected text: soft wraps join without a newline, hard lines are
    /// right-trimmed, wide-char spacers are skipped.
    pub fn selection_text(&self) -> Option<String> {
        let sel = self.inner.selection.as_ref()?;
        Some(selection::text(&self.inner.screen().grid, sel))
    }

    /// Extra characters word selection treats as word constituents on top
    /// of alphanumerics. Default: `/-_.~`.
    pub fn set_word_chars(&mut self, chars: &str) {
        self.inner.word_chars = chars.chars().collect();
    }
}

fn expand(grid: &Grid, mode: SelectionMode, point: Point, extra: &[char]) -> (Point, Point) {
    match mode {
        SelectionMode::Cell => {
            let p = selection::clamp_point(grid, point);
            (p, p)
        }
        SelectionMode::Word => selection::expand_word(grid, point, extra),
        SelectionMode::Line => selection::expand_line(grid, point),
        SelectionMode::Smart => selection::smart_span(grid, point, extra),
    }
}

impl Inner {
    /// Drop the selection unconditionally (conservative scroll/erase rule).
    pub(crate) fn selection_clear_all(&mut self) {
        if self.selection.take().is_some() {
            self.full_damage = true;
        }
    }

    /// Drop the selection when its line range intersects `first..=last`
    /// (absolute lines; live grid rows are `0..rows`).
    pub(crate) fn selection_clear_range(&mut self, first: isize, last: isize) {
        if let Some(sel) = &self.selection {
            if sel.start().line <= last && sel.end().line >= first {
                self.selection = None;
                self.full_damage = true;
            }
        }
    }

    /// Drop the selection when it covers live-grid row `row`.
    pub(crate) fn selection_clear_row(&mut self, row: usize) {
        self.selection_clear_range(row as isize, row as isize);
    }

    /// `pushed` rows entered scrollback from a whole-screen scroll: shift
    /// the selection up to stay content-anchored; invalidate it once its
    /// start has scrolled off the ring (`sb_len` is the post-push length).
    pub(crate) fn selection_shift_up(&mut self, pushed: usize, sb_len: usize) {
        if let Some(sel) = &mut self.selection {
            sel.shift_lines(-(pushed as isize));
            if sel.start().line < -(sb_len as isize) {
                self.selection = None;
                self.full_damage = true;
            }
        }
    }
}

#[cfg(test)]
#[path = "../../tests/term/select.rs"]
mod tests;
