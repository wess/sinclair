//! Bridge between the MCP stdio server and the running terminal.
//!
//! `prompt mcp` runs [`run_stdio`]: an [`mcp`] server whose every tool call is
//! forwarded over the single-instance unix socket ([`crate::ipc`]) to the
//! already-running GUI instance. There, [`handle`] turns each op into a call
//! on the active [`WorkspaceView`]. Splitting it this way keeps the stdio
//! process trivial (no GUI) and lets any MCP client drive the live terminal.

use std::cell::RefCell;
use std::collections::HashMap;

use gpui::{App, WindowHandle};
use serde_json::{json, Value};

use crate::root::WorkspaceView;
use crate::warmhost::WarmPlugins;
use crate::wasmhost::WasmRuntime;

/// Entry point for the `prompt mcp` subcommand: serve MCP over stdio. Built-in
/// terminal-control tools bridge to the running GUI over the socket; plugin
/// `[[tool]]`s are invoked directly here (a plugin runtime is just a spawn), so
/// agents see the plugins' tools alongside the built-ins. Blocks until stdin
/// closes.
pub fn run_stdio() {
    let plugins = config::load()
        .0
        .plugin
        .clone();
    let plugins = plugin::load(&plugins).0;
    let (tools, routes) = all_tools(&plugins);
    // Resident WASM instances for wasm-runtime plugins, created lazily. `None` if
    // the engine can't start; wasm tools then report it rather than crashing.
    let wasm = RefCell::new(WasmRuntime::new().ok());
    // Warm long-lived processes for persistent native plugins.
    let warm = RefCell::new(WarmPlugins::new());
    mcp::serve(tools, &|name, args| match routes.get(name) {
        Some((index, tool_id)) => call_plugin_tool(&plugins[*index], tool_id, args, &wasm, &warm),
        None => crate::ipc::request(name, args),
    });
}

/// The full tool surface: built-ins plus every plugin `[[tool]]`. Returns the
/// tool list and a route map from MCP tool name to `(plugin index, tool id)`.
fn all_tools(plugins: &[plugin::Plugin]) -> (Vec<mcp::Tool>, HashMap<String, (usize, String)>) {
    let mut list = tools();
    let mut routes = HashMap::new();
    for (i, plugin) in plugins.iter().enumerate() {
        for tool in &plugin.tools {
            // MCP tool names must be [A-Za-z0-9_-]; plugin ids may contain `.`.
            let name = format!("{}_{}", plugin.id, tool.id).replace('.', "_");
            list.push(mcp::Tool::new(
                name.clone(),
                tool.description.clone(),
                tool_schema(&tool.params),
            ));
            routes.insert(name, (i, tool.id.clone()));
        }
    }
    (list, routes)
}

/// Build a JSON-Schema `inputSchema` object from a tool's declared params.
fn tool_schema(params: &[plugin::ToolParam]) -> Value {
    let mut props = serde_json::Map::new();
    let mut required = Vec::new();
    for p in params {
        let mut schema = json!({ "type": p.kind });
        if !p.description.is_empty() {
            schema["description"] = json!(p.description);
        }
        props.insert(p.name.clone(), schema);
        if p.required {
            required.push(json!(p.name));
        }
    }
    json!({ "type": "object", "properties": props, "required": required })
}

