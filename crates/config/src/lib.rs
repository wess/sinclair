//! `key = value` configuration loading for the Prompt terminal emulator.

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

/// Default config file path: `$XDG_CONFIG_HOME/prompt/config`, else on Windows
/// `%APPDATA%\prompt\config`, else `~/.config/prompt/config`.
pub fn default_path() -> Option<PathBuf> {
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        if !xdg.is_empty() {
            return Some(PathBuf::from(xdg).join("prompt").join("config"));
        }
    }
    #[cfg(windows)]
    {
        // Windows has no HOME by default; the roaming app-data dir is the home
        // for per-user config.
        if let Some(appdata) = std::env::var_os("APPDATA") {
            if !appdata.is_empty() {
                return Some(PathBuf::from(appdata).join("prompt").join("config"));
            }
        }
    }
    let home = std::env::var_os("HOME")?;
    if home.is_empty() {
        return None;
    }
    Some(
        PathBuf::from(home)
            .join(".config")
            .join("prompt")
            .join("config"),
    )
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
