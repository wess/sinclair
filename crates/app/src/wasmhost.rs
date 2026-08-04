//! WASM plugin runtime for the app: instantiates a plugin's component through
//! [`pluginrt`], keeps it resident, and calls its tools. The [`AppHost`] the
//! plugin sees ([`SocketHost`]) routes terminal operations to the running GUI
//! over the single-instance socket ([`crate::ipc`]) — the same path built-in
//! MCP tools use — and implements storage / logging / filesystem directly.
//!
//! This is the Stage-2 tool path (invoked from `mcpbridge`). Panels, triggers,
//! and webviews get their own GUI-side runner in later stages.

use std::process::{Command, Stdio};
use std::time::Duration;

use pluginrt::{
    AppHost, CommandTarget, ExecOutput, ExecRequest, HttpRequest, HttpResponse, LogLevel, Runtime,
};
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

/// Kill a program that outruns this. A plugin's `exec` is driven synchronously
/// from a panel render or a tool call, so an unbounded child would pin the
/// caller; the guest's fuel budget can't help here because the guest is parked
/// in a host call, not executing instructions.
const EXEC_TIMEOUT: Duration = Duration::from_secs(15);
const EXEC_POLL: Duration = Duration::from_millis(50);

/// Cap on what a program can hand back, so a plugin can't be fed a gigabyte by
/// `cat`-ing the wrong file and take the app's memory with it.
const EXEC_MAX_OUTPUT: usize = 4 * 1024 * 1024;

/// Run a program and capture its output — the `process` capability's one host
/// call, shared by both hosts. `default_cwd` applies when the plugin named none.
///
/// This deliberately spawns the program directly rather than through a shell:
/// the guest supplies `program` and `args` as separate values, so nothing it
/// passes can be reinterpreted as shell syntax.
pub(crate) fn exec(
    request: ExecRequest,
    default_cwd: Option<std::path::PathBuf>,
) -> Result<ExecOutput, String> {
    if request.program.trim().is_empty() {
        return Err("exec: no program".into());
    }
    let mut cmd = Command::new(&request.program);
    cmd.args(&request.args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(dir) = request.cwd.as_deref().map(std::path::PathBuf::from).or(default_cwd) {
        cmd.current_dir(dir);
    }
    // Own session, so a timeout kill takes the whole tree rather than leaving
    // orphaned grandchildren behind (mirrors the pty spawn).
    #[cfg(unix)]
    unsafe {
        use std::os::unix::process::CommandExt;
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    let child = cmd.spawn().map_err(|e| {
        format!(
            "exec `{}`: {e}. Is it installed and on your PATH?",
            request.program
        )
    })?;

    // Watchdog: kill the child if it overruns. `done` stops us killing after a
    // normal exit, when the pid may already have been reused.
    let pid = child.id();
    let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = done.clone();
    let watchdog = std::thread::spawn(move || {
        let mut waited = Duration::ZERO;
        while waited < EXEC_TIMEOUT {
            if flag.load(std::sync::atomic::Ordering::Relaxed) {
                return;
            }
            std::thread::sleep(EXEC_POLL);
            waited += EXEC_POLL;
        }
        if !flag.load(std::sync::atomic::Ordering::Relaxed) {
            kill_tree(pid);
        }
    });

    let out = child.wait_with_output();
    done.store(true, std::sync::atomic::Ordering::Relaxed);
    let _ = watchdog.join();
    let out = out.map_err(|e| format!("exec wait: {e}"))?;

    Ok(ExecOutput {
        // `code()` is None when a signal killed it — including our own timeout.
        status: out.status.code().unwrap_or(-1),
        stdout: clamp(&out.stdout),
        stderr: clamp(&out.stderr),
    })
}

/// Lossy UTF-8, truncated at [`EXEC_MAX_OUTPUT`] bytes.
fn clamp(bytes: &[u8]) -> String {
    let end = bytes.len().min(EXEC_MAX_OUTPUT);
    // Trim to a char boundary so the lossy conversion doesn't append a
    // replacement char for a codepoint we simply cut in half.
    let mut end = end;
    while end > 0 && end < bytes.len() && (bytes[end] & 0xC0) == 0x80 {
        end -= 1;
    }
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

/// SIGKILL a process group (see the `setsid` above).
fn kill_tree(pid: u32) {
    #[cfg(unix)]
    unsafe {
        libc::kill(-(pid as i32), libc::SIGKILL);
    }
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/F", "/T"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

/// The focused pane's working directory, asked of the running GUI over the
/// socket. Used by the CLI-side host, which has no window of its own.
fn focused_cwd_over_ipc() -> Option<std::path::PathBuf> {
    let reply = crate::ipc::request("list_panes", &json!({})).ok()?;
    let panes = reply.get("panes")?.as_array()?;
    let pane = panes
        .iter()
        .find(|p| p.get("focused").and_then(Value::as_bool).unwrap_or(false))?;
    let items = pane.get("items")?.as_array()?;
    let item = items
        .iter()
        .find(|i| i.get("active").and_then(Value::as_bool).unwrap_or(false))
        .or_else(|| items.first())?;
    Some(item.get("cwd")?.as_str()?.into())
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

    fn exec(&mut self, request: ExecRequest) -> Result<ExecOutput, String> {
        // With no cwd of its own, fall back to the pane the user is looking at,
        // so `git status` reports on the repo they're in.
        exec(request, focused_cwd_over_ipc())
    }
}

#[cfg(test)]
#[path = "../tests/wasmhost.rs"]
mod tests;

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
