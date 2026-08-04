//! sysinfo — host stats: load average, and the filesystem backing the focused
//! pane's directory.
//!
//! Runs in the WASM sandbox. It reads the disk through `df` rather than the
//! filesystem capability, so it never needs a path grant — the host runs the
//! program in the right directory and returns the numbers.

wit_bindgen::generate!({
    world: "inspector",
    path: "../../crates/pluginrt/wit",
});

use crate::exports::prompt::plugin::guest::Guest;
use crate::prompt::plugin::host_commands::run_command;
use crate::prompt::plugin::host_process::exec;
use crate::prompt::plugin::types::{CommandTarget, ExecRequest};

use serde_json::{json, Value};

struct Sysinfo;

/// Run a program with no arguments beyond those given, in the focused pane's
/// directory, and return its trimmed stdout ("" on any failure).
fn sh(program: &str, args: &[&str]) -> String {
    let request = ExecRequest {
        program: program.to_string(),
        args: args.iter().map(|a| a.to_string()).collect(),
        cwd: None,
    };
    match exec(&request) {
        Ok(out) if out.status == 0 => out.stdout.trim().to_string(),
        _ => String::new(),
    }
}

/// The load averages out of `uptime`, which reports them on both macOS and
/// Linux — but with different punctuation ("load average:" vs "load averages:"),
/// so match the stem and take what follows.
fn load_average() -> Option<String> {
    let uptime = sh("uptime", &[]);
    let lower = uptime.to_ascii_lowercase();
    let at = lower.find("load average")?;
    let after = &uptime[at..];
    let colon = after.find(':')?;
    Some(after[colon + 1..].trim().to_string()).filter(|s| !s.is_empty())
}

/// size / used / available for the filesystem under the focused directory.
/// `df -h .` reports it; columns 1..3 are stable across the BSD and GNU layouts.
fn disk() -> (Option<String>, Option<String>, Option<String>) {
    let out = sh("df", &["-h", "."]);
    let Some(row) = out.lines().nth(1) else {
        return (None, None, None);
    };
    let cols: Vec<&str> = row.split_whitespace().collect();
    let get = |i: usize| cols.get(i).map(|s| s.to_string());
    (get(1), get(2), get(3))
}

fn panel() -> Value {
    let mut blocks = vec![json!({ "type": "section", "title": "Host" })];

    let host = sh("hostname", &[]);
    if !host.is_empty() {
        blocks.push(json!({ "type": "kv", "key": "host", "value": host }));
    }
    if let Some(load) = load_average() {
        blocks.push(json!({ "type": "kv", "key": "load", "value": load }));
    }

    blocks.push(json!({ "type": "divider" }));
    blocks.push(json!({ "type": "section", "title": "Disk (cwd)" }));
    let (size, used, avail) = disk();
    for (key, value) in [("size", size), ("used", used), ("avail", avail)] {
        blocks.push(json!({ "type": "kv", "key": key, "value": value.unwrap_or("?".into()) }));
    }

    blocks.push(json!({ "type": "divider" }));
    blocks.push(json!({ "type": "button", "id": "refresh", "label": "Refresh", "variant": "subtle" }));
    blocks.push(json!({ "type": "button", "id": "monitor", "label": "Open monitor", "variant": "outline" }));

    json!({ "title": "System", "blocks": blocks })
}

impl Guest for Sysinfo {
    fn init() {}

    fn call_tool(name: String, _params_json: String) -> Result<String, String> {
        if name != "stats" {
            return Err(format!("unknown tool: {name}"));
        }
        // The same host data as the panel, shaped for an agent instead of an eye.
        let (size, used, avail) = disk();
        let host = sh("hostname", &[]);
        Ok(json!({
            "host": if host.is_empty() { Value::Null } else { Value::String(host) },
            "load": load_average(),
            "disk": { "size": size, "used": used, "avail": avail }
        })
        .to_string())
    }

    fn render(_request_json: String) -> String {
        panel().to_string()
    }

    fn on_ui_event(event_json: String) {
        let id = serde_json::from_str::<Value>(&event_json)
            .ok()
            .and_then(|v| v.get("id").and_then(Value::as_str).map(str::to_string))
            .unwrap_or_default();
        if id == "monitor" {
            // Prefer btop, fall back to top, in a split below.
            let _ = run_command(
                "command -v btop >/dev/null && btop || top",
                CommandTarget::SplitDown,
            );
        }
    }
}

export!(Sysinfo);
