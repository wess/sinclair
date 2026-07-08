//! Windows IPC stubs.
//!
//! The single-instance control channel — quick-terminal summon and the MCP
//! bridge — is a Unix-domain socket. A Windows named-pipe transport is not yet
//! implemented, so these degrade gracefully: the client calls report the
//! feature is unavailable and the server never binds.

use gpui::App;
use serde_json::Value;

/// The value injected as `PROMPT_SOCKET` into spawned sessions. Empty on
/// Windows: there is no control channel for external tooling to reach yet.
pub fn socket_env() -> String {
    String::new()
}

/// Client: ask a running instance to toggle the quick terminal.
pub fn send_toggle() -> bool {
    eprintln!("prompt: quick-terminal summon is not supported on Windows yet");
    false
}

/// Client: send one op to the running instance. Always fails on Windows — there
/// is no transport to reach it.
pub fn request(_op: &str, _args: &Value) -> Result<Value, String> {
    Err("prompt: IPC is not supported on Windows yet".to_string())
}

/// Server: own the control channel. A no-op on Windows.
pub fn listen(_cx: &mut App) {}

/// Dev-only CLI (`prompt ipc <op>`): unavailable on Windows.
#[cfg(debug_assertions)]
pub fn run_cli(_args: &[String]) -> i32 {
    eprintln!("prompt ipc: not supported on Windows");
    2
}
