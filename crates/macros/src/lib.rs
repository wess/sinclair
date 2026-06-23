//! Recorded command macros for Prompt.
//!
//! A macro is a named, ordered list of shell command lines. The user records
//! one by toggling capture on, typing commands at a prompt, and toggling it
//! off; replay writes each captured line back into the focused shell. Storage
//! is one plain-text file per macro under the config directory, so macros are
//! trivially hand-editable and renamable.
//!
//! This crate is intentionally free of any UI or terminal dependency: the
//! [`Recorder`] consumes plain key descriptions and the [`store`] module is
//! ordinary file IO. The app layer owns when capture runs and how replay is
//! paced.

mod record;
mod store;

pub use record::Recorder;
pub use store::{defaultdir, delete, load, rename, save};

/// A named sequence of shell command lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Macro {
    /// Stable id and display name. Restricted to [`valid_name`] characters so
    /// it is safe both as a filename and as a `macro:<name>` action param.
    pub name: String,
    /// Command lines, in capture order. Replayed one per submitted line.
    pub commands: Vec<String>,
}

impl Macro {
    pub fn new(name: impl Into<String>, commands: Vec<String>) -> Self {
        Self {
            name: name.into(),
            commands,
        }
    }
}

/// A macro name is non-empty and uses only lowercase letters, digits, `.` or
/// `-`. This matches the plugin/command id rule so a name is always a valid
/// filename and a valid `macro:<name>` keybind action parameter.
pub fn valid_name(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.' || b == b'-')
}

/// Best-effort coercion of a free-typed name (e.g. from the rename modal) into
/// a [`valid_name`]: lowercased, spaces and underscores become `-`, and any
/// other disallowed byte is dropped. Returns `None` if nothing usable remains.
pub fn sanitize_name(input: &str) -> Option<String> {
    let mut out = String::with_capacity(input.len());
    for ch in input.trim().chars() {
        match ch {
            'A'..='Z' => out.push(ch.to_ascii_lowercase()),
            'a'..='z' | '0'..='9' | '.' | '-' => out.push(ch),
            ' ' | '_' => out.push('-'),
            _ => {}
        }
    }
    // Collapse runs of '-' and trim leading/trailing separators for tidiness.
    let collapsed: String = out
        .split('-')
        .filter(|seg| !seg.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    (!collapsed.is_empty()).then_some(collapsed)
}

#[cfg(test)]
#[path = "../tests/lib.rs"]
mod tests;
