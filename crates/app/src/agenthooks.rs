//! Agent status reporting and hook installation.
//!
//! Two process-mode entry points, both invoked as the `sinclair` binary:
//!
//! - `sinclair agent-status <state> [--session id] [--name n] [--pane token]` —
//!   an agent (or its lifecycle hook) reports its semantic state back to the
//!   running GUI over the single-instance socket. The pane is identified by the
//!   `SINCLAIR_PANE` token injected into every spawned session's environment, so a
//!   hook needs no arguments beyond the state. When stdin is piped (as hooks
//!   receive it) any `session_id` in the JSON is captured for session resume,
//!   and on a `SessionStart` payload the agent is told (via the hook's JSON
//!   output) that it runs inside Sinclair and which pane it is in.
//!
//! - `sinclair agent-hooks install|uninstall|status [--agent claude|codex]...
//!   [--project]` — write, remove, or inspect lifecycle hooks in Claude Code's
//!   `settings.json` and Codex's `hooks.json` so an agent running in a pane
//!   reports `working` / `blocked` / `done` / `idle` automatically. Only our own
//!   entries are touched.
//!
//! Both agents share one on-disk shape:
//! `{"hooks": {"<Event>": [{"hooks": [{"type": "command", "command": "…"}]}]}}`.
//! For Codex this is `HooksFile` in openai/codex `codex-rs/config/src/hook_config.rs`
//! (top level allows only `description` and `hooks`; event names are the
//! PascalCase ones below). Codex remembers hook approvals by event, group index
//! and handler index, so we only ever append a new group at the end of an
//! event's array and rewrite our own command in place.

use std::io::Read;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

/// A coding agent whose lifecycle hooks Sinclair can install.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Agent {
  Claude,
  Codex,
}

const AGENTS: [Agent; 2] = [Agent::Claude, Agent::Codex];

/// Claude Code lifecycle events and the state each maps to. Ordered for a
/// stable settings file.
const CLAUDE_EVENTS: &[(&str, &str)] = &[
  ("SessionStart", "idle"),
  ("UserPromptSubmit", "working"),
  ("Notification", "blocked"),
  ("Stop", "done"),
];

/// Codex lifecycle events. Codex has no `Notification`; a `PermissionRequest`
/// is its "blocked", and since nothing else signals the approval was granted,
/// `PostToolUse` flips the pane back to working.
const CODEX_EVENTS: &[(&str, &str)] = &[
  ("SessionStart", "idle"),
  ("UserPromptSubmit", "working"),
  ("PermissionRequest", "blocked"),
  ("PostToolUse", "working"),
  ("Stop", "done"),
];

/// Substring marking a command as ours — used to keep install idempotent and
/// uninstall surgical (it removes only entries whose command contains this).
const MARKER: &str = "agent-status";

impl Agent {
  fn name(self) -> &'static str {
    match self {
      Self::Claude => "claude",
      Self::Codex => "codex",
    }
  }

  fn parse(s: &str) -> Option<Self> {
    AGENTS.into_iter().find(|a| a.name() == s)
  }

  fn events(self) -> &'static [(&'static str, &'static str)] {
    match self {
      Self::Claude => CLAUDE_EVENTS,
      Self::Codex => CODEX_EVENTS,
    }
  }

  fn file_name(self) -> &'static str {
    match self {
      Self::Claude => "settings.json",
      Self::Codex => "hooks.json",
    }
  }

  /// The agent's config directory: the project's `.claude` / `.codex`, or the
  /// user-level one (`CLAUDE_CONFIG_DIR` / `CODEX_HOME`, else under `$HOME`).
  fn dir(self, project: bool) -> Option<PathBuf> {
    let (dotdir, envname) = match self {
      Self::Claude => (".claude", "CLAUDE_CONFIG_DIR"),
      Self::Codex => (".codex", "CODEX_HOME"),
    };
    if project {
      return std::env::current_dir().ok().map(|d| d.join(dotdir));
    }
    std::env::var_os(envname)
      .filter(|v| !v.is_empty())
      .map(PathBuf::from)
      .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(dotdir)))
  }

  fn path(self, project: bool) -> Option<PathBuf> {
    self.dir(project).map(|d| d.join(self.file_name()))
  }

  fn detected(self, project: bool) -> bool {
    self.dir(project).is_some_and(|d| d.is_dir())
  }
}

