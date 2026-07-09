//! Sinclair WASM plugin — Rust authoring template. Copy this crate, edit the
//! `Plugin` impl, `cargo build --target wasm32-wasip2 --release`, and ship the
//! resulting `.wasm` next to a `plugin.toml` (`[runtime] type = "wasm"`).
//!
//! Your plugin's *world* imports only the host interfaces it uses — that is what
//! makes capability gating precise (a plugin that never imports `host-net` can't
//! be granted network by accident). This template uses `core + screen`; add
//! `host-commands`, `host-net`, etc. to the world and declare the matching
//! `capabilities` in `plugin.toml` to use more.

wit_bindgen::generate!({
    world: "screentools", // core + screen; define your own world for other caps
    path: "../../crates/pluginrt/wit",
});

use crate::exports::prompt::plugin::guest::Guest;
use crate::prompt::plugin::host_core::log;
use crate::prompt::plugin::host_screen::read_screen;
use crate::prompt::plugin::types::LogLevel;

/// Implement your plugin here. Only `call_tool` is usually interesting; the panel
/// hooks default to empty so a tool-only plugin stays tiny.
struct Plugin;

impl Guest for Plugin {
    /// Runs once when the plugin is instantiated (kept resident afterwards).
    fn init() {
        log(LogLevel::Info, "template plugin ready");
    }

    /// Handle a tool call from the command palette or an MCP agent. `params_json`
    /// is a JSON object of the declared params; return a JSON result string.
    fn call_tool(name: String, params_json: String) -> Result<String, String> {
        match name.as_str() {
            "wordcount" => {
                let params: serde_json::Value =
                    serde_json::from_str(&params_json).map_err(|e| e.to_string())?;
                let lines = params.get("lines").and_then(serde_json::Value::as_u64).unwrap_or(200);
                // Host functions are ordinary calls — no IPC, no process spawn.
                let screen = read_screen(lines as u32)?;
                let words = screen.split_whitespace().count();
                Ok(serde_json::json!({ "words": words }).to_string())
            }
            other => Err(format!("unknown tool: {other}")),
        }
    }

    /// Return this plugin's panel as a node-tree JSON (`{ title, blocks: [...] }`,
    /// the shape the host renders). Return `"{}"` for a tool-only plugin.
    fn render(_request_json: String) -> String {
        serde_json::json!({
            "title": "Template",
            "blocks": [{ "type": "text", "text": "Edit src/lib.rs to build your plugin." }]
        })
        .to_string()
    }

    /// React to a panel UI event (a button click). Update state and re-render.
    fn on_ui_event(_event_json: String) {}
}

export!(Plugin);
