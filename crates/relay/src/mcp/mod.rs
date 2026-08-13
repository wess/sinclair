use crate::protocol::{err, ok, RpcRequest};
use crate::tools;
use serde_json::{json, Value};

pub const PROTOCOL_VERSION: &str = "2025-06-18";

/// What the transport should do with a parsed request.
pub enum Outcome {
    /// Immediate JSON-RPC response (application/json).
    Now(Value),
    /// A tool call to run, possibly long-lived, streamed over SSE. `progress`
    /// carries the client's `_meta.progressToken` when it supplied one, so the
    /// transport can keep the call alive with `notifications/progress`.
    Tool {
        id: Value,
        name: String,
        args: Value,
        progress: Option<Value>,
    },
    /// A notification, no response body, just 202.
    Accepted,
}

/// Route a single JSON-RPC message. Tool calls are handed back to the transport
/// so it can stream the (possibly blocking) result with keepalives.
pub fn route(req: RpcRequest) -> Outcome {
    let id = req.id.clone().unwrap_or(Value::Null);
    if req.jsonrpc.as_deref() != Some("2.0") {
        return Outcome::Now(err(id, -32600, "invalid request: expected jsonrpc \"2.0\""));
    }
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
            // Clients that want progress updates pass a token in `_meta`; a
            // parked `wait` echoes it back periodically so the call is not aged
            // out as idle (see `protocol::progress`).
            let progress = req
                .params
                .get("_meta")
                .and_then(|m| m.get("progressToken"))
                .cloned();
            Outcome::Tool {
                id,
                name,
                args,
                progress,
            }
        }
        _ if req.id.is_none() => Outcome::Accepted,
        other => Outcome::Now(err(id, -32601, &format!("method not found: {other}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(jsonrpc: Option<&str>, method: &str) -> RpcRequest {
        RpcRequest {
            jsonrpc: jsonrpc.map(str::to_string),
            id: Some(json!(1)),
            method: method.into(),
            params: Value::Null,
        }
    }

    /// `jsonrpc` was carried but never validated; a request without
    /// the `"2.0"` marker must be rejected with -32600 per spec.
    #[test]
    fn missing_or_wrong_jsonrpc_is_invalid() {
        for bad in [None, Some("1.0"), Some("")] {
            let Outcome::Now(v) = route(req(bad, "ping")) else {
                panic!("expected an immediate error for jsonrpc {bad:?}");
            };
            assert_eq!(v["error"]["code"], json!(-32600));
        }
    }

    fn tool_call(meta: Option<Value>) -> RpcRequest {
        let mut params = json!({ "name": "wait", "arguments": {} });
        if let Some(m) = meta {
            params["_meta"] = m;
        }
        RpcRequest {
            jsonrpc: Some("2.0".into()),
            id: Some(json!(1)),
            method: "tools/call".into(),
            params,
        }
    }

    /// The client's progress token has to reach the transport, which echoes it
    /// while a `wait` is parked. Losing it here means no progress frames and a
    /// park the client aborts as idle.
    #[test]
    fn progress_token_is_carried_through() {
        let Outcome::Tool { progress, .. } = route(tool_call(Some(json!({"progressToken": 7}))))
        else {
            panic!("a tool call should route as Outcome::Tool");
        };
        assert_eq!(progress, Some(json!(7)));
    }

    /// A client that asks for no progress updates is still routed normally.
    #[test]
    fn a_missing_progress_token_is_none() {
        let Outcome::Tool { progress, .. } = route(tool_call(None)) else {
            panic!("a tool call should route as Outcome::Tool");
        };
        assert_eq!(progress, None);
        let Outcome::Tool { progress, .. } = route(tool_call(Some(json!({})))) else {
            panic!("a tool call should route as Outcome::Tool");
        };
        assert_eq!(progress, None, "an empty _meta carries no token");
    }

    #[test]
    fn valid_jsonrpc_still_routes() {
        let Outcome::Now(v) = route(req(Some("2.0"), "ping")) else {
            panic!("ping should answer immediately");
        };
        assert!(v.get("error").is_none());
    }
}
