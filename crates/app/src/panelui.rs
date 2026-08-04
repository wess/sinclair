//! The panel UI vocabulary: the block tree a plugin returns and the host paints.
//!
//! A plugin never links gpui. Its `render` export emits this description of what
//! it wants shown, and the sidebar renderer in [`crate::root::pluginpanel`]
//! turns the tree into guise components. Effects are separate — a plugin asks
//! for them by calling host functions, not by describing them here.
//!
//! This is the wire format between guest and renderer, so it deserializes
//! straight from the guest's JSON. An unrecognized block becomes
//! [`Block::Unknown`] rather than failing the parse, so one bad node renders an
//! inline notice instead of blanking the whole panel.

use serde::Deserialize;

/// A plugin's reply: the panel contents plus terminal effects to apply.
#[derive(Deserialize, Default, Clone)]
pub struct Response {
    /// Optional override for the panel header.
    #[serde(default)]
    pub title: Option<String>,
    /// The block tree to render.
    #[serde(default)]
    pub blocks: Vec<Block>,
}

/// One node of a plugin-described UI. Maps onto guise components in the
/// sidebar renderer.
#[derive(Deserialize, Clone)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Block {
    /// A dimmed sub-header.
    Section { title: String },
    /// A line of text. `color` names a palette entry (as `badge` does); with a
    /// `color` set, `dimmed` is ignored. `mono` renders in the terminal font,
    /// for text that is a literal piece of terminal output — a prompt, a path,
    /// a command.
    Text {
        text: String,
        #[serde(default)]
        dimmed: bool,
        #[serde(default)]
        color: Option<String>,
        #[serde(default)]
        mono: bool,
    },
    /// A horizontal rule.
    Divider,
    /// A key/value row (key dimmed, value normal).
    Kv { key: String, value: String },
    /// A small pill.
    Badge {
        label: String,
        #[serde(default)]
        color: Option<String>,
    },
    /// A clickable button; clicking sends an `action` request with `id`.
    Button {
        id: String,
        label: String,
        #[serde(default)]
        variant: Option<String>,
    },
    /// A horizontal group of blocks.
    Row {
        #[serde(default)]
        children: Vec<Block>,
    },
    /// An unrecognized block type. Kept as a catch-all so one unknown node (a
    /// newer block type, a typo) renders an inline notice instead of failing the
    /// whole panel's parse.
    #[serde(other)]
    Unknown,
}
