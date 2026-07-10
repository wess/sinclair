//! Agent status reporting and hook installation.
//!
//! Two process-mode entry points, both invoked as the `prompt` binary:
//!
//! - `sinclair agent-status <state> [--session id] [--name n] [--pane token]` —
//!   an agent (or its lifecycle hook) reports its semantic state back to the
//!   running GUI over the single-instance socket. The pane is identified by the
//!   `SINCLAIR_PANE` token injected into every spawned session's environment, so a
//!   hook needs no arguments beyond the state. When stdin is piped (as hooks
//!   receive it) any `session_id` in the JSON is captured for session resume.
//!
//! - `sinclair agent-hooks install|uninstall [--project]` — write (or remove) a
//!   set of agent lifecycle hooks into a Claude Code `settings.json` so a Claude
//!   Code session running in a pane reports `working` / `blocked` / `done` /
//!   `idle` automatically. Only our own entries are touched.

use std::io::Read;
use std::path::PathBuf;

use serde_json::{json, Value};

/// The agent lifecycle events we hook and the state each maps to. Ordered for a
/// stable settings file.
const HOOK_EVENTS: &[(&str, &str)] = &[
    ("SessionStart", "idle"),
    ("UserPromptSubmit", "working"),
    ("Notification", "blocked"),
    ("Stop", "done"),
];

/// Substring marking a command as ours — used to keep install idempotent and
/// uninstall surgical (it removes only entries whose command contains this).
const MARKER: &str = "agent-status";

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
    // when no explicit --session was given. Skip a terminal so a manual
    // invocation never blocks reading stdin.
    if session.is_none() {
        session = stdin_session();
    }
    session = session.or_else(|| envvar("SESSION"));

    let Some(pane) = pane else {
        // Not running inside a Sinclair-spawned pane (no token) — nothing to route.
        return 0;
    };

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

/// Read a piped stdin JSON payload's `session_id`, if present. Returns `None`
/// when stdin is a terminal, empty, or not the expected shape.
fn stdin_session() -> Option<String> {
    let is_tty = unsafe { libc::isatty(0) } == 1;
    if is_tty {
        return None;
    }
    let mut buf = String::new();
    std::io::stdin().read_to_string(&mut buf).ok()?;
    let v: Value = serde_json::from_str(buf.trim()).ok()?;
    v.get("session_id")
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// The value of a `--flag value` pair in `args`, if present.
fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter()
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

/// `sinclair agent-hooks install|uninstall [--project]`: manage the Claude Code
/// lifecycle hooks. Returns a process exit code.
pub fn hooks(args: &[String]) -> i32 {
    let project = args.iter().any(|a| a == "--project");
    let sub = args.first().map(String::as_str).unwrap_or("install");
    let Some(path) = settings_path(project) else {
        eprintln!("sinclair: could not determine a Claude settings path");
        return 1;
    };
    let root = read_settings(&path);
    let exe = current_exe();
    let (updated, verb) = match sub {
        "install" | "add" => (install_into(root, &exe), "installed"),
        "uninstall" | "remove" => (uninstall_from(root), "removed"),
        other => {
            eprintln!("sinclair agent-hooks: unknown subcommand `{other}` (install|uninstall)");
            return 2;
        }
    };
    match write_settings(&path, &updated) {
        Ok(()) => {
            println!("sinclair: {verb} agent status hooks in {}", path.display());
            0
        }
        Err(e) => {
            eprintln!("sinclair: could not write {}: {e}", path.display());
            1
        }
    }
}

/// The current executable path (for a robust hook command), falling back to the
/// bare `prompt` command name.
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
    format!("{} {MARKER} {state}", crate::relay::sh_quote(exe))
}

fn settings_path(project: bool) -> Option<PathBuf> {
    if project {
        return std::env::current_dir().ok().map(|d| d.join(".claude").join("settings.json"));
    }
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".claude").join("settings.json"))
}

/// Read a settings file into a JSON object, or an empty object when
/// absent/unreadable/not-an-object.
fn read_settings(path: &PathBuf) -> Value {
    std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({}))
}

fn write_settings(path: &PathBuf, value: &Value) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut json = serde_json::to_string_pretty(value).unwrap_or_else(|_| "{}".to_string());
    json.push('\n');
    std::fs::write(path, json)
}

/// Add our lifecycle hook entries to a Claude settings object, idempotently: an
/// event that already carries our command is left untouched. Non-hook settings
/// are preserved.
fn install_into(mut root: Value, exe: &str) -> Value {
    let hooks = root
        .as_object_mut()
        .unwrap()
        .entry("hooks")
        .or_insert_with(|| json!({}));
    if !hooks.is_object() {
        *hooks = json!({});
    }
    let hooks = hooks.as_object_mut().unwrap();
    for (event, state) in HOOK_EVENTS {
        let command = command_for(exe, state);
        let arr = hooks.entry(*event).or_insert_with(|| json!([]));
        if !arr.is_array() {
            *arr = json!([]);
        }
        let arr = arr.as_array_mut().unwrap();
        if arr.iter().any(entry_has_marker) {
            continue;
        }
        arr.push(json!({ "hooks": [ { "type": "command", "command": command } ] }));
    }
    root
}

/// Remove our lifecycle hook entries from a Claude settings object, dropping any
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
        if let Some(arr) = entries.as_array_mut() {
            arr.retain(|e| !entry_has_marker(e));
            if arr.is_empty() {
                empties.push(event.clone());
            }
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

/// Whether a single hook-matcher entry contains one of our commands (a command
/// string carrying the [`MARKER`]).
fn entry_has_marker(entry: &Value) -> bool {
    entry
        .get("hooks")
        .and_then(Value::as_array)
        .map(|hs| {
            hs.iter().any(|h| {
                h.get("command")
                    .and_then(Value::as_str)
                    .is_some_and(|c| c.contains(MARKER))
            })
        })
        .unwrap_or(false)
}

#[cfg(test)]
#[path = "../tests/agenthooks.rs"]
mod tests;
