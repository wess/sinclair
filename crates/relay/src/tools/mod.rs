use crate::cli::build;
use crate::db;
use crate::spawn;
use crate::state::App;
use serde_json::{json, Value};
use std::sync::atomic::Ordering;
use std::time::Duration;

/// Max time a single `wait` call parks before returning empty, in seconds. The
/// agent's protocol is to call `wait` again.
///
/// This must stay *under* the shortest idle window any MCP client applies to a
/// tool call, because a park the client aborts surfaces to the agent as an
/// error rather than an empty result — and an agent that sees an error stops
/// looping. Claude Code's HTTP/SSE idle default is five minutes, so four leaves
/// headroom even for a client that ignores the `notifications/progress` frames
/// the transport emits. Returning empty is cheap: one tool call per agent per
/// four idle minutes.
pub const WAIT_MAX_SECS: u64 = 240;

/// The shortest idle window a supported MCP client applies to a tool call
/// (Claude Code, HTTP/SSE). [`WAIT_MAX_SECS`] must stay below it.
pub const CLIENT_IDLE_FLOOR_SECS: u64 = 300;

// Enforced at compile time: a park that outlives the client's idle window comes
// back to the agent as a tool *error* rather than an empty result, and an agent
// that sees an error stops looping. Several progress frames must also fit inside
// one park, or a client that honours them still ages the call out.
const _: () = assert!(
    WAIT_MAX_SECS < CLIENT_IDLE_FLOOR_SECS,
    "the park deadline must stay under the shortest client idle window"
);
const _: () = assert!(
    WAIT_MAX_SECS / crate::transport::PROGRESS_INTERVAL_SECS >= 4,
    "a park should emit several progress frames, not just one"
);

#[cfg(not(test))]
const WAIT_MAX: Duration = Duration::from_secs(WAIT_MAX_SECS);
/// Tests park for milliseconds instead of minutes; the shipped value is
/// [`WAIT_MAX_SECS`].
#[cfg(test)]
const WAIT_MAX: Duration = Duration::from_millis(300);

/// A tool response plus an optional delivery acknowledgement `(agent, last_id)`
/// the transport runs once the response has actually reached the client — the
/// read cursor must not advance for a reply that was never received (see the
/// delivery contract in [`crate::bus`]).
pub struct Reply {
    pub body: Value,
    pub ack: Option<(String, i64)>,
}

