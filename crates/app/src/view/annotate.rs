//! Line annotations: pin a short note to a line. Notes are keyed by a stable
//! line sequence (vt's monotonic `committed_lines` + row), so they scroll with
//! the content and survive eviction until the line falls off scrollback. Drawn
//! as a small pill in the left gutter of the annotated row.

use gpui::{div, px, AnyElement, IntoElement, ParentElement, SharedString, Styled};

use super::TerminalView;
use crate::colors;

impl TerminalView {
  /// Attach `note` to the current cursor line.
  pub(crate) fn annotate(&mut self, note: String, cx: &mut gpui::Context<Self>) {
    let note = note.trim().to_string();
    if note.is_empty() {
      return;
    }
    // The live cursor row's eventual scrollback sequence (offset is 0 live).
    let seq = self
      .session
      .with_term(|t| t.committed_lines() + t.cursor_pos().0 as u64);
    self.annotations.insert(seq, note);
    cx.notify();
  }

  /// Drop annotations whose rows have been evicted from scrollback; their
  /// sequences can never render again. Called from `update_line_times`.
  pub(crate) fn prune_annotations(&mut self, committed: u64, sb_len: usize) {
    if self.annotations.is_empty() {
      return;
    }
    let oldest = oldest_seq(committed, sb_len);
    self.annotations.retain(|&seq, _| seq >= oldest);
  }

  /// The annotation pills over their rows, when any are visible.
  pub(crate) fn annotations_overlay(&self) -> Option<AnyElement> {
    if self.annotations.is_empty() {
      return None;
    }
    let (committed, offset, rows) = self
      .session
      .with_term(|t| (t.committed_lines(), t.display_offset(), t.rows()));
    let mut layer = div().absolute().top_0().left_0().size_full();
    let mut any = false;
    for r in 0..rows {
      // Render row r shows the line with this sequence (see timestamps.rs).
      let seq = (committed + r as u64).saturating_sub(offset as u64);
      let Some(note) = self.annotations.get(&seq) else {
        continue;
      };
      any = true;
      let y = self.pad.y + r as f32 * self.cell.height;
      layer = layer.child(
        div()
          .absolute()
          .top(px(y))
          .left(px(self.pad.x + 2.0))
          .flex()
          .items_center()
          .gap_1()
          .child(
            div()
              .text_color(colors::rgba(self.colors.cursor))
              .text_size(px(self.cell.height * 0.62))
              .child(SharedString::from("\u{25c6}")),
          )
          .child(
            div()
              .px_1()
              .rounded(px(3.0))
              .bg(colors::rgba(self.colors.selection_bg))
              .text_color(colors::rgba(self.colors.selection_fg))
              .text_size(px(self.cell.height * 0.62))
              .child(SharedString::from(note.clone())),
          ),
      );
    }
    any.then(|| layer.into_any_element())
  }
}

/// The oldest line sequence still reachable: everything below has been
/// evicted from the scrollback ring.
pub(crate) fn oldest_seq(committed: u64, sb_len: usize) -> u64 {
  committed.saturating_sub(sb_len as u64)
}

#[cfg(test)]
#[path = "../../tests/annotate.rs"]
mod tests;