/// `sinclair agent-status <state> [flags]`: report an agent's state to the running
/// instance. Best-effort — always exits 0 so a hook never fails its agent.
pub fn report(args: &[String]) -> i32 {
  let Some(state) = args.first().filter(|s| !s.starts_with("--")) else {
    eprintln!("usage: sinclair agent-status <working|blocked|done|idle> [--session id] [--name n] [--pane token]");
    return 0;
  };
  let mut session = flag(args, "--session");
  let name = flag(args, "--name").or_else(|| envvar("AGENT"));
  let pane = flag(args, "--pane")
    .or_else(|| envvar("PANE"))
    .and_then(|s| s.parse::<u64>().ok());

  // Hooks pipe a JSON payload on stdin that carries `session_id`; capture it
  // when no explicit --session was given.
  let input = stdin_payload();
  if session.is_none() {
    session = payload_session(&input);
  }
  session = session.or_else(|| envvar("SESSION"));

  let Some(pane) = pane else {
    // Not running inside a Sinclair-spawned pane (no token) — nothing to route.
    return 0;
  };

  if payload_event(&input) == Some("SessionStart") {
    println!("{}", session_context(pane));
  }

  let mut payload = json!({ "pane": pane, "state": state });
  if let Some(s) = session.filter(|s| !s.trim().is_empty()) {
    payload["session"] = json!(s);
  }
  if let Some(n) = name.filter(|n| !n.trim().is_empty()) {
    payload["name"] = json!(n);
  }
  // Ignore transport errors: with no running instance there is nothing to do.
  let _ = crate::ipc::request("report_agent", &payload);
  0
}

/// The piped stdin JSON payload, or `Null` when stdin is a terminal (so a
/// manual invocation never blocks), empty, or not JSON.
fn stdin_payload() -> Value {
  let is_tty = unsafe { libc::isatty(0) } == 1;
  if is_tty {
    return Value::Null;
  }
  let mut buf = String::new();
  if std::io::stdin().read_to_string(&mut buf).is_err() {
    return Value::Null;
  }
  serde_json::from_str(buf.trim()).unwrap_or(Value::Null)
}

fn payload_session(v: &Value) -> Option<String> {
  v.get("session_id")
    .and_then(Value::as_str)
    .map(str::to_string)
}

fn payload_event(v: &Value) -> Option<&str> {
  v.get("hook_event_name").and_then(Value::as_str)
}

/// SessionStart hook output that injects context into the agent. Claude Code
/// and Codex share this shape (`hookSpecificOutput.additionalContext`).
fn session_context(pane: u64) -> Value {
  let text = format!(
    "You are running inside the Sinclair terminal emulator, in pane `pane:{pane}` \
     (SINCLAIR_PANE={pane}). To work with other panes from a shell: `sinclair ls` lists them \
     with ids; `sinclair run -p PANE --wait CMD` runs a command and exits with its status; \
     `sinclair capture -p PANE` prints its screen; `sinclair wait -p PANE --text REGEX` blocks \
     until output matches (e.g. a dev server's ready line). PANE is an id, a tab number, or a \
     title. The `sinclair mcp` server offers the same as MCP tools (ls, run_command, \
     read_screen, wait, events, split, new_tab, notify). Report progress with \
     OSC 7501 (state=working|blocked|done|error) so the user sees it on the tab."
  );
  json!({ "hookSpecificOutput": { "hookEventName": "SessionStart", "additionalContext": text } })
}

/// The value of a `--flag value` pair in `args`, if present.
fn flag(args: &[String], name: &str) -> Option<String> {
  args
    .iter()
    .position(|a| a == name)
    .and_then(|i| args.get(i + 1))
    .cloned()
}

/// Read `SINCLAIR_<name>`, falling back to the pre-rename `PROMPT_<name>` so a
/// session spawned by an older instance still reports. Drop after 1.26.
fn envvar(name: &str) -> Option<String> {
  std::env::var(format!("SINCLAIR_{name}"))
    .or_else(|_| std::env::var(format!("PROMPT_{name}")))
    .ok()
    .filter(|v| !v.is_empty())
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Op {
  Install,
  Uninstall,
  Status,
}

/// How much of our hook set is present in a config.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Status {
  /// None of our entries.
  Missing,
  /// Some entries, or entries whose command differs from what we'd write now.
  Outdated,
  /// Every event carries exactly the command we'd install.
  Current,
}

impl Status {
  fn label(self) -> &'static str {
    match self {
      Self::Missing => "missing",
      Self::Outdated => "outdated",
      Self::Current => "installed (current)",
    }
  }
}

