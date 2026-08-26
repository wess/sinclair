//! Configuration loading for the Sinclair terminal emulator. The source of
//! truth is `settings.json` (JSON with comments): compiled-in defaults
//! overridden by whatever keys the user's file sets. The pre-JSON
//! `key = value` format is still parsed for one-time migration.

pub mod action;
mod apply;
pub mod json;
pub mod jsonedit;
pub mod keybind;
pub mod kind;
pub mod options;
pub mod parse;
pub mod settings;
pub mod value;
pub mod watch;

pub use action::{Action, ResizeDir, SelectAdjust, SplitDirection, SplitFocus};
pub use keybind::{
  default_keybinds, diff_from_defaults, format_trigger, parse_keybind, resolve, Keybind, Mods,
};
pub use kind::{kind, Kind};
pub use options::{ClipboardAccess, CursorStyle, FontStyle, OptionAsAlt, Options};
pub use parse::{parse_str, Diagnostic};
pub use watch::{watch, WatchHandle};

use std::path::PathBuf;

/// The settings file: `$XDG_CONFIG_HOME/sinclair/settings.json`, else on
/// Windows `%APPDATA%\sinclair\settings.json`, else
/// `~/.config/sinclair/settings.json`. Reads, writes, and the live-reload
/// watcher all point here.
pub fn default_path() -> Option<PathBuf> {
  app_path("sinclair", "settings.json")
}

/// The pre-JSON `key = value` config file, read only when no settings.json
/// exists (and converted by the app's one-time migration). Prefers the
/// `sinclair` directory, falling back to the pre-rename `prompt` one.
pub fn legacy_path() -> Option<PathBuf> {
  let current = app_path("sinclair", "config")?;
  if !current.exists() {
    if let Some(old) = app_path("prompt", "config").filter(|p| p.exists()) {
      return Some(old);
    }
  }
  Some(current)
}

/// `name` under `app`'s per-user configuration directory.
fn app_path(app: &str, name: &str) -> Option<PathBuf> {
  if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
    if !xdg.is_empty() {
      return Some(PathBuf::from(xdg).join(app).join(name));
    }
  }
  #[cfg(windows)]
  {
    // Windows has no HOME by default; the roaming app-data dir is the home
    // for per-user config.
    if let Some(appdata) = std::env::var_os("APPDATA") {
      if !appdata.is_empty() {
        return Some(PathBuf::from(appdata).join(app).join(name));
      }
    }
  }
  let home = std::env::var_os("HOME")?;
  if home.is_empty() {
    return None;
  }
  Some(PathBuf::from(home).join(".config").join(app).join(name))
}

/// Load configuration from an explicit path, picking the parser by extension
/// (`.json` = settings.json, anything else = legacy `key = value`). A missing
/// or unreadable file yields defaults with no diagnostics.
pub fn load_path(path: &std::path::Path) -> (Options, Vec<Diagnostic>) {
  let Ok(text) = std::fs::read_to_string(path) else {
    return (Options::default(), Vec::new());
  };
  if path.extension().is_some_and(|e| e == "json") {
    settings::parse_json_str(&text)
  } else {
    parse_str(&text)
  }
}

/// Load configuration: settings.json when present, else the legacy config
/// (pre-migration installs), else pure defaults.
pub fn load() -> (Options, Vec<Diagnostic>) {
  if let Some(path) = default_path() {
    if path.exists() {
      return load_path(&path);
    }
  }
  match legacy_path() {
    Some(path) => load_path(&path),
    None => (Options::default(), Vec::new()),
  }
}

#[cfg(test)]
#[path = "../tests/lib.rs"]
mod tests;
