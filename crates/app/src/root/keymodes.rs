//! Key modes on the workspace: the window's [`config::ModeStack`] mapped
//! onto gpui key contexts. Each active mode adds `keymode_<name>` (plus a
//! bare `keymode`) to the workspace's context, and mode bindings are bound
//! with that predicate (see `menus.rs`), so gpui itself only matches them
//! while the mode is on.

use super::*;
use gpui::prelude::*;

/// The key-context identifier that switches a mode's bindings on.
pub(crate) fn keymode_context(mode: &str) -> String {
  format!("keymode_{mode}")
}

impl WorkspaceView {
  /// Install a freshly resolved keybind table, re-deriving the modes it
  /// defines. Active modes that no longer exist are left.
  pub(crate) fn set_keybinds(&mut self, keybinds: Vec<Keybind>, cx: &mut Context<Self>) {
    let (keymodes, diags) = config::keymode::resolve_modes(&self.opts.keymode, &keybinds);
    for d in &diags {
      eprintln!("sinclair: {}: {}", d.key, d.message);
    }
    self.keybinds = keybinds;
    self.keymodes = keymodes;
    let stale = self
      .modes
      .names()
      .any(|n| !self.keymodes.iter().any(|m| m.name == n));
    if stale {
      self.modes.exit_all();
    }
    self.applykeybinds(cx);
    cx.notify();
  }

  /// The workspace root's key context: `Workspace`, plus the active modes.
  pub(crate) fn key_context(&self) -> gpui::KeyContext {
    let mut context = gpui::KeyContext::default();
    context.add("Workspace");
    if !self.modes.is_empty() {
      context.add("keymode");
      for name in self.modes.names() {
        context.add(keymode_context(name));
      }
    }
    context
  }

  pub(crate) fn enter_mode(&mut self, name: &str, once: bool, cx: &mut Context<Self>) {
    if !self.keymodes.iter().any(|m| m.name == name) {
      eprintln!(
        "sinclair: no key mode `{name}` (bind `{name}/<key>=...` or declare it in `keymode`)"
      );
      return;
    }
    self.modes.enter(name, once);
    cx.notify();
  }

  pub(crate) fn exit_mode(&mut self, cx: &mut Context<Self>) {
    if self.modes.exit().is_some() {
      cx.notify();
    }
  }

  pub(crate) fn exit_all_modes(&mut self, cx: &mut Context<Self>) {
    if !self.modes.is_empty() {
      self.modes.exit_all();
      cx.notify();
    }
  }

  /// A mode binding matched. gpui picked one of possibly several active
  /// modes' bindings for this key by table order; resolve it again against
  /// the stack so the most recently entered mode wins.
  pub(crate) fn modekey(&mut self, action: &ModeKey, window: &mut Window, cx: &mut Context<Self>) {
    let Some(kb) = self.keybinds.get(action.0) else {
      return;
    };
    let Some(index) = self.modes.lookup(&self.keybinds, &kb.sequence()) else {
      return;
    };
    let action = self.keybinds[index].action.clone();
    // A once-mode's `exit_mode` key should leave that mode, not it and the
    // one below.
    if action != Action::ExitMode {
      self.modes.fired();
    }
    cx.notify();
    self.dispatch(action, window, cx);
  }

  pub(crate) fn leavemode(&mut self, _: &LeaveMode, _window: &mut Window, cx: &mut Context<Self>) {
    self.exit_mode(cx);
  }

  /// In an exclusive mode, a key no binding claimed must not reach the
  /// focused terminal. Bindings run before key listeners, so anything that
  /// gets here was unbound. Only the terminal is shielded: a dialog or the
  /// palette opened from inside the mode still takes typing.
  pub(crate) fn swallow_unbound(&self, window: &Window, cx: &mut Context<Self>) {
    if !self.modes.exclusive(&self.keymodes) {
      return;
    }
    let terminal_focused = self
      .focused_terminal(cx)
      .is_some_and(|t| t.read(cx).focus_handle(cx).contains_focused(window, cx));
    if terminal_focused {
      cx.stop_propagation();
    }
  }

  /// The floating pill naming the active mode (innermost, with the depth
  /// when modes are nested). Bottom-right, clear of the top-right status
  /// pills.
  pub(crate) fn mode_pill(&self) -> Option<AnyElement> {
    let name = self.modes.innermost()?;
    let depth = self.modes.names().count();
    let exclusive = self.modes.exclusive(&self.keymodes);
    let accent = theme::Rgb::new(120, 190, 250);
    let mut bg = colors::hsla(self.colors.bg);
    bg.a = 0.9;
    let mut border = colors::hsla(accent);
    border.a = 0.5;
    let mut label = name.to_uppercase();
    if depth > 1 {
      label.push_str(&format!(" +{}", depth - 1));
    }
    if exclusive {
      label.push_str(" \u{2022} LOCKED");
    }
    Some(
      div()
        .absolute()
        .bottom(px(8.0))
        .right(px(8.0))
        .flex()
        .items_center()
        .gap_1()
        .px_2()
        .py(px(2.0))
        .rounded(px(6.0))
        .bg(bg)
        .border_1()
        .border_color(border)
        .text_size(px(11.0))
        .text_color(colors::hsla(accent))
        .child(SharedString::from("\u{2328}"))
        .child(SharedString::from(label))
        .child(div().opacity(0.6).child(SharedString::from("esc to leave")))
        .into_any_element(),
    )
  }
}
