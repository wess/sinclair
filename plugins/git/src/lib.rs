//! git — a panel over the repository the focused pane is sitting in.
//!
//! Runs in the WASM sandbox: it holds no privilege of its own and reaches git
//! only through the gated `host-process` interface, which runs the program in
//! the focused pane's directory and hands back its output.

wit_bindgen::generate!({
    world: "inspector",
    path: "../../crates/pluginrt/wit",
});

use crate::exports::prompt::plugin::guest::Guest;
use crate::prompt::plugin::host_commands::run_command;
use crate::prompt::plugin::host_process::exec;
use crate::prompt::plugin::types::{CommandTarget, ExecRequest};

use serde_json::{json, Value};

/// How many changed paths the panel lists before summarising the rest.
const MAX_ROWS: usize = 20;

struct Git;

/// Run `git <args>` in the focused pane's directory. Returns the exit status and
/// stdout; a host-level failure (git missing) reads as a non-zero status so the
/// panel degrades to "not a repository" instead of blanking.
fn git(args: &[&str]) -> (i32, String) {
    let request = ExecRequest {
        program: "git".to_string(),
        args: args.iter().map(|a| a.to_string()).collect(),
        // No cwd: the host runs it where the user is.
        cwd: None,
    };
    match exec(&request) {
        Ok(out) => (out.status, out.stdout),
        Err(_) => (-1, String::new()),
    }
}

/// `git` with the output trimmed — for single-value queries.
fn git_line(args: &[&str]) -> (bool, String) {
    let (status, out) = git(args);
    (status == 0, out.trim().to_string())
}

fn is_repo() -> bool {
    git_line(&["rev-parse", "--is-inside-work-tree"]).0
}

/// A porcelain status code as a short badge and a colour.
fn badge(code: &str) -> (&'static str, &'static str) {
    let c = code.trim();
    if c == "??" {
        return ("?", "gray");
    }
    if c.contains('M') {
        return ("M", "yellow");
    }
    if c.contains('A') {
        return ("A", "green");
    }
    if c.contains('D') {
        return ("D", "red");
    }
    if c.contains('R') {
        return ("R", "teal");
    }
    ("\u{2022}", "gray")
}

fn panel() -> Value {
    if !is_repo() {
        return json!({
            "title": "Git",
            "blocks": [{ "type": "text", "text": "Not a git repository.", "dimmed": true }]
        });
    }

    let branch = match git_line(&["rev-parse", "--abbrev-ref", "HEAD"]) {
        (true, b) if !b.is_empty() => b,
        _ => "(detached)".to_string(),
    };

    let mut blocks = vec![
        json!({ "type": "section", "title": "Branch" }),
        json!({ "type": "kv", "key": "branch", "value": branch }),
    ];

    // Ahead/behind against the upstream, when one is configured.
    let (ok, counts) = git_line(&["rev-list", "--left-right", "--count", "@{u}...HEAD"]);
    if ok && !counts.is_empty() {
        let mut parts = counts.split_whitespace();
        if let (Some(behind), Some(ahead)) = (parts.next(), parts.next()) {
            blocks.push(json!({
                "type": "kv",
                "key": "ahead / behind",
                "value": format!("{ahead} / {behind}")
            }));
        }
    }

    // Deliberately untrimmed: the leading column is significant (a space means
    // "unstaged"), so trimming would shift the first row's path by one.
    let (_, status_raw) = git(&["status", "--porcelain"]);
    let lines: Vec<&str> = status_raw.split('\n').filter(|l| !l.is_empty()).collect();

    blocks.push(json!({ "type": "divider" }));
    blocks.push(json!({ "type": "section", "title": format!("Changes ({})", lines.len()) }));

    if lines.is_empty() {
        blocks.push(json!({ "type": "text", "text": "Working tree clean.", "dimmed": true }));
    } else {
        for line in lines.iter().take(MAX_ROWS) {
            let (code, path) = line.split_at(line.len().min(2));
            let (label, color) = badge(code);
            blocks.push(json!({
                "type": "row",
                "children": [
                    { "type": "badge", "label": label, "color": color },
                    { "type": "text", "text": path.trim_start() }
                ]
            }));
        }
        if lines.len() > MAX_ROWS {
            blocks.push(json!({
                "type": "text",
                "text": format!("\u{2026} and {} more", lines.len() - MAX_ROWS),
                "dimmed": true
            }));
        }
    }

    blocks.push(json!({ "type": "divider" }));
    blocks.push(json!({ "type": "button", "id": "stage_all", "label": "Stage all", "variant": "filled" }));
    blocks.push(json!({ "type": "button", "id": "fetch", "label": "Fetch" }));
    blocks.push(json!({ "type": "button", "id": "refresh", "label": "Refresh", "variant": "subtle" }));
    blocks.push(json!({ "type": "button", "id": "log", "label": "Log in terminal", "variant": "outline" }));

    json!({ "title": format!("Git \u{b7} {branch}"), "blocks": blocks })
}

impl Guest for Git {
    fn init() {}

    fn call_tool(name: String, params_json: String) -> Result<String, String> {
        match name.as_str() {
            // The panel's data, for an agent rather than the eye.
            "status" => {
                if !is_repo() {
                    return Err("not a git repository".to_string());
                }
                let (_, raw) = git(&["status", "--porcelain"]);
                let entries: Vec<Value> = raw
                    .split('\n')
                    .filter(|l| !l.is_empty())
                    .map(|line| {
                        let (code, path) = line.split_at(line.len().min(2));
                        json!({ "code": code.trim(), "path": path.trim_start() })
                    })
                    .collect();
                let branch = git_line(&["rev-parse", "--abbrev-ref", "HEAD"]).1;
                Ok(json!({ "branch": branch, "changes": entries }).to_string())
            }
            // Blame a file and return one entry per line.
            "blame" => {
                let params: Value =
                    serde_json::from_str(&params_json).map_err(|e| e.to_string())?;
                let path = params
                    .get("path")
                    .and_then(Value::as_str)
                    .ok_or("blame requires a `path`")?;
                let (status, out) = git(&["blame", "--line-porcelain", "--", path]);
                if status != 0 {
                    return Err(format!("git blame failed for {path}"));
                }
                let mut entries = Vec::new();
                let mut author = String::new();
                for line in out.lines() {
                    if let Some(rest) = line.strip_prefix("author ") {
                        author = rest.to_string();
                    } else if let Some(rest) = line.strip_prefix('\t') {
                        entries.push(json!({ "author": author, "line": rest }));
                    }
                }
                Ok(json!({ "path": path, "lines": entries }).to_string())
            }
            other => Err(format!("unknown tool: {other}")),
        }
    }

    fn render(_request_json: String) -> String {
        panel().to_string()
    }

    fn on_ui_event(event_json: String) {
        let id = serde_json::from_str::<Value>(&event_json)
            .ok()
            .and_then(|v| v.get("id").and_then(Value::as_str).map(str::to_string))
            .unwrap_or_default();
        match id.as_str() {
            "stage_all" => {
                git(&["add", "-A"]);
            }
            "fetch" => {
                git(&["fetch", "--all", "--prune"]);
            }
            // Hand the graph to the terminal rather than cramming it in the
            // panel — it's something you want to scroll and copy from.
            "log" => {
                let _ = run_command(
                    "git log --oneline --graph --decorate -20",
                    CommandTarget::Pane,
                );
            }
            // "refresh" and anything unknown: the host re-renders after the
            // event, which is the whole action.
            _ => {}
        }
    }
}

export!(Git);
