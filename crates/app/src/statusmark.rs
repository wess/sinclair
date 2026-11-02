//! What a pane is doing, reduced to one signal, and how that signal is drawn.
//!
//! Two sources feed a pane's signal. The richer one is the program status a
//! program reports itself (OSC 7501 / OSC 9;4, held in `vt::status`); it works
//! for any program, over ssh, and from inside containers. The other is the
//! agent state a Claude Code or Codex hook reports over the socket. When both
//! are present the program's own report wins.

use std::time::Duration;

use gpui::prelude::*;
use gpui::{div, px, Animation, AnimationExt, AnyElement, FontWeight, SharedString};

use crate::agentstate::AgentState;
use vt::status::{Kind, Record, State};

/// One pane's status, as the tab strip and sidebar show it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Signal {
  /// Waiting on the user; what for, when the program said.
  Blocked(Option<Kind>),
  /// Failed and stopped.
  Error,
  /// Running; with a percentage when the program has one.
  Working(Option<u8>),
  /// Finished, result not yet seen.
  Done,
  /// At rest.
  Idle,
}

impl Signal {
  pub fn from_record(r: &Record) -> Signal {
    match r.state {
      State::Blocked => Signal::Blocked(r.kind),
      State::Error => Signal::Error,
      State::Working => Signal::Working(r.progress),
      State::Done => Signal::Done,
      State::Idle => Signal::Idle,
    }
  }

  pub fn from_agent(s: AgentState) -> Signal {
    match s {
      AgentState::Blocked => Signal::Blocked(None),
      AgentState::Working => Signal::Working(None),
      AgentState::Done => Signal::Done,
      AgentState::Idle => Signal::Idle,
    }
  }

  /// Ordering for the inbox: what wants the user most comes first.
  pub fn urgency(self) -> u8 {
    match self {
      Signal::Blocked(_) => 4,
      Signal::Error => 3,
      Signal::Working(_) => 2,
      Signal::Done => 1,
      Signal::Idle => 0,
    }
  }

  /// Whether this belongs in the inbox's "needs you" group.
  pub fn wants_user(self) -> bool {
    matches!(self, Signal::Blocked(_) | Signal::Error | Signal::Done)
  }

  pub fn color(self) -> theme::Rgb {
    match self {
      Signal::Blocked(_) => AgentState::Blocked.color(),
      Signal::Error => theme::Rgb::new(210, 70, 160),
      Signal::Working(_) => AgentState::Working.color(),
      Signal::Done => AgentState::Done.color(),
      Signal::Idle => AgentState::Idle.color(),
    }
  }

  /// The character drawn inside a blocked or failed mark, `None` for a plain
  /// dot. Plain ASCII so every font has it.
  pub fn glyph(self) -> Option<&'static str> {
    match self {
      Signal::Blocked(Some(Kind::Permission)) => Some("!"),
      Signal::Blocked(Some(Kind::Question)) => Some("?"),
      Signal::Blocked(Some(Kind::Auth)) => Some("*"),
      Signal::Blocked(None) => Some("!"),
      Signal::Error => Some("x"),
      _ => None,
    }
  }

  /// A filled circle for plain-text surfaces (sidebar rows).
  pub fn emoji(self) -> &'static str {
    match self {
      Signal::Blocked(_) => "\u{1f534}", // 🔴
      Signal::Error => "\u{1f7e3}",      // 🟣
      Signal::Working(_) => "\u{1f7e1}", // 🟡
      Signal::Done => "\u{1f535}",       // 🔵
      Signal::Idle => "\u{1f7e2}",       // 🟢
    }
  }

  /// A short description: "blocked · permission", "working 40%".
  pub fn label(self) -> String {
    match self {
      Signal::Blocked(Some(k)) => format!("blocked \u{00b7} {}", k.label()),
      Signal::Blocked(None) => "blocked".into(),
      Signal::Error => "error".into(),
      Signal::Working(Some(p)) => format!("working {p}%"),
      Signal::Working(None) => "working".into(),
      Signal::Done => "done".into(),
      Signal::Idle => "idle".into(),
    }
  }

  /// What a banner says when the program gave no message.
  pub fn default_message(self) -> &'static str {
    match self {
      Signal::Blocked(Some(Kind::Permission)) => "Waiting for your approval",
      Signal::Blocked(Some(Kind::Question)) => "Waiting for your answer",
      Signal::Blocked(Some(Kind::Auth)) => "Waiting for credentials",
      Signal::Blocked(None) => "Waiting for you",
      Signal::Error => "Failed",
      Signal::Done => "Finished",
      Signal::Working(_) | Signal::Idle => "",
    }
  }
}

