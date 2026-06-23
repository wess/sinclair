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
use crate::selection::{self, Point, Selection, SelectionMode};

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
