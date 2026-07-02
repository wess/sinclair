//! Clipboard history: a small ring of recent copies, shared as a gpui global so
//! any copy path can record into it and a picker can paste from it.

use std::collections::VecDeque;

/// How many recent copies to keep.
const CAP: usize = 50;

/// gpui global holding the recent-copies ring (newest first).
#[derive(Default)]
pub struct History(pub VecDeque<String>);

impl gpui::Global for History {}

/// Record `text` as the most recent copy (de-duplicated, capped). Call this
/// wherever text is written to the system clipboard by a user action.
pub fn remember(text: &str, cx: &mut gpui::App) {
    if text.is_empty() {
        return;
    }
    let mut ring = cx
        .try_global::<History>()
        .map(|h| h.0.clone())
        .unwrap_or_default();
    ring.retain(|e| e != text);
    ring.push_front(text.to_string());
    ring.truncate(CAP);
    cx.set_global(History(ring));
}

/// The current history entries, newest first.
pub fn entries(cx: &gpui::App) -> Vec<String> {
    cx.try_global::<History>()
        .map(|h| h.0.iter().cloned().collect())
        .unwrap_or_default()
}