/// Where a pane sits in the activity inbox.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Bucket {
  /// Blocked, failed, or finished: something for the user to look at.
  NeedsYou,
  /// Still running.
  Running,
  /// Nothing going on.
  Quiet,
}

impl Bucket {
  pub fn of(signal: Signal) -> Bucket {
    match signal {
      Signal::Blocked(_) | Signal::Error | Signal::Done => Bucket::NeedsYou,
      Signal::Working(_) => Bucket::Running,
      Signal::Idle => Bucket::Quiet,
    }
  }

  pub fn heading(self) -> &'static str {
    match self {
      Bucket::NeedsYou => "Needs you",
      Bucket::Running => "Running",
      Bucket::Quiet => "Quiet",
    }
  }
}

/// Inbox order for panes with these signals: indices sorted by bucket, then
/// most urgent first, then original (tab) order.
pub fn inbox(signals: &[Signal]) -> Vec<usize> {
  let mut order: Vec<usize> = (0..signals.len()).collect();
  order.sort_by_key(|&i| {
    (
      Bucket::of(signals[i]),
      std::cmp::Reverse(signals[i].urgency()),
      i,
    )
  });
  order
}

/// How long one breath of the working pulse takes.
const PULSE: Duration = Duration::from_millis(1400);

/// The tab mark for `signal`. `key` must be unique among the marks drawn in
/// one frame (the pane token does): it names the pulse animation.
pub fn mark(signal: Signal, key: u64) -> AnyElement {
  let color = crate::colors::hsla(signal.color());
  if let Some(glyph) = signal.glyph() {
    return div()
      .w(px(12.0))
      .h(px(12.0))
      .rounded_full()
      .bg(color)
      .flex()
      .items_center()
      .justify_center()
      .text_size(px(9.0))
      .font_weight(FontWeight::BOLD)
      .text_color(gpui::white())
      .child(SharedString::from(glyph))
      .into_any_element();
  }
  let dot = div().w(px(6.0)).h(px(6.0)).rounded_full().bg(color);
  match signal {
    Signal::Working(progress) => {
      let pulse = dot.with_animation(
        ("status-pulse", key as usize),
        Animation::new(PULSE)
          .repeat()
          .with_easing(gpui::pulsating_between(0.35, 1.0)),
        |d, delta| d.opacity(delta),
      );
      match progress {
        Some(p) => div()
          .flex()
          .items_center()
          .gap(px(3.0))
          .child(pulse)
          .child(
            div()
              .text_size(px(10.0))
              .opacity(0.75)
              .child(SharedString::from(format!("{p}%"))),
          )
          .into_any_element(),
        None => pulse.into_any_element(),
      }
    }
    _ => dot.into_any_element(),
  }
}

/// One line describing a record for the inbox: its title or app, then its
/// message, with invisible formatting stripped.
pub fn describe(title: Option<&str>, app: Option<&str>, msg: Option<&str>) -> String {
  let head = title.or(app).map(vt::status::disarm);
  let msg = msg.map(vt::status::disarm);
  match (head, msg) {
    (Some(h), Some(m)) => format!("{h}: {m}"),
    (Some(h), None) => h,
    (None, Some(m)) => m,
    (None, None) => String::new(),
  }
}

#[cfg(test)]
#[path = "../tests/statusmark.rs"]
mod tests;
