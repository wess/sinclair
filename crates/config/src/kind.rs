//! The typed schema of the settings file: every key Sinclair reads and the
//! JSON shape its value takes. `settings` uses it to coerce and validate
//! JSON values before they reach [`crate::apply`], and the app uses it to
//! encode writes, so an invalid value degrades to a warning plus the
//! built-in default instead of breaking the load.

/// The JSON shape a settings key expects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// `true` / `false`.
    Bool,
    /// A whole number.
    Int,
    /// Any number.
    Float,
    /// A string.
    Str,
    /// An array of strings (a single string is accepted as a one-entry list).
    List,
}

/// Every known settings key with its kind. One row per key; `kind()` and the
/// migration/write encoders all read from here.
pub const KEYS: &[(&str, Kind)] = &[
    ("font-family", Kind::List),
    ("font-size", Kind::Float),
    ("font-style", Kind::Str),
    ("font-feature", Kind::List),
    ("adjust-cell-width", Kind::Int),
    ("adjust-cell-height", Kind::Int),
    ("theme", Kind::Str),
    ("theme-light", Kind::Str),
    ("theme-dark", Kind::Str),
    ("background", Kind::Str),
    ("foreground", Kind::Str),
    ("cursor-style", Kind::Str),
    ("cursor-style-blink", Kind::Bool),
    ("cursor-color", Kind::Str),
    ("cursor-text", Kind::Str),
    ("selection-foreground", Kind::Str),
    ("selection-background", Kind::Str),
    ("bold-is-bright", Kind::Bool),
    ("minimum-contrast", Kind::Float),
    ("unfocused-split-opacity", Kind::Float),
    ("split-divider-color", Kind::Str),
    ("mouse-scroll-multiplier", Kind::Float),
    ("macos-option-as-alt", Kind::Str),
    ("window-inherit-working-directory", Kind::Bool),
    ("quit-after-last-window-closed", Kind::Bool),
    ("title", Kind::Str),
    ("clipboard-read", Kind::Str),
    ("clipboard-write", Kind::Str),
    ("scrollback-limit", Kind::Int),
    ("window-padding-x", Kind::Int),
    ("window-padding-y", Kind::Int),
    ("window-width", Kind::Int),
    ("window-height", Kind::Int),
    ("command", Kind::Str),
    ("working-directory", Kind::Str),
    ("copy-on-select", Kind::Bool),
    ("confirm-close-surface", Kind::Bool),
    ("confirm-quit", Kind::Bool),
    ("clipboard-paste-protection", Kind::Bool),
    ("mouse-hide-while-typing", Kind::Bool),
    ("session-restore", Kind::Bool),
    ("shell-integration", Kind::Bool),
    ("auto-update", Kind::Bool),
    ("autosuggest-ghost", Kind::Bool),
    ("autosuggest-popup", Kind::Bool),
    ("autosuggest-tab", Kind::Bool),
    ("autosuggest-ai", Kind::Bool),
    ("autosuggest-history", Kind::Bool),
    ("autosuggest-commands", Kind::Bool),
    ("autosuggest-paths", Kind::Bool),
    ("autosuggest-assist", Kind::Bool),
    ("tab-title-show-host", Kind::Bool),
    ("timestamps", Kind::Bool),
    ("visual-bell", Kind::Bool),
    ("word-chars", Kind::Str),
    ("smart-select", Kind::Bool),
    ("redact", Kind::List),
    ("background-opacity", Kind::Float),
    ("focus-follows-mouse", Kind::Bool),
    ("middle-click-paste", Kind::Bool),
    ("trigger", Kind::List),
    ("snippet", Kind::List),
    ("background-image", Kind::Str),
    ("badge", Kind::Str),
    ("profile", Kind::List),
    ("palette", Kind::List),
    ("plugin", Kind::List),
    ("container", Kind::List),
    ("container-engine", Kind::Str),
    ("container-persist", Kind::Bool),
    ("keybind", Kind::List),
    ("ai-enabled", Kind::Bool),
    ("ai-optimize-tokens", Kind::Bool),
    ("mcp-server-enabled", Kind::Bool),
    ("relay-enabled", Kind::Bool),
    ("relay-start-on-launch", Kind::Bool),
    ("relay-address", Kind::Str),
    ("relay-default-agent", Kind::Str),
    ("relay-team-autonomy", Kind::Bool),
    ("relay-team-window", Kind::Bool),
    ("agent-claude", Kind::Bool),
    ("agent-codex", Kind::Bool),
    ("agent-ollama", Kind::Bool),
    ("agent-gemini", Kind::Bool),
    ("agent-claude-path", Kind::Str),
    ("agent-codex-path", Kind::Str),
    ("agent-gemini-path", Kind::Str),
    ("agent-claude-args", Kind::Str),
    ("agent-codex-args", Kind::Str),
    ("agent-gemini-args", Kind::Str),
    ("agent-custom", Kind::List),
];

/// The kind of a known key, or `None` for an unknown one.
pub fn kind(key: &str) -> Option<Kind> {
    KEYS.iter().find(|(k, _)| *k == key).map(|(_, k)| *k)
}

#[cfg(test)]
#[path = "../tests/kind.rs"]
mod tests;
