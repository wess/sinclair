//! In-window "Change Shortcut" dialog: press a key combination and it becomes
//! the action's shortcut in settings.json. A combination that already does
//! something else asks first (Return replaces, Esc cancels, any other combo
//! tries again). Hosted like the rename dialog, as the workspace's modal.
//!
//! Keys are taken with an app keystroke interceptor rather than a key
//! listener: bindings resolve before listeners, so a listener would never
//! see `cmd+t` — it would open a tab instead.

use gpui::prelude::*;
use gpui::{div, AnyWindowHandle, Context, Keystroke, Subscription, WeakEntity, Window};
use guise::{Kbd, Modal, Size, Text};

use config::{Action, Mods};

use crate::keys;
use crate::root::WorkspaceView;

enum State {
  Waiting,
  /// The pressed combo already runs `existing`; confirm before replacing.
  Conflict {
    mods: Mods,
    key: String,
    existing: String,
  },
  /// The combo was refused; the reason, and still waiting for another.
  Refused(String),
}

pub struct RebindDialog {
  root: WeakEntity<WorkspaceView>,
  label: String,
  action: Action,
  current: Option<String>,
  state: State,
  _intercept: Subscription,
}

impl RebindDialog {
  pub fn new(
    root: WeakEntity<WorkspaceView>,
    label: String,
    action: Action,
    current: Option<String>,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) -> Self {
    let handle: AnyWindowHandle = window.window_handle();
    let this = cx.entity().downgrade();
    let intercept = cx.intercept_keystrokes(move |ev, window, app| {
      if window.window_handle() != handle {
        return;
      }
      let keystroke = ev.keystroke.clone();
      let this = this.clone();
      // Mutating the workspace (closing this dialog drops the interceptor)
      // from inside the interceptor's own callback is asking for trouble;
      // run it once dispatch is done.
      window.defer(app, move |window, app| {
        this
          .update(app, |dialog, cx| dialog.key(&keystroke, window, cx))
          .ok();
      });
      app.stop_propagation();
    });
    // Park focus off the terminal so nothing below sees the keys either.
    window.blur();
    Self {
      root,
      label,
      action,
      current,
      state: State::Waiting,
      _intercept: intercept,
    }
  }

  fn key(&mut self, ks: &Keystroke, window: &mut Window, cx: &mut Context<Self>) {
    match (ks.key.as_str(), &self.state) {
      ("escape", _) => return self.close(window, cx),
      ("enter", State::Conflict { mods, key, .. }) => {
        let (mods, key) = (*mods, key.clone());
        return self.apply(mods, &key, window, cx);
      }
      _ => {}
    }
    let Some((mods, key)) = keys::trigger_of(ks) else {
      return;
    };
    if keys::steals_typing(mods, &key) {
      self.state = State::Refused(
        "Add \u{2318}, \u{2303} or \u{2325}: a bare key would stop reaching the shell.".into(),
      );
      cx.notify();
      return;
    }
    let bound = self
      .root
      .read_with(cx, |ws, _| ws.bound_action(mods, &key))
      .ok()
      .flatten();
    match bound {
      Some(existing) if existing == self.action => self.close(window, cx),
      Some(existing) => {
        self.state = State::Conflict {
          mods,
          key,
          existing: crate::root::action_label(&existing),
        };
        cx.notify();
      }
      None => self.apply(mods, &key, window, cx),
    }
  }

  fn apply(&mut self, mods: Mods, key: &str, window: &mut Window, cx: &mut Context<Self>) {
    let trigger = config::format_trigger(mods, key);
    let action = self.action.clone();
    let result = self
      .root
      .update(cx, |ws, _| ws.assign_shortcut(&action, &trigger));
    match result {
      Ok(Ok(())) => self.close(window, cx),
      Ok(Err(e)) => {
        self.state = State::Refused(e);
        cx.notify();
      }
      Err(_) => {}
    }
  }

  fn close(&self, window: &mut Window, cx: &mut Context<Self>) {
    self
      .root
      .update(cx, |ws, cx| ws.close_modal(window, cx))
      .ok();
  }
}

impl Render for RebindDialog {
  fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
    let root = self.root.clone();
    let current = self
      .current
      .clone()
      .unwrap_or_else(|| "no shortcut".to_string());
    let (prompt, hint): (String, &str) = match &self.state {
      State::Waiting => ("Press the new key combination.".into(), "Esc to cancel"),
      State::Refused(why) => (why.clone(), "Try another \u{2022} Esc to cancel"),
      State::Conflict {
        mods,
        key,
        existing,
      } => (
        format!(
          "{} already runs \u{201c}{existing}\u{201d}. Replace it?",
          keys::shortcut_glyphs(*mods, key).unwrap_or_else(|| key.clone())
        ),
        "Return to replace \u{2022} another combo to retry \u{2022} Esc to cancel",
      ),
    };
    div().absolute().top_0().left_0().size_full().child(
      Modal::new()
        .title("Change Shortcut")
        .width(420.0)
        .on_close(move |_ev, window, app| {
          root.update(app, |ws, cx| ws.close_modal(window, cx)).ok();
        })
        .child(
          div()
            .flex()
            .items_center()
            .gap_2()
            .child(Text::new(self.label.clone()).medium())
            .child(Kbd::new(current)),
        )
        .child(Text::new(prompt))
        .child(Text::new(hint).dimmed().size(Size::Sm)),
    )
  }
}
