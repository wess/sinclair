//! Pane-tree and tab model: splits, ratios, focus, navigation. Pure logic.
//!
//! Generic over pane content: panes are opaque [`PaneId`]s; the host maps
//! them to whatever payload it owns (terminals, views, ...).

mod id;
mod layout;
mod nav;
mod pane;
mod tabs;
mod tree;

pub use id::{ItemId, ItemIds, PaneId, PaneIds, SplitId};
pub use layout::{compute_layout, Layout, Rect};
pub use nav::{neighbor, next, prev, Direction};
pub use pane::Pane;
pub use tabs::{Tab, Tabs};
pub use tree::{clamp_ratio, Axis, Node, PaneTree, MAX_RATIO, MIN_RATIO};
