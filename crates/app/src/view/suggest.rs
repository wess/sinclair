//! Autosuggestion behavior for a terminal pane: recompute candidates from the
//! typed line, expose the ghost suffix for the element to draw, cycle/accept
//! completions, and (optionally) fetch an AI suggestion off-thread. Pure state
//! transitions live in [`crate::suggest`]; this wires them to the live vt.

use gpui::prelude::*;
use gpui::{div, px, AnyElement, Context, Keystroke, SharedString};

use crate::colors;
use super::TerminalView;

impl TerminalView {
    /// A floating completion popup anchored just below the cursor, when open.
    pub(crate) fn suggestion_popup_overlay(&self, _cx: &mut Context<Self>) -> Option<AnyElement> {
        let (cands, sel) = self.suggestion_popup()?;
        let (row, col) = self.session.with_term(|t| t.cursor_pos());
        let x = px(self.pad.x + col as f32 * self.cell.width);
        let y = px(self.pad.y + (row as f32 + 1.0) * self.cell.height);
        let fg = colors::hsla(self.colors.fg);
        let mut sel_bg = fg;
        sel_bg.a = 0.16;
        let mut border = fg;
        border.a = 0.25;
        let rows: Vec<AnyElement> = cands
            .iter()
            .take(8)
            .enumerate()
            .map(|(i, c)| {
                let text: String = c.chars().take(52).collect();
                let mut r = div().px(px(8.0)).py(px(2.0)).child(SharedString::from(text));
                if i == sel {
                    r = r.bg(sel_bg);
                }
                r.into_any_element()
            })
            .collect();
        Some(
            div()
                .absolute()
                .left(x)
                .top(y)
                .flex()
                .flex_col()
                .bg(colors::rgba(self.colors.bg))
                .text_color(fg)
                .text_size(px(12.0))
                .border_1()
                .border_color(border)
                .rounded(px(6.0))
                .shadow_md()
                .children(rows)
                .into_any_element(),
        )
    }

    /// Handle a key against the active suggestion. Returns whether it was
    /// consumed (so the caller swallows it instead of sending to the pty).
    pub(crate) fn handle_suggestion_key(
        &mut self,
        ks: &Keystroke,
        mods: input::Mods,
        cx: &mut Context<Self>,
    ) -> bool {
        let plain = !mods.ctrl && !mods.alt && !mods.cmd;
        if !plain {
            return false;
        }
        let key = ks.key.as_str();
        // Popup navigation takes priority while it's open.
        if self.popup_open() {
            match key {
                "up" => return self.move_popup(-1, cx),
                "down" => return self.move_popup(1, cx),
                "enter" | "return" => return self.accept_popup(cx),
                "escape" => return self.dismiss_suggestion(cx),
                _ => {}
            }
        }
        // Tab cycles inline completions instead of reaching the shell.
        if key == "tab" && !mods.shift && self.suggest_cfg.tab && self.cycle_suggestion(cx) {
            return true;
        }
        // Right / End at the end of the line accepts the ghost.
        if matches!(key, "right" | "end") && self.accept_suggestion(cx) {
            return true;
        }
        // Escape clears a lingering ghost (otherwise it reaches the program).
        if key == "escape" && self.suggest.ghost.is_some() {
            return self.dismiss_suggestion(cx);
        }
        false
    }

