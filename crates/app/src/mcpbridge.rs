//! Bridge between the MCP stdio server and the running terminal.
//!
//! `sinclair mcp` runs [`run_stdio`]: an [`mcp`] server whose every tool call is
//! forwarded over the single-instance unix socket ([`crate::ipc`]) to the
//! already-running GUI instance. There, [`handle`] turns each op into a call
//! on the active [`WorkspaceView`]. Splitting it this way keeps the stdio
//! process trivial (no GUI) and lets any MCP client drive the live terminal.

use std::cell::RefCell;
use std::collections::HashMap;

use gpui::{App, WindowHandle};
use serde_json::{json, Value};

use crate::root::WorkspaceView;
use crate::wasmhost::WasmRuntime;

/// Entry point for the `sinclair mcp` subcommand: serve MCP over stdio. Built-in
/// terminal-control tools bridge to the running GUI over the socket; plugin
/// `[[tool]]`s run here in-process on a resident WASM instance, so agents see
/// the plugins' tools alongside the built-ins. Blocks until stdin closes.
pub fn run_stdio() {
  let plugins = config::load().0.plugin.clone();
  let plugins = plugin::load(&plugins).0;
  let (tools, routes) = all_tools(&plugins);
  // Resident WASM instances, created lazily. `None` if the engine can't start;
  // plugin tools then report it rather than crashing.
  let wasm = RefCell::new(WasmRuntime::new().ok());
  mcp::serve(tools, &|name, args| match routes.get(name) {
    Some((index, tool_id)) => call_plugin_tool(&plugins[*index], tool_id, args, &wasm),
    // Waiting polls from this process, so a long wait never holds the
    // GUI's socket (see `cli::wait`).
    None if name == "wait" => mcp_wait(args),
    None if name == "events" => mcp_events(args),
    None => crate::ipc::request(name, args),
  });
}

fn ipc_call(op: &str, args: Value) -> Result<Value, crate::cli::Failure> {
  crate::ipc::request(op, &args).map_err(crate::cli::Failure::from_request)
}

/// Longest an MCP `wait` or `events` call may block.
const MCP_WAIT_CAP: u64 = 1800;

fn mcp_wait(args: &Value) -> Result<Value, String> {
  let until = crate::cli::wait::Until::from_args(args)?;
  let secs = args
    .get("timeout_secs")
    .and_then(Value::as_u64)
    .unwrap_or(300)
    .min(MCP_WAIT_CAP);
  let pane = args.get("pane").and_then(Value::as_str);
  crate::cli::wait::wait(
    &until,
    pane,
    Some(std::time::Duration::from_secs(secs)),
    None,
    &ipc_call,
  )
    .map(|o| o.json)
    .map_err(|f| f.message)
}

