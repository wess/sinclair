//! WASM plugin runtime for the app: instantiates a plugin's component through
//! [`pluginrt`], keeps it resident, and calls its tools. The [`AppHost`] the
//! plugin sees ([`SocketHost`]) routes terminal operations to the running GUI
//! over the single-instance socket ([`crate::ipc`]) — the same path built-in
//! MCP tools use — and implements storage / logging / filesystem directly.
//!
//! This is the Stage-2 tool path (invoked from `mcpbridge`). Panels, triggers,
//! and webviews get their own GUI-side runner in later stages.

use pluginrt::{AppHost, CommandTarget, HttpRequest, HttpResponse, LogLevel, Runtime};
use serde_json::{json, Value};

/// Wraps [`pluginrt::Runtime`] with the app's host and plugin-loading, so the
/// app never touches `wasmtime` directly.
pub struct WasmRuntime {
    rt: Runtime,
}

impl WasmRuntime {
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            rt: Runtime::new().map_err(|e| e.to_string())?,
        })
    }

    /// Call a tool on `plugin`, instantiating the component on first use. Returns
    /// the tool's JSON result (parsed if the plugin returned JSON, else a string).
    pub fn call_tool(
        &mut self,
        plugin: &plugin::Plugin,
        tool_id: &str,
        args: &Value,
    ) -> Result<Value, String> {
        if !self.rt.is_resident(&plugin.id) {
            let wasm_rel = plugin
                .runtime
                .as_ref()
                .and_then(|r| r.wasm.as_deref())
                .ok_or("plugin has no wasm module")?;
            let wasm_path = plugin.path.join(wasm_rel);
            let wasm = std::fs::read(&wasm_path)
                .map_err(|e| format!("read {}: {e}", wasm_path.display()))?;
            let host = Box::new(SocketHost::new(plugin.id.clone(), plugin.path.clone()));
            // Enforce consent: link only the capabilities the user granted.
            let caps = plugin::Installed::load()
                .effective_capabilities(&plugin.id, &plugin.capabilities);
            self.rt
                .ensure(&plugin.id, &wasm, &caps, host)
                .map_err(|e| e.to_string())?;
        }
        let params = serde_json::to_string(args).map_err(|e| e.to_string())?;
        match self.rt.call_tool(&plugin.id, tool_id, &params) {
            Ok(Ok(out)) => Ok(serde_json::from_str(&out).unwrap_or(Value::String(out))),
            Ok(Err(message)) => Err(message),
            Err(trap) => Err(format!("plugin trapped: {trap}")),
        }
    }
}

/// The host a WASM plugin sees. Terminal operations route to the GUI over the
/// socket; storage / logging / filesystem are handled here.
struct SocketHost {
    plugin_id: String,
    /// The plugin's own directory — the sandbox root for filesystem access.
    root: std::path::PathBuf,
}

impl SocketHost {
    fn new(plugin_id: String, root: std::path::PathBuf) -> Self {
        Self { plugin_id, root }
    }
}

/// Per-plugin key/value store directory.
fn storage_dir(id: &str) -> Option<std::path::PathBuf> {
    Some(plugin::defaultdir()?.parent()?.join("pluginstorage").join(id))
}

/// Read a per-plugin storage value.
pub(crate) fn storage_read(id: &str, key: &str) -> Option<String> {
    std::fs::read_to_string(storage_dir(id)?.join(sanitize(key))).ok()
}

/// Write a per-plugin storage value (best effort).
pub(crate) fn storage_write(id: &str, key: &str, value: &str) {
    if let Some(dir) = storage_dir(id) {
        if std::fs::create_dir_all(&dir).is_ok() {
            let _ = std::fs::write(dir.join(sanitize(key)), value);
        }
    }
}

/// Resolve `path` inside `root`, rejecting escapes (`..`, absolute).
pub(crate) fn scoped(root: &std::path::Path, path: &str) -> Result<std::path::PathBuf, String> {
    let p = std::path::Path::new(path);
    if p.is_absolute() || p.components().any(|c| c == std::path::Component::ParentDir) {
        return Err("path escapes the plugin directory".into());
    }
    Ok(root.join(p))
}

/// Map a WIT command target to the `run_command` op's `target` token.
pub(crate) fn target_token(target: CommandTarget) -> &'static str {
    match target {
        CommandTarget::Pane => "pane",
        CommandTarget::Tab => "tab",
        CommandTarget::SplitRight => "split_right",
        CommandTarget::SplitDown => "split_down",
    }
}

/// Best-effort string out of a socket reply (the read_screen op's shape).
fn as_text(value: Value) -> String {
    match value {
        Value::String(s) => s,
        Value::Object(ref map) => map
            .get("screen")
            .or_else(|| map.get("text"))
            .or_else(|| map.get("content"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| value.to_string()),
        other => other.to_string(),
    }
}

impl AppHost for SocketHost {
    fn log(&mut self, level: LogLevel, message: String) {
        eprintln!("[plugin {}] {level:?}: {message}", self.plugin_id);
    }

    fn storage_get(&mut self, key: String) -> Option<String> {
        storage_read(&self.plugin_id, &key)
    }

    fn storage_set(&mut self, key: String, value: String) {
        storage_write(&self.plugin_id, &key, &value);
    }

    fn run_command(&mut self, text: String, target: CommandTarget) -> Result<(), String> {
        crate::ipc::request(
            "run_command",
            &json!({ "text": text, "target": target_token(target) }),
        )
        .map(|_| ())
    }

    fn send_input(&mut self, bytes: Vec<u8>) -> Result<(), String> {
        let text = String::from_utf8_lossy(&bytes).into_owned();
        crate::ipc::request("send_input", &json!({ "text": text })).map(|_| ())
    }

    fn read_screen(&mut self, lines: u32) -> Result<String, String> {
        crate::ipc::request("read_screen", &json!({ "lines": lines })).map(as_text)
    }

    fn selection(&mut self) -> Option<String> {
        None
    }

    fn fetch(&mut self, _request: HttpRequest) -> Result<HttpResponse, String> {
        // Network host support lands with the registry/trust stage; a plugin can
        // declare the capability, but the host call is not wired yet.
        Err("network access is not yet available".into())
    }

    fn read_file(&mut self, path: String) -> Result<Vec<u8>, String> {
        std::fs::read(scoped(&self.root, &path)?).map_err(|e| e.to_string())
    }

    fn write_file(&mut self, path: String, data: Vec<u8>) -> Result<(), String> {
        std::fs::write(scoped(&self.root, &path)?, data).map_err(|e| e.to_string())
    }

    fn clipboard_read(&mut self) -> Result<String, String> {
        Err("clipboard access is not yet available".into())
    }

    fn clipboard_write(&mut self, _text: String) -> Result<(), String> {
        Err("clipboard access is not yet available".into())
    }

    fn notify(&mut self, title: String, body: String) {
        let _ = crate::ipc::request("notify", &json!({ "title": title, "message": body }));
    }
}

/// A key coerced to a safe single filename.
fn sanitize(key: &str) -> String {
    let cleaned: String = key
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' { c } else { '_' })
        .collect();
    if cleaned.is_empty() {
        "_".to_string()
    } else {
        cleaned
    }
}