    /// Recompute suggestions from the current input line. Called after each
    /// wakeup and after we send input to the pty.
    pub(crate) fn recompute_suggestions(&mut self, cx: &mut Context<Self>) {
        let cfg = self.suggest_cfg;
        if !cfg.wants_local() && !cfg.ai {
            self.clear_suggestions();
            return;
        }
        // Read the typed line and whether the cursor sits at its end (only then
        // do we offer a completion, fish-style).
        let (input, at_end) = self.session.with_term(|t| {
            let input = t.current_input();
            let at_end = match (t.input_end(), Some(t.cursor_pos())) {
                (Some(end), Some(cur)) => end == cur,
                _ => false,
            };
            (input, at_end)
        });
        let Some(input) = input.filter(|s| !s.trim().is_empty()) else {
            self.clear_suggestions();
            return;
        };
        if !at_end {
            // Editing mid-line: keep candidates but don't draw a ghost.
            self.suggest.ghost = None;
            cx.notify();
            return;
        }
        // Only now is history needed: cloning it up front deep-copied the whole
        // shell history on every output wakeup, then dropped it in the common
        // (no active input line) case above.
        let history = self.session.with_term(|t| t.command_history());
        let cwd = self.cwd_path();
        let cands = crate::suggest::candidates(&cfg, &input, &history, cwd.as_deref());
        self.suggest.input = input.clone();
        self.suggest.candidates = cands;
        self.suggest.tab_idx = None;
        self.suggest.popup_sel = 0;
        self.suggest.popup_open = cfg.popup && !self.suggest.candidates.is_empty();
        self.suggest.ghost = if cfg.ghost {
            crate::suggest::ghost(&self.suggest.candidates, &input)
        } else {
            None
        };
        // Fall back to an AI suggestion when nothing local completes the line.
        if cfg.ai && self.suggest.ghost.is_none() && self.suggest.candidates.is_empty() {
            self.fetch_ai_suggestion(&input, cx);
        } else {
            self.suggest.ai_ghost = None;
        }
        cx.notify();
    }

    /// The ghost suffix to draw past the cursor right now, if any.
    pub(crate) fn suggestion_ghost(&self) -> Option<String> {
        if self.suggest.ghost.is_some() {
            return self.suggest.ghost.clone();
        }
        // AI ghost only if it's still for the current input line.
        if self.suggest.ai_for.as_deref() == Some(self.suggest.input.as_str()) {
            return self.suggest.ai_ghost.clone();
        }
        None
    }

    /// The open completion popup's candidates + selected index, for rendering.
    pub(crate) fn suggestion_popup(&self) -> Option<(&[String], usize)> {
        if self.suggest_cfg.popup && self.suggest.popup_open && !self.suggest.candidates.is_empty() {
            Some((&self.suggest.candidates, self.suggest.popup_sel))
        } else {
            None
        }
    }

    fn clear_suggestions(&mut self) {
        self.suggest.input.clear();
        self.suggest.candidates.clear();
        self.suggest.ghost = None;
        self.suggest.ai_ghost = None;
        self.suggest.ai_for = None;
        self.suggest.popup_open = false;
        self.suggest.tab_idx = None;
    }

    /// Accept the current ghost: send its bytes to the pty. Returns whether an
    /// acceptance happened (so the key handler can swallow the key).
    pub(crate) fn accept_suggestion(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(ghost) = self.suggestion_ghost() else {
            return false;
        };
        let _ = self.session.write(ghost.as_bytes());
        self.suggest.ghost = None;
        self.suggest.ai_ghost = None;
        self.suggest.popup_open = false;
        cx.notify();
        true
    }

