//! Bridge between the MCP stdio server and the running terminal.
//!
//! `prompt mcp` runs [`run_stdio`]: an [`mcp`] server whose every tool call is
//! forwarded over the single-instance unix socket ([`crate::ipc`]) to the
//! already-running GUI instance. There, [`handle`] turns each op into a call
//! on the active [`WorkspaceView`]. Splitting it this way keeps the stdio
//! process trivial (no GUI) and lets any MCP client drive the live terminal.

use gpui::{App, WindowHandle};
use serde_json::{json, Value};

use crate::root::WorkspaceView;

/// Entry point for the `prompt mcp` subcommand: serve MCP over stdio, bridging
/// each tool call to the running instance over the socket. Blocks until stdin
/// closes.
pub fn run_stdio() {
    mcp::serve(tools(), &|name, args| crate::ipc::request(name, args));
}

/// The terminal-control tool surface exposed to MCP clients.
pub fn tools() -> Vec<mcp::Tool> {
    vec![
        mcp::Tool::new(
            "run_command",
            "Type a shell command into the terminal and run it. `target` chooses where: \
             pane (the focused shell, default), tab (a new tab), split_right, or split_down.",
            json!({
                "type": "object",
                "properties": {
                    "text": { "type": "string", "description": "The command line to run." },
                    "target": {
                        "type": "string",
                        "enum": ["pane", "tab", "split_right", "split_down"],
                        "description": "Where to run the command. Defaults to pane."
                    }
                },
                "required": ["text"]
            }),
        ),
        mcp::Tool::new(
            "read_screen",
            "Read recent terminal text (scrollback plus the visible grid) from the focused pane.",
            json!({
                "type": "object",
                "properties": {
                    "lines": {
                        "type": "integer",
                        "minimum": 1,
                        "description": "How many trailing lines to return. Defaults to the visible row count."
                    }
                }
            }),
        ),
        mcp::Tool::new(
            "send_input",
            "Send raw text to the focused pane with no trailing newline — for keystrokes or \
             partial input. Use run_command to run a whole command line.",
            json!({
                "type": "object",
                "properties": { "text": { "type": "string", "description": "Bytes to send as-is." } },
                "required": ["text"]
            }),
        ),
        mcp::Tool::new(
            "new_tab",
            "Open a new terminal tab.",
            json!({ "type": "object", "properties": {} }),
        ),
        mcp::Tool::new(
            "split",
            "Split the focused pane into two. `direction` is right or down.",
            json!({
                "type": "object",
                "properties": {
                    "direction": { "type": "string", "enum": ["right", "down"] }
                },
                "required": ["direction"]
            }),
        ),
        mcp::Tool::new(
            "list_panes",
            "List the panes in the active tab with their titles and working directories.",
            json!({ "type": "object", "properties": {} }),
        ),
        mcp::Tool::new(
            "notify",
            "Post a desktop notification to get the user's attention.",
            json!({
                "type": "object",
                "properties": {
                    "body": { "type": "string", "description": "The notification message." },
                    "title": { "type": "string", "description": "Optional title (default \"Prompt\")." }
                },
                "required": ["body"]
            }),
        ),
        mcp::Tool::new(
            "list_macros",
            "List saved command macros and the commands each one replays.",
            json!({ "type": "object", "properties": {} }),
        ),
        mcp::Tool::new(
            "run_macro",
            "Replay a saved command macro by name into the focused pane.",
            json!({
                "type": "object",
                "properties": { "name": { "type": "string", "description": "The macro name." } },
                "required": ["name"]
            }),
        ),
        mcp::Tool::new(
            "list_tabs",
            "List open tabs with their titles and which one is active.",
            json!({ "type": "object", "properties": {} }),
        ),
        mcp::Tool::new(
            "focus_tab",
            "Switch to the tab at the given zero-based index.",
            json!({
                "type": "object",
                "properties": { "index": { "type": "integer", "minimum": 0 } },
                "required": ["index"]
            }),
        ),
    ]
}

/// Server side: run one op against the active workspace and return its result.
pub fn handle(op: &str, args: &Value, cx: &mut App) -> Result<Value, String> {
    // The quick-terminal toggle needs no workspace and predates the tool set.
    if op == "toggle_quick" {
        crate::quick::toggle(cx);
        return Ok(json!({ "ok": true }));
    }
    // Notifications post natively and need no workspace.
    if op == "notify" {
        let body = args.get("body").and_then(Value::as_str).unwrap_or_default();
        let title = args.get("title").and_then(Value::as_str).unwrap_or("Prompt");
        crate::view::post_os_notification(title, body);
        return Ok(json!({ "ok": true }));
    }
    let workspace = active_workspace(cx).ok_or("no active terminal window")?;
    let op = op.to_string();
    let args = args.clone();
    workspace
        .update(cx, |view, window, cx| view.mcp_dispatch(&op, &args, window, cx))
        .map_err(|_| "terminal window is gone".to_string())?
}

/// The frontmost workspace window, falling back to any workspace window when
/// none is currently active (e.g. another app has focus).
fn active_workspace(cx: &mut App) -> Option<WindowHandle<WorkspaceView>> {
    let mut fallback = None;
    for handle in cx.windows() {
        if let Some(workspace) = handle.downcast::<WorkspaceView>() {
            if workspace.is_active(cx) == Some(true) {
                return Some(workspace);
            }
            fallback.get_or_insert(workspace);
        }
    }
    fallback
}
