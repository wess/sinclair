//! Keymap surfaces reachable from the palette: "Show Keymap" (a tab running
//! `sinclair keymap`) and "Change Shortcut…" (pick an action, press a combo,
//! written to settings.json).

use super::*;

/// The palette's label for an action, else its config name.
pub(crate) fn action_label(action: &Action) -> String {
  palette_catalog()
    .into_iter()
    .find(|(_, a)| a == action)
    .map(|(label, _)| label.to_string())
    .unwrap_or_else(|| action.to_config())
}

impl WorkspaceView {
  /// Open a tab paging this binary's `keymap` listing. Paged so the tab
  /// stays up to be read; quitting the pager closes it.
  pub(crate) fn show_keymap(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let Ok(exe) = std::env::current_exe() else {
      eprintln!("sinclair: show keymap: can't locate the running executable");
      return;
    };
    let exe = exe.to_string_lossy().replace('\'', "'\\''");
    let command = format!("'{exe}' keymap | less -S");
    if let Some(id) = self.spawncommand(&command, window, cx) {
      self.group.update(cx, |g, cx| g.add_to_focused(id, cx));
      self.focusactive(window, cx);
      cx.notify();
    }
  }

  /// Step one of Change Shortcut: a Spotlight over the palette's actions,
  /// each showing its current shortcut. Picking one opens the capture dialog.
  pub(crate) fn open_change_shortcut(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let view = cx.entity();
    let entries: Vec<(String, Option<String>, Action)> = palette_catalog()
      .into_iter()
      .filter(|(_, a)| *a != Action::ChangeShortcut)
      .map(|(label, action)| (label.to_string(), self.shortcut_hint(&action), action))
      .collect();
    let spotlight = cx.new(|scx| {
      let mut spot = guise::Spotlight::new(scx);
      for (label, hint, action) in entries {
        let view = view.clone();
        let current = hint.clone();
        let name = label.clone();
        let run = move |window: &mut Window, app: &mut App| {
          let (name, action, current) = (name.clone(), action.clone(), current.clone());
          view.update(app, |this, cx| {
            this.open_rebind(name, action, current, window, cx)
          });
        };
        spot = match hint {
          Some(hint) => spot.item_hint(label, hint, run),
          None => spot.item(label, run),
        };
      }
      spot
    });
    spotlight.update(cx, |spot, scx| spot.open(window, scx));
    self.spotlight = Some(spotlight);
    cx.notify();
  }

  fn open_rebind(
    &mut self,
    label: String,
    action: Action,
    current: Option<String>,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) {
    let root = cx.entity().downgrade();
    let view =
      cx.new(|cx| crate::rebind::RebindDialog::new(root, label, action, current, window, cx));
    self.modal = Some(view.into());
    cx.notify();
  }

  /// The action an always-on single-stroke binding on this trigger runs.
  pub(crate) fn bound_action(&self, mods: config::Mods, key: &str) -> Option<Action> {
    self
      .keybinds
      .iter()
      .find(|kb| kb.mode.is_none() && kb.tail.is_empty() && kb.mods == mods && kb.key == key)
      .map(|kb| kb.action.clone())
  }

  /// Make `trigger` the shortcut for `action` in settings.json. The file
  /// watcher's live reload rebinds every window.
  pub(crate) fn assign_shortcut(&self, action: &Action, trigger: &str) -> Result<(), String> {
    let entries = config::rebind(&self.opts.keybind, action, trigger)?;
    crate::confwrite::set_list("keybind", &entries);
    Ok(())
  }
}
