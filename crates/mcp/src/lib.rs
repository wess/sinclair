//! A minimal Model Context Protocol server over stdio.
//!
//! This is transport + framing only: it speaks the JSON-RPC dialect MCP
//! clients (Claude Desktop, Claude Code, ...) use over a spawned stdio
//! subprocess - newline-delimited JSON, `initialize` / `tools/list` /
//! `tools/call`. It knows nothing about terminals; the caller supplies the
//! tool list and a handler that runs each call. In Sinclair, the handler bridges
//! to the running GUI instance over the single-instance unix socket.
//!
//! Kept dependency-light on purpose: `serde_json` for robust JSON, nothing
//! else. Messages are dispatched synchronously, one at a time, which is all an
//! editor-driven tool session needs.

mod serve;

pub use serve::{serve, Handler};
use serde_json::Value;

/// MCP protocol revision advertised in the `initialize` reply.
pub const PROTOCOL_VERSION: &str = "2024-11-05";

/// One callable tool exposed to clients.
#[derive(Debug, Clone)]
pub struct Tool {
    /// Stable tool id used in `tools/call`.
    pub name: String,
    /// Human-facing description shown by the client.
    pub description: String,
    /// JSON Schema for the tool's arguments (the `inputSchema` field).
    pub input_schema: Value,
}

impl Tool {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        input_schema: Value,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            input_schema,
        }
    }
}
