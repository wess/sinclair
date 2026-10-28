//! The bar a pane shows when its process died too soon to have been used.
//!
//! A pane normally closes the moment its child exits. When that happens within
//! a fraction of a second — a broken shell rc, a mistyped profile command, an
//! agent that cannot start — closing hides the only evidence, and closing the
//! last pane takes the window with it. Such a pane stays open, read-only, with
//! its output and the exit status on show; any key then closes it.

use std::time::Duration;

use gpui::prelude::*;
use gpui::{div, px, AnyElement, SharedString};

use super::TerminalView;
use crate::colors;

/// Exits sooner than this after spawn are treated as failures to start.
const ABNORMAL_RUNTIME: Duration = Duration::from_millis(250);

/// Whether a child that ran for `runtime` should leave its pane open.
pub(crate) fn lingers(runtime: Duration) -> bool {
  runtime < ABNORMAL_RUNTIME
}

/// What the bar says about an exit `code` (`None`: killed by a signal).
pub(crate) fn message(code: Option<i32>) -> String {
  let status = match code {
    Some(0) => "Process exited immediately".to_string(),
    Some(code) => format!("Process exited with code {code}"),
    None => "Process was terminated by a signal".to_string(),
  };
  format!("{status} — press any key to close")
}

impl TerminalView {
  pub(crate) fn exit_overlay(&self) -> Option<AnyElement> {
    let code = self.exited?;
    let failed = code != Some(0);
    let mut bg = colors::hsla(if failed {
      self.colors.palette.get(1)
    } else {
      self.colors.selection_bg
    });
    bg.a = 0.92;
    let fg = colors::hsla(if failed {
      self.colors.bg
    } else {
      self.colors.selection_fg
    });
    Some(
      div()
        .absolute()
        .bottom_0()
        .left_0()
        .right_0()
        .px(px(10.0))
        .py(px(4.0))
        .bg(bg)
        .text_color(fg)
        .child(SharedString::from(message(code)))
        .into_any_element(),
    )
  }
}

#[cfg(test)]
#[path = "../../tests/exitbar.rs"]
mod tests;
