use super::*;

impl TerminalView {
    /// Any write to the pty snaps the view back to the live bottom.
    pub(crate) fn scroll_to_bottom(&self, cx: &mut Context<Self>) {
        let was_back = self.session.with_term(|term| {
            let back = term.display_offset() != 0;
            term.set_display_offset(0);
            back
        });
        if was_back {
            cx.notify();
        }
    }

    /// Scroll the viewport by `delta` rows into (positive) or out of
    /// (negative) scrollback history.
    pub fn scroll_lines(&mut self, delta: isize, cx: &mut Context<Self>) {
        let moved = self.session.with_term(|term| {
            let before = term.display_offset();
            term.scroll_display(delta);
            term.display_offset() != before
        });
        if moved {
            cx.notify();
        }
    }

    /// Scroll by whole pages (the pane's row count), sign as in
    /// [`Self::scroll_lines`].
    pub fn scroll_pages(&mut self, pages: isize, cx: &mut Context<Self>) {
        let rows = self.session.with_term(|term| term.rows()) as isize;
        self.scroll_lines(pages * rows.max(1), cx);
    }

    /// Jump to the oldest scrollback line.
    pub fn scroll_to_top(&mut self, cx: &mut Context<Self>) {
        let moved = self.session.with_term(|term| {
            let max = term.grid().scrollback().len();
            let before = term.display_offset();
            term.set_display_offset(max);
            term.display_offset() != before
        });
        if moved {
            cx.notify();
        }
    }

    /// Jump to the live bottom (alias of the input scroll-to-bottom path).
    pub fn scroll_to_live(&mut self, cx: &mut Context<Self>) {
        self.scroll_to_bottom(cx);
    }

    /// Move the viewport by `delta` shell prompts (OSC 133;A marks);
    /// negative scrolls toward older prompts.
    pub fn jump_prompt(&mut self, delta: i32, cx: &mut Context<Self>) {
        if delta == 0 {
            return;
        }
        let moved = self.session.with_term(|term| {
            let prompts = term.prompt_lines();
            if prompts.is_empty() {
                return false;
            }
            let sb = term.grid().scrollback().len();
            let mut top = sb - term.display_offset().min(sb);
            let mut changed = false;
            for _ in 0..delta.unsigned_abs() {
                let next = if delta < 0 {
                    prompts.iter().rev().find(|&&p| p < top).copied()
                } else {
                    prompts.iter().find(|&&p| p > top).copied()
                };
                match next {
                    Some(p) => {
                        top = p;
                        changed = true;
                    }
                    None => break,
                }
            }
            if changed {
                term.set_display_offset(sb.saturating_sub(top));
            }
            changed
        });
        if moved {
            cx.notify();
        }
    }

    /// Clear the visible screen the way most terminals' "clear" does: send
    /// a form feed so the shell redraws its prompt at the top.
    pub fn clear_screen(&mut self, cx: &mut Context<Self>) {
        self.scroll_to_bottom(cx);
        let _ = self.session.write(b"\x0c");
    }
}
