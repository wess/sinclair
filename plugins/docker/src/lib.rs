//! docker — a panel over the containers on this machine.
//!
//! Runs in the WASM sandbox: it reaches the docker CLI only through the gated
//! `host-process` interface, and hands anything worth scrolling to the terminal
//! through `host-commands`.

wit_bindgen::generate!({
    world: "inspector",
    path: "../../crates/pluginrt/wit",
});

use crate::exports::prompt::plugin::guest::Guest;
use crate::prompt::plugin::host_commands::run_command;
use crate::prompt::plugin::host_process::exec;
use crate::prompt::plugin::types::{CommandTarget, ExecRequest};

use serde_json::{json, Value};

/// How many containers the panel lists before it stops.
const MAX_ROWS: usize = 30;

struct Docker;

/// Run `docker <args>`. A host-level failure reads as a non-zero status, so a
/// machine without docker installed lands on the same "not available" path as
/// one where the daemon is down.
fn docker(args: &[&str]) -> (i32, String) {
    let request = ExecRequest {
        program: "docker".to_string(),
        args: args.iter().map(|a| a.to_string()).collect(),
        cwd: None,
    };
    match exec(&request) {
        Ok(out) => (out.status, out.stdout),
        Err(_) => (-1, String::new()),
    }
}

/// Whether a docker daemon is actually answering — not just whether the binary
/// exists, which is why this asks for the *server* version.
fn daemon_is_up() -> bool {
    docker(&["version", "--format", "{{.Server.Version}}"]).0 == 0
}

fn panel() -> Value {
    if !daemon_is_up() {
        return json!({
            "title": "Docker",
            "blocks": [{
                "type": "text",
                "text": "Docker is not available or not running.",
                "dimmed": true
            }]
        });
    }

    // Tab-separated, so a container name containing spaces stays one field.
    let (_, out) = docker(&["ps", "-a", "--format", "{{.Names}}\t{{.Status}}\t{{.Image}}"]);
    let lines: Vec<&str> = out.trim().split('\n').filter(|l| !l.is_empty()).collect();

    let mut blocks = vec![json!({
        "type": "section",
        "title": format!("Containers ({})", lines.len())
    })];

    if lines.is_empty() {
        blocks.push(json!({ "type": "text", "text": "No containers.", "dimmed": true }));
    } else {
        for line in lines.iter().take(MAX_ROWS) {
            let mut fields = line.split('\t');
            let name = fields.next().unwrap_or("");
            let status = fields.next().unwrap_or("");
            let image = fields.next().unwrap_or("");
            let up = status.to_ascii_lowercase().starts_with("up");
            blocks.push(json!({
                "type": "row",
                "children": [
                    {
                        "type": "badge",
                        "label": if up { "up" } else { "off" },
                        "color": if up { "green" } else { "gray" }
                    },
                    { "type": "text", "text": format!("{name}  \u{b7}  {image}") }
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
    blocks.push(json!({ "type": "button", "id": "refresh", "label": "Refresh", "variant": "subtle" }));
    blocks.push(json!({ "type": "button", "id": "stats", "label": "Stats in terminal", "variant": "outline" }));
    blocks.push(json!({ "type": "button", "id": "prune", "label": "Prune stopped", "variant": "filled" }));

    json!({ "title": "Docker", "blocks": blocks })
}

impl Guest for Docker {
    fn init() {}

    fn call_tool(name: String, _params_json: String) -> Result<String, String> {
        match name.as_str() {
            "containers" => {
                if !daemon_is_up() {
                    return Err("docker is not available or not running".to_string());
                }
                let (_, out) =
                    docker(&["ps", "-a", "--format", "{{.Names}}\t{{.Status}}\t{{.Image}}"]);
                let entries: Vec<Value> = out
                    .trim()
                    .split('\n')
                    .filter(|l| !l.is_empty())
                    .map(|line| {
                        let mut f = line.split('\t');
                        let name = f.next().unwrap_or("");
                        let status = f.next().unwrap_or("");
                        let image = f.next().unwrap_or("");
                        json!({
                            "name": name,
                            "status": status,
                            "image": image,
                            "running": status.to_ascii_lowercase().starts_with("up")
                        })
                    })
                    .collect();
                Ok(json!({ "containers": entries }).to_string())
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
            // `docker stats` is a live full-screen view; it belongs in a tab.
            "stats" => {
                let _ = run_command("docker stats", CommandTarget::Tab);
            }
            "prune" => {
                docker(&["container", "prune", "-f"]);
            }
            _ => {}
        }
    }
}

export!(Docker);
