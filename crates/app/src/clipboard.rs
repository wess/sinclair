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

/// The macOS stand-in for X11's primary selection: the text copy-on-select
/// last captured, which a middle click pastes. Linux uses the real one.
#[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
#[derive(Default)]
struct Primary(String);

#[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
impl gpui::Global for Primary {}

/// Set the primary selection to `text`.
pub fn write_primary(text: String, cx: &mut gpui::App) {
  #[cfg(any(target_os = "linux", target_os = "freebsd"))]
  cx.write_to_primary(gpui::ClipboardItem::new_string(text));
  #[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
  cx.set_global(Primary(text));
}

/// The primary selection's text, if anything has set it.
pub fn read_primary(cx: &gpui::App) -> Option<String> {
  #[cfg(any(target_os = "linux", target_os = "freebsd"))]
  let text = cx.read_from_primary().and_then(|item| item.text());
  #[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
  let text = cx.try_global::<Primary>().map(|p| p.0.clone());
  text.filter(|t| !t.is_empty())
}
