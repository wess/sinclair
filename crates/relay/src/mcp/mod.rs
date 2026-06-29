use crate::protocol::{err, ok, RpcRequest};
use crate::tools;
use serde_json::{json, Value};

pub const PROTOCOL_VERSION: &str = "2025-06-18";

/// What the transport should do with a parsed request.
pub enum Outcome {
    /// Immediate JSON-RPC response (application/json).
    Now(Value),
    /// A tool call to run, possibly long-lived, streamed over SSE.
    Tool { id: Value, name: String, args: Value },
    /// A notification, no response body, just 202.
    Accepted,
}

/// Route a single JSON-RPC message. Tool calls are handed back to the transport
/// so it can stream the (possibly blocking) result with keepalives.
pub fn route(req: RpcRequest) -> Outcome {
    let id = req.id.clone().unwrap_or(Value::Null);
    match req.method.as_str() {
        "initialize" => Outcome::Now(ok(
            id,
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "relay", "version": env!("CARGO_PKG_VERSION") }
            }),
        )),
        "ping" => Outcome::Now(ok(id, json!({}))),
        "tools/list" => Outcome::Now(ok(id, tools::list())),
        "tools/call" => {
            let name = req
                .params
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let args = req
                .params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            if req.id.is_none() {
                return Outcome::Accepted;
            }
            Outcome::Tool { id, name, args }
        }
        _ if req.id.is_none() => Outcome::Accepted,
        other => Outcome::Now(err(id, -32601, &format!("method not found: {other}"))),
    }
}
