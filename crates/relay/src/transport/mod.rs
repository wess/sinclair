use crate::mcp::{self, Outcome};
use crate::protocol::{err, ok, RpcRequest};
use crate::state::App;
use crate::tools;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::Value;
use std::convert::Infallible;
use std::time::Duration;

const SESSION_HEADER: &str = "mcp-session-id";

/// How often a still-running tool call emits `notifications/progress`, in
/// seconds. Well inside the shortest idle window an MCP client is likely to
/// apply (five minutes for HTTP/SSE in Claude Code), so a parked `wait` keeps
/// its call alive at the protocol level, not just the socket.
pub const PROGRESS_INTERVAL_SECS: u64 = 30;

#[cfg(not(test))]
const PROGRESS_INTERVAL: Duration = Duration::from_secs(PROGRESS_INTERVAL_SECS);
/// Tests run on a live clock — a paused one breaks sqlx's pool timeouts — so
/// the cadence is compressed rather than slept through. The shipped value is
/// [`PROGRESS_INTERVAL_SECS`]; `tools` const-asserts it fits inside a park.
#[cfg(test)]
const PROGRESS_INTERVAL: Duration = Duration::from_millis(20);

/// Single Streamable-HTTP endpoint. Immediate methods reply application/json;
/// tool calls stream over SSE so blocking `wait` calls can be held open with
/// `notifications/progress` frames and transport keepalives.
pub async fn handle(State(app): State<App>, headers: HeaderMap, body: Bytes) -> Response {
    let incoming_session = headers
        .get(SESSION_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);

    let parsed: Result<Value, _> = serde_json::from_slice(&body);
    let Ok(value) = parsed else {
        return Json(err(Value::Null, -32700, "parse error")).into_response();
    };

    // JSON-RPC batching was removed in MCP 2025-06-18; reject arrays outright
    // rather than stalling a batched `wait` for its full timeout with no
    // keepalives on the connection.
    if value.is_array() {
        return Json(err(Value::Null, -32600, "batch requests are not supported")).into_response();
    }

    let Ok(req) = serde_json::from_value::<RpcRequest>(value) else {
        return Json(err(Value::Null, -32600, "invalid request")).into_response();
    };
    let is_initialize = req.method == "initialize";

    match mcp::route(req) {
        Outcome::Accepted => StatusCode::ACCEPTED.into_response(),
        Outcome::Now(v) => {
            let mut resp = Json(v).into_response();
            if is_initialize {
                let session = incoming_session.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
                if let Ok(hv) = HeaderValue::from_str(&session) {
                    resp.headers_mut().insert(SESSION_HEADER, hv);
                }
            }
            resp
        }
        Outcome::Tool {
            id,
            name,
            args,
            progress,
        } => {
            // Every tool call must name its session. Defaulting a missing header
            // to "" used to bucket every such client under one key, so the
            // second agent to `register` silently took over the first one's
            // identity — its `wait` then drained *and acked* the other's inbox.
            // Failing loudly here is the only safe reading of an absent header.
            let Some(session) = incoming_session.filter(|s| !s.is_empty()) else {
                return Json(err(
                    id,
                    -32600,
                    "missing 'mcp-session-id' header — complete `initialize` and send the \
                     returned session id on every request",
                ))
                .into_response();
            };
            stream_tool(app, session, id, name, args, progress)
        }
    }
}

/// Streamable-HTTP session teardown: a client DELETEs `/mcp` with its session
/// header when it shuts down; drop the binding so the sessions map does not
/// hold it forever (abandoned sessions also age out — see `App::bind`).
pub async fn end(State(app): State<App>, headers: HeaderMap) -> StatusCode {
    if let Some(session) = headers.get(SESSION_HEADER).and_then(|v| v.to_str().ok()) {
        app.unbind(session).await;
    }
    StatusCode::NO_CONTENT
}

