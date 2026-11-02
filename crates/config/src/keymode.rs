//! Key modes: named sets of bindings that only apply while the mode is
//! entered (a `resize` mode where the arrows move dividers until Escape).
//!
//! Bindings join a mode with a `mode/` prefix on the trigger
//! (`resize/left=resize_split:left`). A mode exists as soon as a binding
//! names it; the `keymode` setting only adds options to one:
//! `"keymode": ["resize:exclusive"]`. An exclusive mode swallows keys it
//! doesn't bind instead of letting them reach the terminal.
//!
//! [`ModeStack`] is the runtime state, kept pure so its rules are tested
//! here and the app only maps it onto gpui key contexts.

use crate::action::Action;
use crate::keybind::{Keybind, Mods};
use crate::parse::Diagnostic;

/// A declared (or implied) key mode and its options.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyMode {
  pub name: String,
  /// Keys the mode doesn't bind are dropped instead of reaching the pane.
  pub exclusive: bool,
}

/// Whether `s` can name a mode: ASCII letters, digits, `_` and `-`, so it is
/// also a valid gpui key-context identifier and never contains `+`, `>` or
/// `=`, which keeps `mode/trigger` unambiguous.
pub fn valid_name(s: &str) -> bool {
  !s.is_empty()
    && s
      .bytes()
      .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// Parse one `keymode` entry: `name` or `name:option[,option]`.
pub fn parse_keymode(s: &str) -> Result<KeyMode, String> {
  let (name, opts) = match s.split_once(':') {
    Some((n, o)) => (n.trim(), Some(o)),
    None => (s.trim(), None),
  };
  if !valid_name(name) {
    return Err(format!(
      "invalid mode name `{name}` (letters, digits, `_` and `-`)"
    ));
  }
  let mut mode = KeyMode {
    name: name.to_ascii_lowercase(),
    exclusive: false,
  };
  for opt in opts.into_iter().flat_map(|o| o.split([',', ':'])) {
    match opt.trim().to_ascii_lowercase().as_str() {
      "exclusive" => mode.exclusive = true,
      "" => {}
      other => {
        return Err(format!(
          "unknown mode option `{other}` (expected `exclusive`)"
        ))
      }
    }
  }
  Ok(mode)
}

/// Every mode in play: the declared ones (a later declaration of the same
/// name wins) followed by those only bindings mention, with default options.
pub fn resolve_modes(raw: &[String], binds: &[Keybind]) -> (Vec<KeyMode>, Vec<Diagnostic>) {
  let mut modes: Vec<KeyMode> = Vec::new();
  let mut diags = Vec::new();
  for entry in raw {
    match parse_keymode(entry) {
      Ok(mode) => {
        modes.retain(|m| m.name != mode.name);
        modes.push(mode);
      }
      Err(message) => diags.push(Diagnostic {
        line: 0,
        key: "keymode".to_string(),
        message: format!("`{entry}`: {message}"),
      }),
    }
  }
  for kb in binds {
    if let Some(name) = &kb.mode {
      if !modes.iter().any(|m| &m.name == name) {
        modes.push(KeyMode {
          name: name.clone(),
          exclusive: false,
        });
      }
    }
  }
  (modes, diags)
}

/// `enter_mode` actions that name a mode nothing defines: a typo that would
/// otherwise enter a mode where no key does anything.
pub fn undefined_targets(binds: &[Keybind], modes: &[KeyMode]) -> Vec<(usize, String)> {
  binds
    .iter()
    .enumerate()
    .filter_map(|(i, kb)| match &kb.action {
      Action::EnterMode(name) | Action::EnterModeOnce(name)
        if !modes.iter().any(|m| &m.name == name) =>
      {
        Some((i, name.clone()))
      }
      _ => None,
    })
    .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Active {
  name: String,
  once: bool,
}

/// The modes a window is in, outermost first. The innermost (most recently
/// entered) is consulted first when a key could mean something in several.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModeStack {
  active: Vec<Active>,
}

impl ModeStack {
  /// Enter `name`. Entering a mode that is already active moves it to the
  /// top rather than stacking a second copy.
  pub fn enter(&mut self, name: &str, once: bool) {
    self.active.retain(|a| a.name != name);
    self.active.push(Active {
      name: name.to_string(),
      once,
    });
  }

  /// Leave the innermost mode, returning its name.
  pub fn exit(&mut self) -> Option<String> {
    self.active.pop().map(|a| a.name)
  }

  /// Leave every mode.
  pub fn exit_all(&mut self) {
    self.active.clear();
  }

  pub fn is_empty(&self) -> bool {
    self.active.is_empty()
  }

  /// Active mode names, outermost first.
  pub fn names(&self) -> impl Iterator<Item = &str> {
    self.active.iter().map(|a| a.name.as_str())
  }

  pub fn innermost(&self) -> Option<&str> {
    self.active.last().map(|a| a.name.as_str())
  }

  /// Whether the innermost mode is exclusive.
  pub fn exclusive(&self, modes: &[KeyMode]) -> bool {
    self
      .innermost()
      .is_some_and(|name| modes.iter().any(|m| m.name == name && m.exclusive))
  }

  /// The binding a key sequence means right now among mode bindings: the
  /// innermost active mode that binds `keys` wins. Returns its index.
  pub fn lookup(&self, binds: &[Keybind], keys: &[(Mods, String)]) -> Option<usize> {
    self.active.iter().rev().find_map(|a| {
      binds
        .iter()
        .position(|kb| kb.mode.as_deref() == Some(a.name.as_str()) && kb.sequence() == keys)
    })
  }

  /// A mode-bound key just fired: a mode entered with `enter_mode_once`
  /// ends here. Call before running the bound action, so an action that
  /// enters another mode isn't undone.
  pub fn fired(&mut self) {
    if self.active.last().is_some_and(|a| a.once) {
      self.active.pop();
    }
  }
}

#[cfg(test)]
#[path = "../tests/keymode.rs"]
mod tests;
