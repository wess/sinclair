use crate::db;
use crate::spawn;
use crate::state::App;
use serde_json::{json, Value};
use std::sync::atomic::Ordering;
use std::time::Duration;

/// Max time a single `wait` call parks before returning empty (safety net for
/// transport timeouts). The agent's protocol is to call `wait` again.
const WAIT_MAX: Duration = Duration::from_secs(1500); // 25 min

/// JSON-Schema tool list returned by `tools/list`.
pub fn list() -> Value {
    json!({ "tools": [
        tool("register", "Join the mesh under a name. Call this FIRST, once. Returns the current roster.", json!({
            "type": "object",
            "properties": {
                "name": {"type": "string", "description": "Your unique agent name, e.g. 'supervisor' or 'frontend'."},
                "role": {"type": "string", "description": "Short role description."},
                "capabilities": {"type": "string", "description": "Optional free-text of what you can do."}
            },
            "required": ["name"]
        })),
        tool("send", "Send a direct message to one agent by name.", json!({
            "type": "object",
            "properties": {
                "to": {"type": "string", "description": "Recipient agent name."},
                "body": {"type": "string", "description": "Message text."}
            },
            "required": ["to", "body"]
        })),
        tool("post", "Post a message to a channel; all subscribers receive it.", json!({
            "type": "object",
            "properties": {
                "channel": {"type": "string", "description": "Channel name, e.g. 'devops'."},
                "body": {"type": "string"}
            },
            "required": ["channel", "body"]
        })),
        tool("broadcast", "Send a message to every registered agent.", json!({
            "type": "object",
            "properties": { "body": {"type": "string"} },
            "required": ["body"]
        })),
        tool("join", "Subscribe to a channel so you receive its posts.", json!({
            "type": "object",
            "properties": { "channel": {"type": "string"} },
            "required": ["channel"]
        })),
        tool("leave", "Unsubscribe from a channel.", json!({
            "type": "object",
            "properties": { "channel": {"type": "string"} },
            "required": ["channel"]
        })),
        tool("wait", "Block until messages arrive for you, then return them. Call this whenever you have nothing else to do — it is how you stay reachable. Costs nothing while parked.", json!({
            "type": "object", "properties": {}
        })),
        tool("inbox", "Return any pending messages immediately without blocking (may be empty).", json!({
            "type": "object", "properties": {}
        })),
        tool("agents", "List all agents, their roles, and whether they are online.", json!({
            "type": "object", "properties": {}
        })),
        tool("channels", "List channels and their subscriber counts.", json!({
            "type": "object", "properties": {}
        })),
        tool("whoami", "Show your own name, role, and channel subscriptions.", json!({
            "type": "object", "properties": {}
        })),
        tool("spawn", "Spawn a new headless Claude Code worker that joins this mesh, registers, and parks on wait. Use this to grow your team.", json!({
            "type": "object",
            "properties": {
                "name": {"type": "string", "description": "Unique name for the new worker."},
                "role": {"type": "string", "description": "Role, e.g. 'backend dev'."},
                "task": {"type": "string", "description": "Standing focus/instructions for the worker."},
                "channels": {"type": "array", "items": {"type": "string"}, "description": "Channels the worker should join."},
                "model": {"type": "string", "description": "Optional model override, e.g. 'claude-sonnet-4-6'."},
                "cwd": {"type": "string", "description": "Working directory for the worker (defaults to the server's cwd)."},
                "keep_alive": {"type": "boolean", "description": "Respawn the worker if it exits (default true)."}
            },
            "required": ["name"]
        })),
        tool("workers", "List headless workers spawned by this server and their status.", json!({
            "type": "object", "properties": {}
        })),
        tool("stop_worker", "Stop a spawned headless worker by name.", json!({
            "type": "object",
            "properties": { "name": {"type": "string"} },
            "required": ["name"]
        })),
    ]})
}

fn parse_list(v: Option<&Value>) -> Vec<String> {
    match v {
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(|x| x.as_str().map(str::to_string))
            .collect(),
        Some(Value::String(s)) => s
            .split(',')
            .map(str::trim)
            .filter(|x| !x.is_empty())
            .map(str::to_string)
            .collect(),
        _ => Vec::new(),
    }
}

