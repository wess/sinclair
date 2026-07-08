//! Single-instance IPC: `prompt --toggle-quick` summons the quick terminal and
//! `prompt mcp` bridges Model Context Protocol tool calls into the running GUI.
//!
//! Unix carries this over a per-user domain socket (`unix`). Windows has no
//! named-pipe transport yet, so it falls back to graceful stubs (`windows`)
//! that keep the CLI surface intact while the feature is unavailable.

#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub use unix::*;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::*;