    /// Tab-cycle the ghost through candidates (forward). Returns whether it did.
    pub(crate) fn cycle_suggestion(&mut self, cx: &mut Context<Self>) -> bool {
        if self.suggest.candidates.is_empty() {
            return false;
        }
        let n = self.suggest.candidates.len();
        let idx = self.suggest.tab_idx.map(|i| (i + 1) % n).unwrap_or(0);
        self.suggest.tab_idx = Some(idx);
        let cand = &self.suggest.candidates[idx];
        let input = &self.suggest.input;
        self.suggest.ghost = cand
            .strip_prefix(input.as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        cx.notify();
        true
    }

    /// Move the popup selection by `delta` (wrapping). No-op when closed.
    pub(crate) fn move_popup(&mut self, delta: i32, cx: &mut Context<Self>) -> bool {
        if !self.suggest.popup_open || self.suggest.candidates.is_empty() {
            return false;
        }
        let n = self.suggest.candidates.len() as i32;
        let sel = (self.suggest.popup_sel as i32 + delta).rem_euclid(n) as usize;
        self.suggest.popup_sel = sel;
        // Mirror the selection into the ghost so it previews inline too.
        let cand = &self.suggest.candidates[sel];
        self.suggest.ghost = cand
            .strip_prefix(self.suggest.input.as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        cx.notify();
        true
    }

    /// Accept the popup's selected candidate.
    pub(crate) fn accept_popup(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.suggest.popup_open {
            return false;
        }
        let Some(cand) = self.suggest.candidates.get(self.suggest.popup_sel).cloned() else {
            return false;
        };
        if let Some(suffix) = cand.strip_prefix(self.suggest.input.as_str()) {
            if !suffix.is_empty() {
                let _ = self.session.write(suffix.as_bytes());
            }
        }
        self.suggest.popup_open = false;
        self.suggest.ghost = None;
        cx.notify();
        true
    }

    /// Dismiss any ghost/popup. Returns whether something was showing.
    pub(crate) fn dismiss_suggestion(&mut self, cx: &mut Context<Self>) -> bool {
        let showing = self.suggest.ghost.is_some() || self.suggest.popup_open;
        if showing {
            self.suggest.ghost = None;
            self.suggest.ai_ghost = None;
            self.suggest.popup_open = false;
            self.suggest.tab_idx = None;
            cx.notify();
        }
        showing
    }

    /// Whether the popup is open (for the key handler's routing).
    pub(crate) fn popup_open(&self) -> bool {
        self.suggest_cfg.popup && self.suggest.popup_open
    }

    /// Drive [`Self::handle_suggestion_key`] with a synthetic plain keypress,
    /// for the dev `suggest_key` IPC op. Routes through the exact code path a
    /// real keystroke hits (which `send_input` bypasses), so accept / Tab-cycle
    /// / popup navigation are verifiable without OS-level synthetic input.
    #[cfg(debug_assertions)]
    pub(crate) fn debug_suggest_key(&mut self, key: &str, cx: &mut Context<Self>) -> bool {
        let ks = Keystroke {
            modifiers: Default::default(),
            key: key.to_string(),
            key_char: None,
        };
        let mods = input::Mods { shift: false, alt: false, ctrl: false, cmd: false };
        self.handle_suggestion_key(&ks, mods, cx)
    }

    /// A snapshot of suggestion state for the dev `read_suggestion` IPC op:
    /// `(input, ghost, candidates, popup_open)`. Lets scripted tests assert the
    /// overlay (which isn't in the grid) as data.
    #[cfg(debug_assertions)]
    pub(crate) fn suggestion_debug(&self) -> (String, Option<String>, Vec<String>, bool) {
        (
            self.suggest.input.clone(),
            self.suggestion_ghost(),
            self.suggest.candidates.clone(),
            self.suggest.popup_open,
        )
    }

    /// Ask Claude for a one-line next-command suggestion off the UI thread. The
    /// result is only applied if it's still for the same input line.
    fn fetch_ai_suggestion(&mut self, input: &str, cx: &mut Context<Self>) {
        if self.suggest.ai_for.as_deref() == Some(input) {
            return; // already fetching / fetched for this line
        }
        self.suggest.ai_for = Some(input.to_string());
        self.suggest.ai_ghost = None;
        let input = input.to_string();
        let recent = self.session.with_term(|t| {
            t.command_history().into_iter().take(10).collect::<Vec<_>>()
        });
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let line = input.clone();
            let suggestion = input.clone();
            let result = executor
                .spawn(async move {
                    let (opts, _) = config::load();
                    crate::relay::suggest_command(&opts, &recent, &suggestion)
                })
                .await;
            let _ = this.update(cx, |view, cx| {
                if view.suggest.ai_for.as_deref() != Some(line.as_str()) {
                    return; // input moved on
                }
                if let Some(full) = result {
                    if let Some(suffix) = full.strip_prefix(line.as_str()) {
                        if !suffix.is_empty() {
                            view.suggest.ai_ghost = Some(suffix.to_string());
                            cx.notify();
                        }
                    }
                }
            });
        })
        .detach();
    }
}
