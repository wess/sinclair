//! Hint mode ("follow links by keyboard"): label every visible URL with a short
//! keystroke; typing a label opens it. Modeled on the search overlay — a small
//! optional state on the view, intercepted in `key_down`, drawn over the grid.

use gpui::{div, px, AnyElement, Context, IntoElement, ParentElement, SharedString, Styled};

use super::TerminalView;
use crate::colors;

/// One labelled link target in the viewport.
pub(crate) struct Hint {
    pub row: usize,
    pub start_col: usize,
    pub label: String,
    pub url: String,
}

/// Active hint-mode state: the labelled targets and the prefix typed so far.
pub(crate) struct Hints {
    pub hints: Vec<Hint>,
    pub typed: String,
}

/// Home-row-first label alphabet (Vimium-style).
const ALPHABET: &str = "asdfghjklqwertyuiopzxcvbnm";

/// `n` distinct labels: single characters while they last, then two-char combos.
fn labels(n: usize) -> Vec<String> {
    let a: Vec<char> = ALPHABET.chars().collect();
    if n <= a.len() {
        return a.iter().take(n).map(|c| c.to_string()).collect();
    }
    let mut out = Vec::with_capacity(n);
    'outer: for &c1 in &a {
        for &c2 in &a {
            out.push(format!("{c1}{c2}"));
            if out.len() >= n {
                break 'outer;
            }
        }
    }
    out
}

/// Add a scheme so bare `www.…` links still open.
fn with_scheme(url: &str) -> String {
    if url.contains("://") {
        url.to_string()
    } else {
        format!("https://{url}")
    }
}

impl TerminalView {
    /// Enter hint mode over the visible URLs. A no-op when there are none.
    pub(crate) fn open_hints(&mut self, cx: &mut Context<Self>) {
        let links = self.session.with_term(|t| t.visible_links());
        if links.is_empty() {
            return;
        }
        let labels = labels(links.len());
        let hints = links
            .into_iter()
            .zip(labels)
            .map(|((row, start_col, _end, url), label)| Hint { row, start_col, label, url })
            .collect();
        self.hints = Some(Hints { hints, typed: String::new() });
        cx.notify();
    }

    pub(crate) fn hints_active(&self) -> bool {
        self.hints.is_some()
    }

    /// Handle a keystroke while hint mode is open: build up the typed label,
    /// open on an exact match, cancel on Escape or an impossible prefix.
    pub(crate) fn hints_key(&mut self, ks: &gpui::Keystroke, cx: &mut Context<Self>) {
        match ks.key.as_str() {
            "escape" => {
                self.hints = None;
                cx.notify();
                return;
            }
            "backspace" => {
                if let Some(h) = &mut self.hints {
                    h.typed.pop();
                }
                cx.notify();
                return;
            }
            _ => {}
        }
        let Some(ch) = ks
            .key_char
            .as_deref()
            .filter(|c| c.chars().count() == 1 && c.chars().all(|c| c.is_ascii_alphabetic()))
        else {
            return;
        };
        let mut open = None;
        let mut cancel = false;
        if let Some(h) = &mut self.hints {
            h.typed.push_str(&ch.to_ascii_lowercase());
            if let Some(hit) = h.hints.iter().find(|hh| hh.label == h.typed) {
                open = Some(hit.url.clone());
            } else if !h.hints.iter().any(|hh| hh.label.starts_with(&h.typed)) {
                cancel = true;
            }
        }
        if let Some(url) = open {
            self.hints = None;
            let url = with_scheme(&url);
            if crate::pointer::openable(&url) {
                cx.open_url(&url);
            } else {
                eprintln!("prompt: refused to open link with disallowed scheme: {url}");
            }
        } else if cancel {
            self.hints = None;
        }
        cx.notify();
    }

    /// The floating label chips over the grid, or `None` when hint mode is off.
    pub(crate) fn hints_overlay(&self) -> Option<AnyElement> {
        let h = self.hints.as_ref()?;
        let bg = colors::rgba(self.colors.selection_bg);
        let fg = colors::rgba(self.colors.selection_fg);
        let mut chips = div().absolute().top_0().left_0().size_full();
        for hint in &h.hints {
            if !hint.label.starts_with(&h.typed) {
                continue;
            }
            let x = self.pad.x + hint.start_col as f32 * self.cell.width;
            let y = self.pad.y + hint.row as f32 * self.cell.height;
            chips = chips.child(
                div()
                    .absolute()
                    .left(px(x))
                    .top(px(y))
                    .px(px(3.0))
                    .rounded(px(3.0))
                    .bg(bg)
                    .text_color(fg)
                    .text_size(px(self.cell.height * 0.72))
                    .child(SharedString::from(hint.label.to_uppercase())),
            );
        }
        Some(chips.into_any_element())
    }
}