fn tool(name: &str, desc: &str, schema: Value) -> Value {
    json!({ "name": name, "description": desc, "inputSchema": schema })
}

fn text(s: impl Into<String>) -> Value {
    json!({ "content": [{ "type": "text", "text": s.into() }], "isError": false })
}

fn fail(s: impl Into<String>) -> Value {
    json!({ "content": [{ "type": "text", "text": s.into() }], "isError": true })
}

fn arg<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(Value::as_str)
}

/// Dispatch a `tools/call`. Returns a CallToolResult value.
pub async fn call(app: &App, session: &str, name: &str, args: &Value) -> Value {
    if name == "register" {
        let Some(agent) = arg(args, "name") else {
            return fail("register requires a 'name'");
        };
        let role = arg(args, "role").unwrap_or("");
        let caps = arg(args, "capabilities").unwrap_or("");
        if let Err(e) = db::upsert_agent(&app.db, agent, role, caps).await {
            return fail(format!("register failed: {e}"));
        }
        app.bind(session, agent).await;
        let roster = roster_text(app).await;
        return text(format!("registered as '{agent}'.\n{roster}"));
    }

    // Every other tool requires an established identity.
    let Some(me) = app.name_of(session).await else {
        return fail("not registered on this connection — call 'register' first");
    };

    match name {
        "send" => {
            let (Some(to), Some(body)) = (arg(args, "to"), arg(args, "body")) else {
                return fail("send requires 'to' and 'body'");
            };
            match crate::bus::deliver(app, &me, "direct", Some(to), body).await {
                Ok(_) => text(format!("sent to {to}")),
                Err(e) => fail(format!("send failed: {e}")),
            }
        }
        "post" => {
            let (Some(ch), Some(body)) = (arg(args, "channel"), arg(args, "body")) else {
                return fail("post requires 'channel' and 'body'");
            };
            match crate::bus::deliver(app, &me, "channel", Some(ch), body).await {
                Ok(_) => text(format!("posted to #{ch}")),
                Err(e) => fail(format!("post failed: {e}")),
            }
        }
        "broadcast" => {
            let Some(body) = arg(args, "body") else {
                return fail("broadcast requires 'body'");
            };
            match crate::bus::deliver(app, &me, "broadcast", None, body).await {
                Ok(_) => text("broadcast sent"),
                Err(e) => fail(format!("broadcast failed: {e}")),
            }
        }
        "join" => {
            let Some(ch) = arg(args, "channel") else {
                return fail("join requires 'channel'");
            };
            match db::subscribe(&app.db, &me, ch).await {
                Ok(_) => text(format!("joined #{ch}")),
                Err(e) => fail(format!("join failed: {e}")),
            }
        }
        "leave" => {
            let Some(ch) = arg(args, "channel") else {
                return fail("leave requires 'channel'");
            };
            match db::unsubscribe(&app.db, &me, ch).await {
                Ok(_) => text(format!("left #{ch}")),
                Err(e) => fail(format!("leave failed: {e}")),
            }
        }
        "inbox" => drain(app, &me, false).await,
        "wait" => drain(app, &me, true).await,
        "agents" => match db::list_agents(&app.db).await {
            Ok(rows) => {
                let list: Vec<Value> = rows
                    .into_iter()
                    .map(|(n, r, on, c)| json!({"name": n, "role": r, "online": on, "channels": c}))
                    .collect();
                text(serde_json::to_string_pretty(&json!({"agents": list})).unwrap_or_default())
            }
            Err(e) => fail(format!("agents failed: {e}")),
        },
        "channels" => match db::list_channels(&app.db).await {
            Ok(rows) => {
                let list: Vec<Value> = rows
                    .into_iter()
                    .map(|(c, n)| json!({"channel": c, "subscribers": n}))
                    .collect();
                text(serde_json::to_string_pretty(&json!({"channels": list})).unwrap_or_default())
            }
            Err(e) => fail(format!("channels failed: {e}")),
        },
        "whoami" => {
            let subs = db::subs_of(&app.db, &me).await.unwrap_or_default();
            text(serde_json::to_string_pretty(&json!({"name": me, "channels": subs})).unwrap_or_default())
        }
        "spawn" => {
            let Some(wname) = arg(args, "name") else {
                return fail("spawn requires 'name'");
            };
            let role = arg(args, "role").unwrap_or("worker");
            let task = arg(args, "task");
            let model = arg(args, "model");
            let channels = parse_list(args.get("channels"));
            let cwd = arg(args, "cwd").map(str::to_string).unwrap_or_else(|| {
                std::env::current_dir()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|_| ".".into())
            });
            let keep_alive = args.get("keep_alive").and_then(Value::as_bool).unwrap_or(true);

            let mcp_str = match crate::cli::paths::write_mcp_config(&app.endpoint, wname) {
                Ok(p) => p,
                Err(e) => return fail(format!("spawn failed: {e}")),
            };
            let brief = crate::cli::role::resolve(role)
                .map(|r| r.description)
                .unwrap_or_default();
            let prompt = crate::cli::agent::harness_prompt(wname, role, &brief, &channels, task, false);
            let built = match crate::cli::agent::build(&crate::cli::agent::Spec {
                agent: "claude",
                custom: None,
                name: wname,
                role,
                prompt: &prompt,
                mcp_file: &mcp_str,
                url: &app.endpoint,
                headless: true,
                model,
                channels: &channels,
                skip_perms: true,
            }) {
                Ok(b) => b,
                Err(e) => return fail(format!("spawn failed: {e}")),
            };
            let spec = spawn::Spec {
                name: wname.to_string(),
                role: role.to_string(),
                program: built.program,
                args: built.args,
                cwd,
                keep_alive,
            };
            match spawn::launch(app, spec).await {
                Ok(log) => text(format!(
                    "spawned worker '{wname}'. it will register and park on wait. logs: {log}"
                )),
                Err(e) => fail(format!("spawn failed: {e}")),
            }
        }
        "workers" => {
            let map = app.workers.lock().await;
            let mut list = Vec::new();
            for w in map.values() {
                list.push(json!({
                    "name": w.name,
                    "role": w.role,
                    "status": w.status.lock().await.clone(),
                    "pid": w.pid.load(Ordering::SeqCst),
                    "restarts": w.restarts.load(Ordering::SeqCst),
                    "keep_alive": w.keep_alive,
                    "started": w.started,
                    "cwd": w.cwd,
                    "log": w.log,
                }));
            }
            text(serde_json::to_string_pretty(&json!({"workers": list})).unwrap_or_default())
        }
        "stop_worker" => {
            let Some(wname) = arg(args, "name") else {
                return fail("stop_worker requires 'name'");
            };
            if spawn::stop(app, wname).await {
                text(format!("stopping worker '{wname}'"))
            } else {
                fail(format!("no worker named '{wname}'"))
            }
        }
        other => fail(format!("unknown tool '{other}'")),
    }
}

/// Return pending messages. When `block`, park (cheaply) until something arrives
/// or the safety timeout elapses.
async fn drain(app: &App, me: &str, block: bool) -> Value {
    let msgs = crate::bus::await_messages(app, me, block, WAIT_MAX).await;
    let payload = if msgs.is_empty() && block {
        json!({ "messages": [], "note": "no messages yet — call wait again to stay parked" })
    } else {
        json!({ "messages": msgs })
    };
    text(serde_json::to_string_pretty(&payload).unwrap_or_default())
}

async fn roster_text(app: &App) -> String {
    match db::list_agents(&app.db).await {
        Ok(rows) if !rows.is_empty() => {
            let names: Vec<String> = rows
                .into_iter()
                .map(|(n, r, on, _)| format!("  - {n} ({r}){}", if on { "" } else { " [offline]" }))
                .collect();
            format!("online roster:\n{}", names.join("\n"))
        }
        _ => "roster is empty".into(),
    }
}