/// Run a tool call as a one-shot SSE stream: `notifications/progress` frames
/// (plus transport keepalives) while it works, then a single `message` event
/// carrying the JSON-RPC response.
fn stream_tool(
    app: App,
    session: String,
    id: Value,
    name: String,
    args: Value,
    progress: Option<Value>,
) -> Response {
    let stream = async_stream::stream! {
        let call = tools::call(&app, &session, &name, &args);
        tokio::pin!(call);
        // Emit a progress frame every interval until the call resolves. A `wait`
        // parks for minutes; without protocol-level traffic the client aborts it
        // as idle — SSE keepalives are transport comments the JSON-RPC layer
        // never sees. Clients that sent no token get keepalives only, so they
        // still depend on a short `WAIT_MAX`. The spec wants a rising value.
        let mut n = 0u64;
        let reply = loop {
            tokio::select! {
                r = &mut call => break r,
                _ = tokio::time::sleep(PROGRESS_INTERVAL) => {
                    if let Some(token) = &progress {
                        n += 1;
                        let frame = crate::protocol::progress(token, n, "parked on the bus");
                        let data = serde_json::to_string(&frame).unwrap_or_default();
                        yield Ok::<Event, Infallible>(Event::default().event("message").data(data));
                    }
                }
            }
        };
        let response = ok(id, reply.body);
        let data = serde_json::to_string(&response).unwrap_or_default();
        yield Ok::<Event, Infallible>(Event::default().event("message").data(data));
        // Reached only when the stream is polled past the yield, i.e. after the
        // response event was handed to the connection. A client that died first
        // drops the stream and never acks, so its drain is redelivered by the
        // next wait — the at-least-once half of the bus delivery contract.
        if let Some((agent, last)) = reply.ack {
            if let Err(e) = crate::bus::ack(&app, &agent, last).await {
                tracing::warn!("relay: delivery ack for '{agent}' failed: {e}");
            }
        }
    };

    Sse::new(stream)
        .keep_alive(
            KeepAlive::new()
                .interval(Duration::from_secs(15))
                .text("keepalive"),
        )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn app() -> (App, std::path::PathBuf) {
        static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let path =
            std::env::temp_dir().join(format!("relay-transport-{}-{n}.db", std::process::id()));
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

    async fn body_json(resp: Response) -> Value {
        let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    /// Protocol 2025-06-18 removed JSON-RPC batching; an array body
    /// must be rejected with -32600, not processed (or stalled).
    #[tokio::test]
    async fn batch_requests_are_rejected() {
        let (app, path) = app().await;
        let body = Bytes::from(r#"[{"jsonrpc":"2.0","id":1,"method":"ping"}]"#);
        let resp = handle(State(app), HeaderMap::new(), body).await;
        let v = body_json(resp).await;
        assert_eq!(v["error"]["code"], serde_json::json!(-32600));
        cleanup(&path);
    }

    fn call_body(tool: &str, with_progress: bool) -> Bytes {
        let mut params = serde_json::json!({ "name": tool, "arguments": {} });
        if with_progress {
            params["_meta"] = serde_json::json!({ "progressToken": "p1" });
        }
        Bytes::from(
            serde_json::json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":params})
                .to_string(),
        )
    }

    /// Collect every SSE `data:` payload from a streamed tool response.
    async fn sse_frames(resp: Response) -> Vec<Value> {
        let bytes = axum::body::to_bytes(resp.into_body(), 1 << 22)
            .await
            .unwrap();
        String::from_utf8_lossy(&bytes)
            .lines()
            .filter_map(|l| l.strip_prefix("data:"))
            .filter_map(|d| serde_json::from_str(d.trim()).ok())
            .collect()
    }

    /// A tool call without an `mcp-session-id` header must be refused. It used
    /// to default to "", so every such client shared one binding and the last
    /// agent to `register` silently took over the others' identities — its
    /// `wait` then drained and acked someone else's inbox.
    #[tokio::test]
    async fn tool_call_without_a_session_is_refused() {
        let (app, path) = app().await;
        app.bind("", "backend").await;
        let resp = handle(State(app), HeaderMap::new(), call_body("whoami", false)).await;
        let v = body_json(resp).await;
        assert_eq!(v["error"]["code"], serde_json::json!(-32600));
        assert!(
            v["error"]["message"]
                .as_str()
                .unwrap()
                .contains("mcp-session-id"),
            "the error must name the missing header, got {v}"
        );
        cleanup(&path);
    }

    /// An empty session header is the same hazard as an absent one.
    #[tokio::test]
    async fn empty_session_header_is_refused() {
        let (app, path) = app().await;
        let mut headers = HeaderMap::new();
        headers.insert(SESSION_HEADER, HeaderValue::from_static(""));
        let resp = handle(State(app), headers, call_body("whoami", false)).await;
        let v = body_json(resp).await;
        assert_eq!(v["error"]["code"], serde_json::json!(-32600));
        cleanup(&path);
    }

    /// A parked `wait` must emit `notifications/progress` while it blocks.
    /// SSE keepalives are transport-level comments the JSON-RPC layer never
    /// sees, so a client that ages out idle tool calls aborted every park —
    /// which reaches the agent as an error, and an agent that sees an error
    /// stops looping. The cadence is compressed under `cfg(test)`.
    #[tokio::test]
    async fn a_parked_wait_emits_progress_frames() {
        let (app, path) = app().await;
        crate::db::upsert_agent(&app.db, "backend", "backend", "")
            .await
            .unwrap();
        app.bind("s1", "backend").await;
        let mut headers = HeaderMap::new();
        headers.insert(SESSION_HEADER, HeaderValue::from_static("s1"));

        let resp = handle(State(app), headers, call_body("wait", true)).await;
        let frames = sse_frames(resp).await;

        let progress: Vec<&Value> = frames
            .iter()
            .filter(|f| f["method"] == "notifications/progress")
            .collect();
        assert!(
            progress.len() >= 5,
            "a full park at this interval should emit several frames, got {}",
            progress.len()
        );
        assert_eq!(
            progress[0]["params"]["progressToken"],
            serde_json::json!("p1")
        );
        let vals: Vec<u64> = progress
            .iter()
            .map(|f| f["params"]["progress"].as_u64().unwrap())
            .collect();
        assert!(
            vals.windows(2).all(|w| w[1] > w[0]),
            "progress must increase: {vals:?}"
        );
        assert_eq!(
            frames.last().unwrap()["id"],
            serde_json::json!(1),
            "the response comes last"
        );
        cleanup(&path);
    }

    /// No progress token means nothing to echo: the call still works, and the
    /// response is the only frame.
    #[tokio::test]
    async fn no_progress_token_still_answers() {
        let (app, path) = app().await;
        crate::db::upsert_agent(&app.db, "backend", "backend", "")
            .await
            .unwrap();
        app.bind("s1", "backend").await;
        let mut headers = HeaderMap::new();
        headers.insert(SESSION_HEADER, HeaderValue::from_static("s1"));
        let resp = handle(State(app), headers, call_body("whoami", false)).await;
        let frames = sse_frames(resp).await;
        assert_eq!(
            frames.len(),
            1,
            "expected just the response, got {frames:?}"
        );
        assert_eq!(frames[0]["id"], serde_json::json!(1));
        cleanup(&path);
    }

    /// A session DELETE drops the binding; the freed name needs a re-register.
    #[tokio::test]
    async fn delete_unbinds_the_session() {
        let (app, path) = app().await;
        app.bind("s1", "backend").await;
        let mut headers = HeaderMap::new();
        headers.insert(SESSION_HEADER, HeaderValue::from_static("s1"));
        let status = end(State(app.clone()), headers).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        assert_eq!(app.name_of("s1").await, None);
        cleanup(&path);
    }
}