/// Invoke a plugin's tool and return its `result`. A `wasm` plugin runs through
/// the in-process [`WasmRuntime`]; a `process` plugin is spawned as before.
fn call_plugin_tool(
    plugin: &plugin::Plugin,
    tool_id: &str,
    args: &Value,
    wasm: &RefCell<Option<WasmRuntime>>,
    warm: &RefCell<WarmPlugins>,
) -> Result<Value, String> {
    if plugin.runtime.as_ref().map(|r| r.kind) == Some(plugin::RuntimeKind::Wasm) {
        let mut rt = wasm.borrow_mut();
        let rt = rt.as_mut().ok_or("wasm plugin runtime is unavailable")?;
        return rt.call_tool(plugin, tool_id, args);
    }
    let req = crate::pluginhost::Request {
        kind: "tool",
        panel: &plugin.id,
        action: None,
        cwd: None,
        method: Some(tool_id),
        params: Some(args),
    };
    // A persistent native plugin is a warm stdio server: send the request line,
    // read the response line, instead of spawning per call.
    if let Some(rt) = plugin.runtime.as_ref().filter(|r| r.persistent) {
        let mut parts = rt.command.split_whitespace();
        let program = parts.next().ok_or("empty runtime command")?;
        let cmd_args: Vec<String> = parts.map(String::from).collect();
        let body = serde_json::to_string(&req).map_err(|e| e.to_string())?;
        let line = warm
            .borrow_mut()
            .request(&plugin.id, program, &cmd_args, &plugin.path, &body)?;
        let resp: crate::pluginhost::Response =
            serde_json::from_str(&line).map_err(|e| format!("bad response: {e}"))?;
        return Ok(resp.result.unwrap_or_else(|| json!({ "ok": true })));
    }
    let resp = crate::pluginhost::invoke(plugin, &req)?;
    Ok(resp.result.unwrap_or_else(|| json!({ "ok": true })))
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
        mcp::Tool::new(
            "agent_states",
            "List the panes in this window with each one's reported agent state \
             (working / blocked / done / idle) and native session id.",
            json!({ "type": "object", "properties": {} }),
        ),
        mcp::Tool::new(
            "worktree_create",
            "Create a git worktree from the focused pane's repository and open it \
             in a new tab. `branch` names a new branch (default: the path's basename).",
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Worktree path (relative to the repo, or absolute)." },
                    "branch": { "type": "string", "description": "Optional new branch name." }
                },
                "required": ["path"]
            }),
        ),
        mcp::Tool::new(
            "worktree_open",
            "Open an existing git worktree path in a new tab.",
            json!({
                "type": "object",
                "properties": { "path": { "type": "string", "description": "Worktree path to open." } },
                "required": ["path"]
            }),
        ),
        mcp::Tool::new(
            "worktree_list",
            "List the git worktrees of the focused pane's repository.",
            json!({ "type": "object", "properties": {} }),
        ),
        mcp::Tool::new(
            "worktree_remove",
            "Remove a git worktree by path (`git worktree remove`).",
            json!({
                "type": "object",
                "properties": { "path": { "type": "string", "description": "Worktree path to remove." } },
                "required": ["path"]
            }),
        ),
    ]
}

/// Server side: run one op against the active workspace and return its result.
pub fn handle(op: &str, args: &Value, cx: &mut App) -> Result<Value, String> {
    if op == "toggle_quick" {
        crate::quick::toggle(cx);
        return Ok(json!({ "ok": true }));
    }
    if op == "notify" {
        let body = args.get("body").and_then(Value::as_str).unwrap_or_default();
        let title = args.get("title").and_then(Value::as_str).unwrap_or("Prompt");
        crate::view::post_os_notification(title, body);
        return Ok(json!({ "ok": true }));
    }
    // An agent status report is addressed by pane token (`PROMPT_PANE`), which
    // may belong to any window — not necessarily the active one — so search all
    // workspaces for the pane rather than dispatching to the frontmost.
    if op == "report_agent" {
        let token = args.get("pane").and_then(Value::as_u64).unwrap_or(0);
        let state = args.get("state").and_then(Value::as_str).unwrap_or_default().to_string();
        let session = args.get("session").and_then(Value::as_str).map(str::to_string);
        let mut applied = false;
        for handle in cx.windows() {
            let Some(workspace) = handle.downcast::<WorkspaceView>() else {
                continue;
            };
            let found = workspace
                .update(cx, |view, _window, cx| {
                    view.apply_agent_report(token, &state, session.as_deref(), cx)
                })
                .unwrap_or(false);
            if found {
                applied = true;
                break;
            }
        }
        return Ok(json!({ "ok": applied }));
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
