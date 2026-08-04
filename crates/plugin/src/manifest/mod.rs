//! `plugin.toml` parsing: the data model (`model`) and the parser (`parse`).

mod model;
mod parse;

pub use model::{
    Command, CommandMode, Diagnostic, Panel, Plugin, Runtime, RuntimeKind, Tool,
    ToolParam, Trigger, TriggerAction, TriggerTarget, CAPABILITIES,
    TRIGGER_EVENTS,
};
pub use parse::parse;

// The black-box test mirror reaches the public surface via `use super::*`; it
// also names `PathBuf`, so keep it in scope for test builds.
#[cfg(test)]
use std::path::PathBuf;

#[cfg(test)]
#[path = "../../tests/manifest.rs"]
mod tests;