/// `sinclair agent-hooks install|uninstall|status [--agent a]... [--project]`.
/// Returns a process exit code.
pub fn hooks(args: &[String]) -> i32 {
  let project = args.iter().any(|a| a == "--project");
  let sub = args
    .first()
    .map(String::as_str)
    .filter(|s| !s.starts_with("--"))
    .unwrap_or("install");
  let op = match sub {
    "install" | "add" => Op::Install,
    "uninstall" | "remove" => Op::Uninstall,
    "status" => Op::Status,
    other => {
      eprintln!("sinclair agent-hooks: unknown subcommand `{other}` (install|uninstall|status)");
      return 2;
    }
  };

  let mut chosen = Vec::new();
  let mut i = 0;
  while i < args.len() {
    if args[i] == "--agent" {
      let Some(agent) = args.get(i + 1).and_then(|v| Agent::parse(v)) else {
        eprintln!("sinclair agent-hooks: --agent takes `claude` or `codex`");
        return 2;
      };
      if !chosen.contains(&agent) {
        chosen.push(agent);
      }
      i += 1;
    }
    i += 1;
  }
  let explicit = !chosen.is_empty();
  if !explicit {
    chosen = AGENTS.into_iter().filter(|a| a.detected(project)).collect();
    if chosen.is_empty() {
      eprintln!("sinclair: no agent config directory found (looked for Claude Code and Codex); use --agent to target one");
      return 1;
    }
  }

  let exe = current_exe();
  let mut code = 0;
  for agent in chosen {
    match apply(agent, op, project, &exe) {
      Ok(msg) => println!("{msg}"),
      Err(e) => {
        eprintln!("sinclair: {}: {e}", agent.name());
        code = 1;
      }
    }
  }
  code
}

/// Install into every detected agent's user-level config and post the outcome
/// as a desktop notification. Backs the `install_agent_hooks` palette action.
pub fn install_all_notify() {
  let exe = current_exe();
  let mut lines = Vec::new();
  let mut codex_done = false;
  for agent in AGENTS.into_iter().filter(|a| a.detected(false)) {
    match apply(agent, Op::Install, false, &exe) {
      Ok(msg) => {
        codex_done |= agent == Agent::Codex;
        lines.push(msg.lines().next().unwrap_or_default().to_string());
      }
      Err(e) => lines.push(format!("{}: {e}", agent.name())),
    }
  }
  if lines.is_empty() {
    crate::view::post_os_notification(
      "Install Sinclair for Agents",
      "No Claude Code or Codex config directory found.",
    );
    return;
  }
  if codex_done {
    lines.push("Approve the Codex hooks with /hooks inside Codex.".to_string());
  }
  crate::view::post_os_notification("Install Sinclair for Agents", &lines.join("\n"));
}

/// Run `op` against one agent's config file; the Ok text is for the user.
fn apply(agent: Agent, op: Op, project: bool, exe: &str) -> Result<String, String> {
  let path = agent
    .path(project)
    .ok_or_else(|| "could not determine the config path".to_string())?;
  let root = read_config(&path)?;
  let name = agent.name();
  match op {
    Op::Status => {
      let st = status_of(&root, exe, agent.events());
      Ok(format!("{name}: {} ({})", st.label(), path.display()))
    }
    Op::Install => {
      let updated = install_into(root.clone(), exe, agent.events());
      if updated != root || !path.exists() {
        write_config(&path, &updated).map_err(|e| format!("could not write {}: {e}", path.display()))?;
      }
      let mut msg = format!("{name}: installed agent status hooks in {}", path.display());
      if agent == Agent::Codex {
        msg.push_str("\nnote: approve the new hooks with /hooks inside Codex before they run.");
      }
      Ok(msg)
    }
    Op::Uninstall => {
      if !path.exists() {
        return Ok(format!("{name}: nothing to remove ({} not found)", path.display()));
      }
      let updated = uninstall_from(root.clone());
      if updated != root {
        write_config(&path, &updated).map_err(|e| format!("could not write {}: {e}", path.display()))?;
      }
      Ok(format!("{name}: removed agent status hooks from {}", path.display()))
    }
  }
}

/// The current executable path (for a robust hook command), falling back to the
/// bare `sinclair` command name.
fn current_exe() -> String {
  std::env::current_exe()
    .ok()
    .and_then(|p| p.to_str().map(str::to_string))
    .unwrap_or_else(|| "sinclair".to_string())
}

/// The hook command for `state`, e.g. `'/path/to/sinclair' agent-status working`.
/// The executable path is shell-quoted: hooks run through a shell, and an
/// install path containing a space (or quote) would otherwise produce a
/// permanently broken hook in the settings file.
fn command_for(exe: &str, state: &str) -> String {
  format!("{} {MARKER} {state}", sh_quote(exe))
}

/// Single-quote `s` for a `/bin/sh` command line; embedded quotes are closed,
/// escaped, and reopened so the content stays inert.
fn sh_quote(s: &str) -> String {
  format!("'{}'", s.replace('\'', "'\\''"))
}

