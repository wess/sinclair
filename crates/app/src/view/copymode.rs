//! Copy mode: a vi-style keyboard cursor over the viewport and scrollback for
//! selecting text without the mouse. `hjkl`/arrows move, `w`/`b` step a column,
//! `0`/`$` line ends, `g`/`G` top/bottom, `v` toggles a selection, `y`/Enter
//! yanks it (redacted) and exits, `Esc` exits. The cursor lives in absolute-row
//! space (same as selection points); moving off-screen scrolls the view.

use gpui::{div, px, AnyElement, ClipboardItem, Context, IntoElement, Styled};

use super::TerminalView;
use crate::colors;

/// Active copy-mode cursor.
pub(crate) struct CopyMode {
    line: isize,
    col: usize,
    selecting: bool,
}

impl TerminalView {
    /// Enter copy mode at the terminal cursor, or exit if already in it.
    pub(crate) fn toggle_copy_mode(&mut self, cx: &mut Context<Self>) {
        if self.copy_mode.is_some() {
            self.exit_copy_mode(cx);
            return;
        }
        let (line, col) = self.session.with_term(|t| {
            let (vp_row, col) = t.cursor_pos();
            let base = t.grid().scrollback().len().saturating_sub(t.display_offset());
            ((base + vp_row) as isize, col)
        });
        self.copy_mode = Some(CopyMode {
            line,
            col,
            selecting: false,
        });
        cx.notify();
    }

    fn exit_copy_mode(&mut self, cx: &mut Context<Self>) {
        self.copy_mode = None;
        self.session.with_term(|t| t.clear_selection());
        cx.notify();
    }

    pub(crate) fn copy_mode_active(&self) -> bool {
        self.copy_mode.is_some()
    }

    fn copy_yank(&mut self, cx: &mut Context<Self>) {
        let text = self.session.with_term(|t| t.selection_text());
        if let Some(text) = text.filter(|s| !s.is_empty()) {
            let text = crate::redact::mask(text, cx);
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
        self.exit_copy_mode(cx);
    }

    /// Handle a keystroke while copy mode is open.
    pub(crate) fn copy_mode_key(&mut self, ks: &gpui::Keystroke, cx: &mut Context<Self>) {
        let Some(cm) = self.copy_mode.as_ref() else {
            return;
        };
        let (mut line, mut col, mut selecting) = (cm.line, cm.col, cm.selecting);
        let key = ks.key.as_str();
        let ch = ks.key_char.as_deref().unwrap_or("");
        let (rows, cols, sb) =
            self.session.with_term(|t| (t.rows(), t.cols(), t.grid().scrollback().len()));
        let max_line = (sb + rows).saturating_sub(1) as isize;
        let last_col = cols.saturating_sub(1);

        match (key, ch) {
            ("escape", _) => {
                self.exit_copy_mode(cx);
                return;
            }
            ("enter", _) | (_, "y") => {
                self.copy_yank(cx);
                return;
            }
            ("left", _) | (_, "h") | (_, "b") => col = col.saturating_sub(1),
            ("right", _) | (_, "l") | (_, "w") => col = (col + 1).min(last_col),
            ("up", _) | (_, "k") => line = (line - 1).max(0),
            ("down", _) | (_, "j") => line = (line + 1).min(max_line),
            (_, "0") => col = 0,
            (_, "$") => col = last_col,
            (_, "g") => line = 0,
            (_, "G") => line = max_line,
            (_, "v") => selecting = !selecting,
            _ => return,
        }

        if let Some(cm) = &mut self.copy_mode {
            cm.line = line;
            cm.col = col;
            cm.selecting = selecting;
        }

        self.session.with_term(|t| {
            let point = vt::Point::new(line, col);
            if selecting {
                if t.selection().is_none() {
                    t.start_selection(vt::SelectionMode::Cell, point);
                } else {
                    t.update_selection(point);
                }
            } else {
                t.clear_selection();
            }
            // Scroll so the cursor line stays on screen.
            let sb = t.grid().scrollback().len();
            let base = sb.saturating_sub(t.display_offset()) as isize;
            let vr = line - base;
            if vr < 0 {
                let off = (t.display_offset() + (-vr) as usize).min(sb);
                t.set_display_offset(off);
            } else if vr >= t.rows() as isize {
                let over = (vr - t.rows() as isize + 1) as usize;
                let off = t.display_offset().saturating_sub(over);
                t.set_display_offset(off);
            }
        });
        cx.notify();
    }

    /// A block outline over the copy-mode cursor, when it's on screen.
    pub(crate) fn copy_cursor_overlay(&self) -> Option<AnyElement> {
        let cm = self.copy_mode.as_ref()?;
        let (base, rows) = self.session.with_term(|t| {
            (
                t.grid().scrollback().len().saturating_sub(t.display_offset()),
                t.rows(),
            )
        });
        let vr = cm.line - base as isize;
        if vr < 0 || vr >= rows as isize {
            return None;
        }
        let x = self.pad.x + cm.col as f32 * self.cell.width;
        let y = self.pad.y + vr as f32 * self.cell.height;
        Some(
            div()
                .absolute()
                .left(px(x))
                .top(px(y))
                .w(px(self.cell.width))
                .h(px(self.cell.height))
                .border_2()
                .border_color(colors::rgba(self.colors.cursor))
                .into_any_element(),
        )
    }
}