/// Events after `after` (or a fresh cursor). With `wait_secs`, block until at
/// least one arrives or the time is up.
fn mcp_events(args: &Value) -> Result<Value, String> {
  let secs = args
    .get("wait_secs")
    .and_then(Value::as_u64)
    .unwrap_or(0)
    .min(MCP_WAIT_CAP);
  let deadline = std::time::Instant::now() + std::time::Duration::from_secs(secs);
  let mut args = args.clone();
  if args.get("after").is_none() {
    let head = crate::ipc::request("events", &json!({}))?;
    args["after"] = head["next"].clone();
  }
  loop {
    let out = crate::ipc::request("events", &args)?;
    let empty = out["events"].as_array().is_none_or(|e| e.is_empty());
    if !empty || std::time::Instant::now() >= deadline {
      return Ok(out);
    }
    std::thread::sleep(std::time::Duration::from_millis(200));
  }
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

/// Invoke a plugin's tool on its resident WASM instance.
fn call_plugin_tool(
  plugin: &plugin::Plugin,
  tool_id: &str,
  args: &Value,
  wasm: &RefCell<Option<WasmRuntime>>,
) -> Result<Value, String> {
  let mut rt = wasm.borrow_mut();
  let rt = rt.as_mut().ok_or("the plugin runtime is unavailable")?;
  rt.call_tool(plugin, tool_id, args)
}

/// The `pane` argument every pane-addressed tool takes.
fn pane_param() -> Value {
  json!({
      "type": "string",
      "description": "Which pane: `pane:<n>` (an id from `ls`), a tab position from 1, \
                      or a title or unique fragment of one. Defaults to the focused pane."
  })
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
              },
              "pane": pane_param()
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
              },
              "pane": pane_param()
          }
      }),
    ),
    mcp::Tool::new(
      "send_input",
      "Send raw text to the focused pane with no trailing newline — for keystrokes or \
             partial input. Use run_command to run a whole command line.",
      json!({
          "type": "object",
          "properties": {
              "text": { "type": "string", "description": "Bytes to send as-is." },
              "pane": pane_param()
          },
          "required": ["text"]
      }),
    ),
    mcp::Tool::new(
      "new_tab",
      "Open a new terminal tab.",
      json!({ "type": "object", "properties": {} }),
    ),
    mcp::Tool::new(
      "action",
      "Dispatch a config action on the active window, exactly as a keybinding \
             would. Takes the keybind action vocabulary: `notes`, `new_tab`, \
             `close_tab`, `goto_tab:2`, `quit`, ...",
      json!({
          "type": "object",
          "properties": {
              "action": {
                  "type": "string",
                  "description": "The action, as `name` or `name:param`."
              }
          },
          "required": ["action"]
      }),
    ),
    mcp::Tool::new(
      "ls",
      "List every tab with its id (`pane:<n>`), position, title, cwd, whether a command \
             is running, and its status (what its program reports: working, blocked, done...).",
      json!({ "type": "object", "properties": {} }),
    ),
    mcp::Tool::new(
      "status",
      "A pane's program status records (OSC 7501 / progress) and summary signal.",
      json!({ "type": "object", "properties": { "pane": pane_param() } }),
    ),
    mcp::Tool::new(
      "focus",
      "Bring a pane to the front.",
      json!({ "type": "object", "properties": { "pane": pane_param() }, "required": ["pane"] }),
    ),
    mcp::Tool::new(
      "wait",
      "Block until something happens in a pane, instead of polling read_screen: `exit` \
             (its process exits; returns the exit code), `command` (the next shell command \
             finishes; needs shell integration), `text` (the screen matches the regex \
             `pattern`, e.g. a dev server's \"ready\" line), or `status` (its status becomes \
             one of `state`, comma-separated; default done,error,blocked).",
      json!({
          "type": "object",
          "properties": {
              "for": { "type": "string", "enum": ["exit", "command", "text", "status"] },
              "pattern": { "type": "string", "description": "Regex, for `text`." },
              "state": { "type": "string", "description": "States, for `status`." },
              "pane": pane_param(),
              "timeout_secs": { "type": "integer", "minimum": 1, "description": "Give up after this long (default 300, max 1800)." }
          },
          "required": ["for"]
      }),
    ),
    mcp::Tool::new(
      "events",
      "Terminal events (bell, title_changed, dir_changed, notify, command_finished, exit, \
             status_changed) after cursor `after`. Call without `after` to get a starting \
             cursor (`next`); pass `next` back to get what happened since. `wait_secs` blocks \
             until at least one event arrives.",
      json!({
          "type": "object",
          "properties": {
              "after": { "type": "integer" },
              "pane": pane_param(),
              "wait_secs": { "type": "integer", "minimum": 0 }
          }
      }),
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
              "title": { "type": "string", "description": "Optional title (default \"Sinclair\")." }
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
  // Agent status reports are local UX (the status dots for panes this
  // instance spawned), not exposure — like toggle_quick they stay available.
  // Every other op is the MCP tool-dispatch surface, gated on the live
  // "MCP server" setting (config reload applies it without a restart).
  // Debug builds skip the gate: `sinclair ipc` is the documented dev drive
  // surface and is compiled out of release builds.
  if op != "report_agent" && !cfg!(debug_assertions) && !mcp_enabled(cx) {
    return Err(
      "the MCP server is disabled (Settings \u{2192} AI \u{2192} MCP server)".to_string(),
    );
  }
  if op == "notify" {
    let body = args.get("body").and_then(Value::as_str).unwrap_or_default();
    let title = args
      .get("title")
      .and_then(Value::as_str)
      .unwrap_or("Sinclair");
    crate::view::post_os_notification(title, body);
    return Ok(json!({ "ok": true }));
  }
  // An agent status report is addressed by pane token (`SINCLAIR_PANE`), which
  // may belong to any window — not necessarily the active one — so search all
  // workspaces for the pane rather than dispatching to the frontmost.
  if op == "report_agent" {
    let token = args.get("pane").and_then(Value::as_u64).unwrap_or(0);
    let state = args
      .get("state")
      .and_then(Value::as_str)
      .unwrap_or_default()
      .to_string();
    let session = args
      .get("session")
      .and_then(Value::as_str)
      .map(str::to_string);
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
  // The journal is app-wide and outlives windows; reading all of it needs
  // no window (a wait on a pane whose window just closed still finds the
  // pane's last events).
  if op == "events" && args.get("pane").is_none() {
    return Ok(crate::root::journal_since(args, cx));
  }
  // A pane named by id may live in any window; anything else is resolved
  // against the frontmost one.
  let by_id = args
    .get("pane")
    .and_then(Value::as_str)
    .and_then(|p| p.strip_prefix("pane:"))
    .and_then(|n| n.parse::<u64>().ok());
  let workspace = match by_id {
    Some(token) => workspace_with(token, cx)
      .ok_or_else(|| format!("{}: no pane pane:{token}", crate::target::NOT_FOUND))?,
    None => active_workspace(cx).ok_or("no active terminal window")?,
  };
  let op = op.to_string();
  let args = args.clone();
  workspace
    .update(cx, |view, window, cx| {
      view.mcp_dispatch(&op, &args, window, cx)
    })
    .map_err(|_| "terminal window is gone".to_string())?
}

/// Whether the live config exposes this terminal over MCP. Read from a
/// workspace's options (kept fresh by the config watcher) so a reload applies
/// without restarting. With no workspace window there is nothing to expose.
fn mcp_enabled(cx: &mut App) -> bool {
  active_workspace(cx)
    .and_then(|w| w.update(cx, |view, _window, _cx| view.mcp_enabled()).ok())
    .unwrap_or(false)
}

/// The workspace window holding the pane with this token.
fn workspace_with(token: u64, cx: &mut App) -> Option<WindowHandle<WorkspaceView>> {
  cx.windows().into_iter().find_map(|handle| {
    let workspace = handle.downcast::<WorkspaceView>()?;
    workspace
      .update(cx, |view, _w, _cx| view.has_pane_token(token))
      .ok()
      .filter(|&found| found)
      .map(|_| workspace)
  })
}

/// The frontmost workspace window, falling back to any workspace window when
/// none is currently active (e.g. another app has focus).
pub(crate) fn active_workspace(cx: &mut App) -> Option<WindowHandle<WorkspaceView>> {
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