/// Read a config file into a JSON object: an empty object when the file is
/// absent or blank. A file that exists but is not a JSON object is an error,
/// so a hand-edited file is never silently replaced.
fn read_config(path: &Path) -> Result<Value, String> {
  let bytes = match std::fs::read(path) {
    Ok(b) => b,
    Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(json!({})),
    Err(e) => return Err(format!("could not read {}: {e}", path.display())),
  };
  if bytes.iter().all(u8::is_ascii_whitespace) {
    return Ok(json!({}));
  }
  serde_json::from_slice::<Value>(&bytes)
    .ok()
    .filter(Value::is_object)
    .ok_or_else(|| format!("{} is not a JSON object; fix or remove it first", path.display()))
}

fn write_config(path: &Path, value: &Value) -> std::io::Result<()> {
  if let Some(dir) = path.parent() {
    std::fs::create_dir_all(dir)?;
  }
  let mut json = serde_json::to_string_pretty(value).unwrap_or_else(|_| "{}".to_string());
  json.push('\n');
  std::fs::write(path, json)
}

/// Add our lifecycle hook entries to a config object, idempotently. An event
/// that already carries one of our commands keeps its position and has that
/// command rewritten in place if stale; otherwise a new group is appended to
/// the end of the event's array. Everything else is preserved.
fn install_into(mut root: Value, exe: &str, events: &[(&str, &str)]) -> Value {
  let hooks = root
    .as_object_mut()
    .unwrap()
    .entry("hooks")
    .or_insert_with(|| json!({}));
  if !hooks.is_object() {
    *hooks = json!({});
  }
  let hooks = hooks.as_object_mut().unwrap();
  for (event, state) in events {
    let command = command_for(exe, state);
    let arr = hooks.entry(*event).or_insert_with(|| json!([]));
    if !arr.is_array() {
      *arr = json!([]);
    }
    let arr = arr.as_array_mut().unwrap();
    let mut found = false;
    for entry in arr.iter_mut() {
      let Some(hs) = entry.get_mut("hooks").and_then(Value::as_array_mut) else {
        continue;
      };
      for h in hs.iter_mut().filter(|h| is_ours(h)) {
        found = true;
        h["command"] = json!(command);
      }
    }
    if !found {
      arr.push(json!({ "hooks": [ { "type": "command", "command": command } ] }));
    }
  }
  root
}

/// Remove our lifecycle hook handlers from a config object, dropping any group,
/// event array (and the `hooks` object) left empty. Other hooks are preserved.
fn uninstall_from(mut root: Value) -> Value {
  let Some(obj) = root.as_object_mut() else {
    return root;
  };
  let Some(hooks) = obj.get_mut("hooks").and_then(Value::as_object_mut) else {
    return root;
  };
  let mut empties = Vec::new();
  for (event, entries) in hooks.iter_mut() {
    let Some(arr) = entries.as_array_mut() else {
      continue;
    };
    for entry in arr.iter_mut() {
      if let Some(hs) = entry.get_mut("hooks").and_then(Value::as_array_mut) {
        hs.retain(|h| !is_ours(h));
      }
    }
    arr.retain(|e| {
      !e.get("hooks")
        .and_then(Value::as_array)
        .is_some_and(Vec::is_empty)
    });
    if arr.is_empty() {
      empties.push(event.clone());
    }
  }
  for event in empties {
    hooks.remove(&event);
  }
  if hooks.is_empty() {
    obj.remove("hooks");
  }
  root
}

/// Whether a single hook handler is one of ours (a command string carrying the
/// [`MARKER`]).
fn is_ours(handler: &Value) -> bool {
  handler
    .get("command")
    .and_then(Value::as_str)
    .is_some_and(|c| c.contains(MARKER))
}

/// Compare a config against the hooks `install_into` would write.
fn status_of(root: &Value, exe: &str, events: &[(&str, &str)]) -> Status {
  let mut current = 0;
  let mut any = false;
  for (event, state) in events {
    let want = command_for(exe, state);
    let ours: Vec<&str> = root
      .get("hooks")
      .and_then(|h| h.get(*event))
      .and_then(Value::as_array)
      .into_iter()
      .flatten()
      .filter_map(|e| e.get("hooks").and_then(Value::as_array))
      .flatten()
      .filter(|h| is_ours(h))
      .filter_map(|h| h.get("command").and_then(Value::as_str))
      .collect();
    any |= !ours.is_empty();
    if ours.contains(&want.as_str()) {
      current += 1;
    }
  }
  if current == events.len() {
    Status::Current
  } else if any {
    Status::Outdated
  } else {
    Status::Missing
  }
}

#[cfg(test)]
#[path = "../tests/agenthooks.rs"]
mod tests;
