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
    #[allow(dead_code)]
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

pub fn now() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