impl Reply {
    fn plain(body: Value) -> Self {
        Reply { body, ack: None }
    }
}

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
        tool("wait", "Block until messages arrive for you, then return them. Call this whenever you have nothing else to do — it is how you stay reachable, and it costs nothing while parked. After a few idle minutes it returns an empty list instead: that is a normal timeout, not a failure, so just call `wait` again. Do the same if it returns an error.", json!({
            "type": "object", "properties": {}
        })),
        tool("inbox", "Return any pending messages immediately without blocking (may be empty).", json!({
            "type": "object", "properties": {}
        })),
        tool("report_status", "Report your current semantic work state so others (and the UI) can see it at a glance: 'working', 'idle', 'blocked' (waiting on input), or 'done'. A custom label is also allowed. Cheap; call it whenever your state changes.", json!({
            "type": "object",
            "properties": {
                "status": {"type": "string", "description": "One of 'working', 'idle', 'blocked', 'done', or a short custom label."}
            },
            "required": ["status"]
        })),
        tool("wait_status", "Block until another agent reaches one of the given states, then return its status. Use this to coordinate: e.g. wait for a worker to be 'done' or 'blocked'. Returns the current status on timeout — call again to keep waiting.", json!({
            "type": "object",
            "properties": {
                "name": {"type": "string", "description": "The agent to watch."},
                "status": {"type": ["array", "string"], "items": {"type": "string"}, "description": "State(s) to wait for, e.g. 'done' or ['done','blocked']. Empty matches any reported state."}
            },
            "required": ["name"]
        })),
        tool("agents", "List all agents, their roles, whether they are online, and their last-reported status.", json!({
            "type": "object", "properties": {}
        })),
        tool("channels", "List channels and their subscriber counts.", json!({
            "type": "object", "properties": {}
        })),
        tool("whoami", "Show your own name, role, and channel subscriptions.", json!({
            "type": "object", "properties": {}
        })),
        tool("spawn", "Spawn a new headless worker (Claude Code by default) that joins this mesh, registers, and parks on wait. Use this to grow your team. Bounded by a concurrent-worker cap.", json!({
            "type": "object",
            "properties": {
                "name": {"type": "string", "description": "Unique name for the new worker."},
                "role": {"type": "string", "description": "Role, e.g. 'backend dev'."},
                "task": {"type": "string", "description": "Standing focus/instructions for the worker."},
                "agent": {"type": "string", "description": "Agent CLI to run: 'claude' (default) or 'codex'."},
                "tools": {"type": "array", "items": {"type": "string"}, "description": "Pre-granted tool rules (claude --allowedTools), e.g. ['Read','Edit','Bash(git:*)']. Merges with the role's tools."},
                "channels": {"type": "array", "items": {"type": "string"}, "description": "Channels the worker should join."},
                "model": {"type": "string", "description": "Optional model override, e.g. 'claude-sonnet-4-6'."},
                "cwd": {"type": "string", "description": "Working directory for the worker (defaults to the relay state directory)."},
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

/// Dispatch a `tools/call`. Returns the CallToolResult plus, for the drain
/// tools, the delivery ack the transport runs after the response is written.
pub async fn call(app: &App, session: &str, name: &str, args: &Value) -> Reply {
    // The transport rejects an absent session header before reaching here; this
    // guards the other callers. An empty key would be shared by every such
    // client, letting one agent inherit another's identity.
    if session.is_empty() {
        return Reply::plain(fail(
            "no MCP session on this connection — complete `initialize` first",
        ));
    }
    if name == "register" {
        let Some(agent) = arg(args, "name") else {
            return Reply::plain(fail("register requires a 'name'"));
        };
        let role = arg(args, "role").unwrap_or("");
        let caps = arg(args, "capabilities").unwrap_or("");
        if let Err(e) = db::upsert_agent(&app.db, agent, role, caps).await {
            return Reply::plain(fail(format!("register failed: {e}")));
        }
        app.bind(session, agent).await;
        app.bump();
        let roster = roster_text(app).await;
        return Reply::plain(text(format!("registered as '{agent}'.\n{roster}")));
    }

    let Some(me) = app.name_of(session).await else {
        return Reply::plain(fail(
            "not registered on this connection — call 'register' first",
        ));
    };
    // Heartbeat: every tool call refreshes this agent's `last_seen`, so one that
    // stops calling (and is not parked on `wait`) ages out of the live set.
    db::touch(&app.db, &me).await.ok();

    match name {
        "inbox" => drain(app, &me, false).await,
        "wait" => drain(app, &me, true).await,
        other => Reply::plain(dispatch(app, &me, other, args).await),
    }
}

/// The simple tools: everything except `register` (session binding) and the
/// drains (which carry a delivery ack).
async fn dispatch(app: &App, me: &str, name: &str, args: &Value) -> Value {
    match name {
        "send" => {
            let (Some(to), Some(body)) = (arg(args, "to"), arg(args, "body")) else {
                return fail("send requires 'to' and 'body'");
            };
            match crate::bus::deliver(app, me, "direct", Some(to), body).await {
                Ok(_) => text(format!("sent to {to}")),
                Err(e) => fail(format!("send failed: {e}")),
            }
        }
        "post" => {
            let (Some(ch), Some(body)) = (arg(args, "channel"), arg(args, "body")) else {
                return fail("post requires 'channel' and 'body'");
            };
            match crate::bus::deliver(app, me, "channel", Some(ch), body).await {
                Ok(_) => text(format!("posted to #{ch}")),
                Err(e) => fail(format!("post failed: {e}")),
            }
        }
        "broadcast" => {
            let Some(body) = arg(args, "body") else {
                return fail("broadcast requires 'body'");
            };
            match crate::bus::deliver(app, me, "broadcast", None, body).await {
                Ok(_) => text("broadcast sent"),
                Err(e) => fail(format!("broadcast failed: {e}")),
            }
        }
        "join" => {
            let Some(ch) = arg(args, "channel") else {
                return fail("join requires 'channel'");
            };
            match db::subscribe(&app.db, me, ch).await {
                Ok(_) => text(format!("joined #{ch}")),
                Err(e) => fail(format!("join failed: {e}")),
            }
        }
        "leave" => {
            let Some(ch) = arg(args, "channel") else {
                return fail("leave requires 'channel'");
            };
            match db::unsubscribe(&app.db, me, ch).await {
                Ok(_) => text(format!("left #{ch}")),
                Err(e) => fail(format!("leave failed: {e}")),
            }
        }
        "report_status" => {
            let Some(status) = arg(args, "status") else {
                return fail("report_status requires a 'status'");
            };
            match crate::bus::report_status(app, me, status).await {
                Ok(_) => text(format!("status set to '{status}'")),
                Err(e) => fail(format!("report_status failed: {e}")),
            }
        }
        "wait_status" => {
            let Some(target) = arg(args, "name") else {
                return fail("wait_status requires a 'name'");
            };
            let want = parse_list(args.get("status"));
            match crate::bus::await_status(app, target, &want, true, WAIT_MAX).await {
                Ok(status) => text(
                    serde_json::to_string_pretty(&json!({"name": target, "status": status}))
                        .unwrap_or_default(),
                ),
                Err(e) => fail(format!("wait_status failed: {e}")),
            }
        }
        "agents" => match db::list_agents(&app.db).await {
            Ok(rows) => {
                let list: Vec<Value> = rows
                    .into_iter()
                    .map(|(n, r, st, reg, c, ls)| {
                        let online = app.is_live(&n, ls);
                        json!({"name": n, "role": r, "status": st, "online": online, "registered": reg, "channels": c, "last_seen": ls})
                    })
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
            let subs = db::subs_of(&app.db, me).await.unwrap_or_default();
            text(
                serde_json::to_string_pretty(&json!({"name": me, "channels": subs}))
                    .unwrap_or_default(),
            )
        }
        "spawn" => {
            let Some(wname) = arg(args, "name") else {
                return fail("spawn requires 'name'");
            };
            let role = arg(args, "role").unwrap_or("worker");
            let channels = parse_list(args.get("channels"));
            let extra_tools = parse_list(args.get("tools"));
            // Default the worker to the relay state dir, never the daemon's own
            // cwd — a Finder-launched app leaves the daemon in `/`, which no
            // agent can work from.
            let cwd = arg(args, "cwd")
                .map(str::to_string)
                .unwrap_or_else(|| crate::cli::paths::abs_dir().to_string_lossy().into_owned());
            let keep_alive = args
                .get("keep_alive")
                .and_then(Value::as_bool)
                .unwrap_or(true);

            // The same pipeline `relay launch` uses; the differences from the
            // CLI plane are deliberate and spelled out here: a spawned worker is
            // always headless, cannot answer permission prompts (skip_perms),
            // and keeps the caller's project/user MCP servers (no strict_mcp).
            let built = match build::worker(
                &app.endpoint,
                &app.token,
                &build::Options {
                    name: wname,
                    role,
                    // The worker's own root, never the daemon's cwd.
                    role_root: Some(std::path::Path::new(&cwd)),
                    agent: arg(args, "agent"),
                    task: arg(args, "task"),
                    channels: &channels,
                    tools: &extra_tools,
                    model: arg(args, "model"),
                    lead: false,
                    optimize: false,
                    headless: true,
                    skip_perms: true,
                    strict_mcp: false,
                    extra_args: &[],
                    bin: None,
                    custom: None,
                    // A worker spawned over MCP runs on the host, not in the
                    // project sandbox: the daemon is not told which container
                    // a session belongs to. With the sandbox's identity mount
                    // both see the same files; the toolchain is the host's.
                    mcp_dir_as_seen: None,
                },
            ) {
                Ok(b) => b,
                Err(e) => return fail(format!("spawn failed: {e}")),
            };
            let spec = spawn::Spec {
                name: wname.to_string(),
                role: role.to_string(),
                program: built.program,
                args: built.args,
                env: built.env,
                cwd,
                keep_alive,
                session_id: built.session_id,
                resume: false,
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
            if spawn::stop_and_forget(app, wname).await {
                text(format!("stopping worker '{wname}'"))
            } else {
                fail(format!("no worker named '{wname}'"))
            }
        }
        other => fail(format!("unknown tool '{other}'")),
    }
}

/// Return pending messages. When `block`, park (cheaply) until something
/// arrives or the safety timeout elapses. The reply carries the delivery ack —
/// the cursor advances only once the transport has written the response, so a
/// drain lost in flight is redelivered (see [`crate::bus`]).
async fn drain(app: &App, me: &str, block: bool) -> Reply {
    match crate::bus::await_messages(app, me, block, WAIT_MAX).await {
        Ok(msgs) => {
            let ack = msgs.last().map(|m| (me.to_string(), m.id));
            let payload = if msgs.is_empty() && block {
                json!({ "messages": [], "note": "no messages yet — call wait again to stay parked" })
            } else {
                json!({ "messages": msgs })
            };
            Reply {
                body: text(serde_json::to_string_pretty(&payload).unwrap_or_default()),
                ack,
            }
        }
        Err(e) => Reply::plain(fail(format!(
            "{name} failed: {e}",
            name = if block { "wait" } else { "inbox" }
        ))),
    }
}

async fn roster_text(app: &App) -> String {
    match db::list_agents(&app.db).await {
        Ok(rows) if !rows.is_empty() => {
            let names: Vec<String> = rows
                .into_iter()
                .map(|(n, r, _st, _reg, _c, ls)| {
                    let live = app.is_live(&n, ls);
                    format!("  - {n} ({r}){}", if live { "" } else { " [offline]" })
                })
                .collect();
            format!("roster:\n{}", names.join("\n"))
        }
        _ => "roster is empty".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::App;

    async fn app() -> (App, std::path::PathBuf) {
        static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!("relay-tools-{}-{n}.db", std::process::id()));
        let pool = crate::db::open(path.to_str().unwrap()).await.unwrap();
        (
            App::new(pool, "http://127.0.0.1:0".into(), "t".into()),
            path,
        )
    }

    fn cleanup(path: &std::path::Path) {
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(path.with_extension("db-wal"));
        let _ = std::fs::remove_file(path.with_extension("db-shm"));
    }

    /// A tool call with no bound session must be refused outright. Treating an
    /// empty session as valid let one agent inherit another's identity.
    #[tokio::test]
    async fn a_call_without_a_session_is_refused() {
        let (app, path) = app().await;
        let reply = call(&app, "", "whoami", &json!({})).await;
        assert_eq!(reply.body["isError"], json!(true));
        assert!(reply.ack.is_none());
        cleanup(&path);
    }

    /// An unbound (but non-empty) session is still refused, with the message
    /// that tells the agent to register.
    #[tokio::test]
    async fn an_unregistered_session_is_told_to_register() {
        let (app, path) = app().await;
        let reply = call(&app, "s-unknown", "whoami", &json!({})).await;
        assert_eq!(reply.body["isError"], json!(true));
        let text = reply.body["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("register"), "got {text}");
        cleanup(&path);
    }

    /// A `wait` that times out must return an empty list as a *success*, with a
    /// note telling the agent to call again — never an error, which is what
    /// makes an agent stop looping.
    #[tokio::test]
    async fn an_empty_park_is_success_not_an_error() {
        let (app, path) = app().await;
        db::upsert_agent(&app.db, "backend", "backend", "")
            .await
            .unwrap();
        app.bind("s1", "backend").await;
        let reply = call(&app, "s1", "wait", &json!({})).await;
        assert_eq!(
            reply.body["isError"],
            json!(false),
            "an idle park is not a failure"
        );
        let text = reply.body["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("call wait again"), "got {text}");
        assert!(
            reply.ack.is_none(),
            "nothing was delivered, so nothing to ack"
        );
        cleanup(&path);
    }

    /// The delivered path: a queued message comes back and carries the ack the
    /// transport runs once the response has actually been written.
    #[tokio::test]
    async fn a_delivered_message_carries_its_ack() {
        let (app, path) = app().await;
        db::upsert_agent(&app.db, "backend", "backend", "")
            .await
            .unwrap();
        app.bind("s1", "backend").await;
        let id = crate::bus::deliver(&app, "lead", "direct", Some("backend"), "ship it")
            .await
            .unwrap();
        let reply = call(&app, "s1", "wait", &json!({})).await;
        assert_eq!(reply.body["isError"], json!(false));
        assert_eq!(reply.ack, Some(("backend".to_string(), id)));
        cleanup(&path);
    }
}
