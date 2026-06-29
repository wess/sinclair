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

/// Single Streamable-HTTP endpoint. Immediate methods reply application/json;
/// tool calls stream over SSE so blocking `wait` calls can be held open with
/// keepalives.
pub async fn handle(State(app): State<App>, headers: HeaderMap, body: Bytes) -> Response {
    let incoming_session = headers
        .get(SESSION_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);

    let parsed: Result<Value, _> = serde_json::from_slice(&body);
    let Ok(value) = parsed else {
        return Json(err(Value::Null, -32700, "parse error")).into_response();
    };

    if let Value::Array(items) = value {
        let session = incoming_session.unwrap_or_default();
        let mut out = Vec::new();
        for item in items {
            if let Ok(req) = serde_json::from_value::<RpcRequest>(item) {
                match mcp::route(req) {
                    Outcome::Now(v) => out.push(v),
                    Outcome::Tool { id, name, args } => {
                        let result = tools::call(&app, &session, &name, &args).await;
                        out.push(ok(id, result));
                    }
                    Outcome::Accepted => {}
                }
            }
        }
        return Json(Value::Array(out)).into_response();
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
                let session = incoming_session
                    .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
                if let Ok(hv) = HeaderValue::from_str(&session) {
                    resp.headers_mut().insert(SESSION_HEADER, hv);
                }
            }
            resp
        }
        Outcome::Tool { id, name, args } => {
            let session = incoming_session.unwrap_or_default();
            stream_tool(app, session, id, name, args)
        }
    }
}

/// Run a tool call as a one-shot SSE stream: keepalive pings while it works,
/// then a single `message` event carrying the JSON-RPC response.
fn stream_tool(app: App, session: String, id: Value, name: String, args: Value) -> Response {
    let stream = async_stream::stream! {
        let result = tools::call(&app, &session, &name, &args).await;
        let response = ok(id, result);
        let data = serde_json::to_string(&response).unwrap_or_default();
        yield Ok::<Event, Infallible>(Event::default().event("message").data(data));
    };

    Sse::new(stream)
        .keep_alive(
            KeepAlive::new()
                .interval(Duration::from_secs(15))
                .text("keepalive"),
        )
        .into_response()
}
