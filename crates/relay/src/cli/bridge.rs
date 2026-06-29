//! Bridge runtime: makes a non-agentic backend (Ollama) a mesh participant.
//! Relay drives the loop, register, wait for a message, run an agentic
//! tool-using turn against the model, report back, over the plain-HTTP control
//! plane (no MCP client needed).

use super::{http, AgentArgs};
use anyhow::{anyhow, Result};
use serde_json::{json, Value};

const MAX_TOOL_STEPS: usize = 8;

pub fn run(a: AgentArgs) -> Result<()> {
    if a.backend != "ollama" {
        return Err(anyhow!("unknown bridge backend '{}'", a.backend));
    }
    let relay = control_addr(&a.url);
    let ollama = host_port(&a.ollama);

    let reg = json!({ "name": a.name, "role": a.role, "channels": a.channels }).to_string();
    http::post(&relay, "/control/register", &reg)?;
    println!(
        "relay: ollama agent '{}' registered ({} via {})",
        a.name, a.model, a.ollama
    );

    loop {
        let resp = http::post(
            &relay,
            "/control/wait",
            &json!({ "name": a.name, "block": true }).to_string(),
        )?;
        let v: Value = serde_json::from_str(&resp).unwrap_or(Value::Null);
        for m in v["messages"].as_array().cloned().unwrap_or_default() {
            let sender = m["sender"].as_str().unwrap_or("someone").to_string();
            let body = m["body"].as_str().unwrap_or("").to_string();
            if let Err(e) = turn(&a, &relay, &ollama, &sender, &body) {
                eprintln!("relay: ollama turn failed: {e}");
            }
        }
    }
}

/// One agentic turn: feed the message to the model, run any tool calls it makes,
/// then report its final answer back to the sender.
fn turn(a: &AgentArgs, relay: &str, ollama: &str, sender: &str, body: &str) -> Result<()> {
    let mut messages = vec![
        json!({ "role": "system", "content": system_prompt(a) }),
        json!({ "role": "user", "content": format!("Message from {sender}: {body}") }),
    ];

    for _ in 0..MAX_TOOL_STEPS {
        let req = json!({
            "model": a.model,
            "messages": messages,
            "stream": false,
            "tools": tool_defs(),
        })
        .to_string();
        let resp = http::post_timeout(ollama, "/api/chat", &req, 300)
            .map_err(|e| anyhow!("ollama unreachable at {} ({e})", a.ollama))?;
        let v: Value = serde_json::from_str(&resp)?;
        let msg = &v["message"];
        let calls = msg["tool_calls"].as_array().cloned().unwrap_or_default();

        if calls.is_empty() {
            let content = msg["content"].as_str().unwrap_or("").trim().to_string();
            if !content.is_empty() {
                let _ = http::post(
                    relay,
                    "/control/send",
                    &json!({ "from": a.name, "kind": "direct", "target": sender, "body": content })
                        .to_string(),
                );
            }
            return Ok(());
        }

        messages.push(msg.clone());
        for tc in calls {
            let name = tc["function"]["name"].as_str().unwrap_or("");
            let args = &tc["function"]["arguments"];
            let result = exec_tool(a, relay, name, args);
            messages.push(json!({ "role": "tool", "content": result }));
        }
    }
    Ok(())
}

fn exec_tool(a: &AgentArgs, relay: &str, name: &str, args: &Value) -> String {
    let body = args["body"].as_str().unwrap_or("");
    let payload = match name {
        "send" => json!({ "from": a.name, "kind": "direct", "target": args["to"].as_str(), "body": body }),
        "post" => json!({ "from": a.name, "kind": "channel", "target": args["channel"].as_str(), "body": body }),
        "broadcast" => json!({ "from": a.name, "kind": "broadcast", "body": body }),
        other => return format!("unknown tool '{other}'"),
    };
    match http::post(relay, "/control/send", &payload.to_string()) {
        Ok(_) => "ok".to_string(),
        Err(e) => format!("failed: {e}"),
    }
}

fn tool_defs() -> Value {
    json!([
        tool("send", "Send a direct message to one agent by name.", json!({
            "type": "object",
            "properties": { "to": {"type":"string"}, "body": {"type":"string"} },
            "required": ["to", "body"]
        })),
        tool("post", "Post a message to a channel.", json!({
            "type": "object",
            "properties": { "channel": {"type":"string"}, "body": {"type":"string"} },
            "required": ["channel", "body"]
        })),
        tool("broadcast", "Send a message to every agent.", json!({
            "type": "object",
            "properties": { "body": {"type":"string"} },
            "required": ["body"]
        })),
    ])
}

fn tool(name: &str, desc: &str, params: Value) -> Value {
    json!({ "type": "function", "function": { "name": name, "description": desc, "parameters": params } })
}

fn system_prompt(a: &AgentArgs) -> String {
    let mut s = String::new();
    if !a.system.trim().is_empty() {
        s.push_str(a.system.trim());
        s.push_str("\n\n");
    }
    s.push_str(
        "You are an AI agent in the Relay mesh. Another agent has messaged you. \
         Do the requested work and produce a clear, concise answer — your final \
         reply (a message with no tool calls) is sent back to whoever asked. Use \
         the send/post/broadcast tools only to coordinate with other agents.",
    );
    s
}

/// "http://127.0.0.1:7777/mcp" -> "127.0.0.1:7777".
fn control_addr(url: &str) -> String {
    url.trim_start_matches("http://")
        .trim_start_matches("https://")
        .trim_end_matches("/mcp")
        .trim_end_matches('/')
        .to_string()
}

/// "http://127.0.0.1:11434" -> "127.0.0.1:11434".
fn host_port(url: &str) -> String {
    url.trim_start_matches("http://")
        .trim_start_matches("https://")
        .trim_end_matches('/')
        .to_string()
}
