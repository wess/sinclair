//! A pane's program status: the records its programs report (OSC 7501 /
//! OSC 9;4), cached here so the tab strip and sidebar read them without
//! taking the terminal lock, and the banner a record raises when it starts
//! wanting the user while the pane is out of sight.

use super::*;
use crate::statusmark::Signal;
use vt::status::{Board, Record, State};

impl TerminalView {
  /// Re-read the status records after the session said they changed. A
  /// record that has just become blocked, failed, or done while the pane is
  /// unfocused posts a banner (through the pane's rate limiter).
  pub(crate) fn refresh_status(&mut self, cx: &mut Context<Self>) {
    let board = self.session.with_term(|t| t.status().clone());
    if !self.focused {
      let fresh: Vec<&Record> = board
        .records()
        .iter()
        .filter(|r| {
          Signal::from_record(r).wants_user()
            && self.status.records().iter().find(|o| o.id == r.id).map(|o| o.state) != Some(r.state)
        })
        .collect();
      if let Some(r) = fresh.into_iter().max_by_key(|r| (r.state.urgency(), r.seq)) {
        let (heading, body) = self.status_banner(&board, r);
        self.post_notification(&heading, &body);
        if matches!(r.state, State::Blocked | State::Error) {
          self.attention = true;
          cx.emit(ViewEvent::Attention);
        }
      }
    }
    self.status = board;
    cx.emit(ViewEvent::Status);
    cx.notify();
  }

  /// The banner for `r`. The heading names the pane it came from, so a
  /// program can't pass itself off as one running somewhere else.
  fn status_banner(&self, board: &Board, r: &Record) -> (String, String) {
    let pane = self.title().to_string();
    let app = board.app_of(r).map(vt::status::disarm);
    let heading = match app {
      Some(app) if !pane.is_empty() => format!("{pane} \u{2014} {app}"),
      Some(app) => app,
      None if !pane.is_empty() => pane,
      None => "Sinclair".to_string(),
    };
    let signal = Signal::from_record(r);
    let text = crate::statusmark::describe(r.title.as_deref(), None, r.msg.as_deref());
    let body = if text.is_empty() {
      signal.default_message().to_string()
    } else {
      text
    };
    (heading, body)
  }

  /// This pane's status records.
  pub fn status(&self) -> &Board {
    &self.status
  }

  /// The pane's signal from its program status, if any program reported one.
  pub fn status_signal(&self) -> Option<Signal> {
    self.status.headline().map(Signal::from_record)
  }

  /// The user is back at this pane and typing: drop its `done` and `error`
  /// records, which existed to be found.
  pub(crate) fn acknowledge_status(&mut self, cx: &mut Context<Self>) {
    let seen = self
      .status
      .records()
      .iter()
      .any(|r| matches!(r.state, State::Done | State::Error));
    if seen {
      self.session.with_term(|t| t.acknowledge_status());
      self.refresh_status(cx);
    }
  }
}
