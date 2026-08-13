//! Per-line timestamps: a faint relative time drawn at the right edge of each
//! scrollback row. Times are captured when a row scrolls into history — vt
//! exposes a monotonic `committed_lines` counter, and this keeps a ring of
//! capture times parallel to the scrollback so eviction stays aligned.

use std::collections::VecDeque;
use std::time::{SystemTime, UNIX_EPOCH};

use gpui::{div, px, AnyElement, IntoElement, ParentElement, SharedString, Styled};

use super::TerminalView;

/// gpui global: whether the line-timestamp gutter is on (config `timestamps`).
pub struct TimestampsOn(pub bool);

impl gpui::Global for TimestampsOn {}

pub fn install(on: bool, cx: &mut gpui::App) {
    cx.set_global(TimestampsOn(on));
}

pub fn enabled(cx: &gpui::App) -> bool {
    cx.try_global::<TimestampsOn>().is_some_and(|g| g.0)
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// A compact relative time: `5s`, `2m`, `1h`, `3d`.
fn relative(secs_ago: u64) -> String {
    if secs_ago < 60 {
        format!("{secs_ago}s")
    } else if secs_ago < 3600 {
        format!("{}m", secs_ago / 60)
    } else if secs_ago < 86_400 {
        format!("{}h", secs_ago / 3600)
    } else {
        format!("{}d", secs_ago / 86_400)
    }
}

impl TerminalView {
    /// Keep `line_times` parallel to the scrollback, stamping rows that scrolled
    /// off since the last wakeup with the current time. Called from `wakeup`.
    /// Also prunes annotations whose rows have been evicted, which must happen
    /// whether or not the timestamp gutter is enabled.
    pub(crate) fn update_line_times(&mut self, cx: &gpui::App) {
        let on = enabled(cx);
        if !on && self.annotations.is_empty() {
            // Reset so re-enabling starts fresh rather than back-dating history.
            self.committed_last = u64::MAX;
            self.line_times.clear();
            return;
        }
        let (committed, sb_len) = self
            .session
            .with_term(|t| (t.committed_lines(), t.grid().scrollback().len()));
        self.prune_annotations(committed, sb_len);
        if !on {
            self.committed_last = u64::MAX;
            self.line_times.clear();
            return;
        }
        if self.committed_last == u64::MAX {
            // First scan since enabling: existing scrollback has no real time.
            self.line_times = VecDeque::from(vec![0u64; sb_len]);
            self.committed_last = committed;
            return;
        }
        let new = committed.saturating_sub(self.committed_last);
        let now = now_secs();
        for _ in 0..new.min(sb_len as u64) {
            self.line_times.push_back(now);
        }
        while self.line_times.len() > sb_len {
            self.line_times.pop_front();
        }
        self.committed_last = committed;
    }

    /// The faint relative-time labels beside each visible scrollback row.
    pub(crate) fn timestamps_overlay(&self, cx: &gpui::App) -> Option<AnyElement> {
        if !enabled(cx) || self.line_times.is_empty() {
            return None;
        }
        let (sb_len, offset, rows) = self
            .session
            .with_term(|t| (t.grid().scrollback().len(), t.display_offset(), t.rows()));
        let top = sb_len.saturating_sub(offset);
        let now = now_secs();
        let mut color = crate::colors::hsla(self.colors.fg);
        color.a = 0.28;
        let mut layer = div().absolute().top_0().left_0().size_full();
        let mut any = false;
        for r in 0..rows {
            let global = top + r;
            if global >= sb_len {
                break; // live-screen rows carry no timestamp
            }
            let Some(&t) = self.line_times.get(global) else {
                continue;
            };
            if t == 0 {
                continue;
            }
            any = true;
            let y = self.pad.y + r as f32 * self.cell.height;
            layer = layer.child(
                div()
                    .absolute()
                    .top(px(y))
                    .right(px(self.pad.x + 4.0))
                    .text_color(color)
                    .text_size(px(self.cell.height * 0.62))
                    .child(SharedString::from(relative(now.saturating_sub(t)))),
            );
        }
        any.then(|| layer.into_any_element())
    }
}
