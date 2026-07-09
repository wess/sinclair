//! `key = value` configuration loading for the Sinclair terminal emulator.

pub mod action;
mod apply;
pub mod edit;
pub mod keybind;
pub mod options;
pub mod parse;
pub mod value;
pub mod watch;

pub use action::{Action, ResizeDir, SelectAdjust, SplitDirection, SplitFocus};
pub use edit::{set_list, upsert};
pub use keybind::{
    default_keybinds, diff_from_defaults, format_trigger, parse_keybind, resolve, Keybind, Mods,
};
pub use options::{ClipboardAccess, CursorStyle, FontStyle, OptionAsAlt, Options};
pub use parse::{parse_str, Diagnostic};
pub use watch::{watch, WatchHandle};

use std::path::PathBuf;

/// Default config file path: `$XDG_CONFIG_HOME/sinclair/config`, else on Windows
/// `%APPDATA%\sinclair\config`, else `~/.config/sinclair/config`.
///
/// When no config exists at that path but one exists under the pre-rename
/// `prompt` directory, the old path is returned instead, so upgrading keeps
/// reading the user's existing settings. Writing a config at the `sinclair`
/// path takes precedence from the next launch.
pub fn default_path() -> Option<PathBuf> {
    let current = app_path("sinclair")?;
    if !current.exists() {
        if let Some(legacy) = app_path("prompt").filter(|p| p.exists()) {
            return Some(legacy);
        }
    }
    Some(current)
}

/// The config file under `app`'s per-user configuration directory.
fn app_path(app: &str) -> Option<PathBuf> {
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        if !xdg.is_empty() {
            return Some(PathBuf::from(xdg).join(app).join("config"));
        }
    }
    #[cfg(windows)]
    {
        // Windows has no HOME by default; the roaming app-data dir is the home
        // for per-user config.
        if let Some(appdata) = std::env::var_os("APPDATA") {
            if !appdata.is_empty() {
                return Some(PathBuf::from(appdata).join(app).join("config"));
            }
        }
    }
    let home = std::env::var_os("HOME")?;
    if home.is_empty() {
        return None;
    }
    Some(PathBuf::from(home).join(".config").join(app).join("config"))
}

/// Load configuration from an explicit path. A missing or unreadable file
/// yields defaults with no diagnostics.
pub fn load_path(path: &std::path::Path) -> (Options, Vec<Diagnostic>) {
    match std::fs::read_to_string(path) {
        Ok(text) => parse_str(&text),
        Err(_) => (Options::default(), Vec::new()),
    }
}

/// Load configuration from the default path. A missing file yields defaults.
pub fn load() -> (Options, Vec<Diagnostic>) {
    match default_path() {
        Some(path) => load_path(&path),
        None => (Options::default(), Vec::new()),
    }
}

#[cfg(test)]
#[path = "../tests/lib.rs"]
mod tests;
