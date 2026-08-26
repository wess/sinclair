use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A single mesh message as stored and delivered.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Message {
  pub id: i64,
  pub sender: String,
  /// "direct" | "channel" | "broadcast"
  pub kind: String,
  /// agent name (direct) or channel name (channel); null for broadcast.
  pub target: Option<String>,
  pub body: String,
  pub created: i64,
}

/// Incoming JSON-RPC request (or notification when `id` is absent).
#[derive(Debug, Deserialize)]
pub struct RpcRequest {
  pub jsonrpc: Option<String>,
  pub id: Option<Value>,
  pub method: String,
  #[serde(default)]
  pub params: Value,
}

pub fn ok(id: Value, result: Value) -> Value {
  serde_json::json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

pub fn err(id: Value, code: i64, message: &str) -> Value {
  serde_json::json!({
      "jsonrpc": "2.0",
      "id": id,
      "error": { "code": code, "message": message }
  })
}

/// An MCP `notifications/progress` frame for `token`, emitted periodically while
/// a long tool call is still running.
///
/// A parked `wait` sends nothing for minutes. MCP clients age out a tool call
/// that goes quiet, and they count *protocol* traffic: an SSE keepalive comment
/// is transport-level framing the JSON-RPC layer never sees, so it does not
/// count. Without these frames a park is aborted by the client long before the
/// server's own deadline — the agent then sees a tool error and, having no
/// retry instruction, stops looping.
pub fn progress(token: &Value, n: u64, message: &str) -> Value {
  serde_json::json!({
      "jsonrpc": "2.0",
      "method": "notifications/progress",
      "params": {
          "progressToken": token,
          "progress": n,
          "message": message
      }
  })
}

pub fn now() -> i64 {
  use std::time::{SystemTime, UNIX_EPOCH};
  SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map(|d| d.as_secs() as i64)
    .unwrap_or(0)
}
