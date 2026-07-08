//! Invoke IPC plugins, serverless-style: spawn the plugin's `[runtime]`
//! command once per event, exchange a single JSON request/response over
//! stdin/stdout, and let the short-lived process exit.
//!
//! A request is either a `render` (draw this panel) or an `action` (a button in
//! the panel was clicked). The response is a block tree (what to paint) plus
//! optional `run` directives (commands to execute in the terminal). The plugin
//! never links the GUI; it only describes UI and asks for terminal effects.

use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Kill a plugin that runs longer than this (it runs off the UI thread, but a
/// hung child would otherwise pin a worker thread and leave the panel stuck).
const TIMEOUT: Duration = Duration::from_secs(15);
const POLL: Duration = Duration::from_millis(100);

/// One request sent to a plugin runtime (serialized to stdin as JSON).
#[derive(Serialize)]
pub struct Request<'a> {
    /// `"render"`, `"action"`, or `"message"` (a call from a webview surface).
    pub kind: &'a str,
    /// Target surface id (panel or webview).
    pub panel: &'a str,
    /// Action id, when `kind == "action"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<&'a str>,
    /// The focused pane's working directory, so the plugin acts on the right
    /// place (e.g. the repo the user is in).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<&'a str>,
    /// Method name, when `kind == "message"` (a webview `invoke(method, …)`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method: Option<&'a str>,
    /// Arbitrary params for a `message`, forwarded verbatim from the page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<&'a serde_json::Value>,
}

/// A plugin's reply: the panel contents plus terminal effects to apply.
#[derive(Deserialize, Default, Clone)]
pub struct Response {
    /// Optional override for the panel header.
    #[serde(default)]
    pub title: Option<String>,
    /// The block tree to render.
    #[serde(default)]
    pub blocks: Vec<Block>,
    /// Commands to run in the terminal after rendering.
    #[serde(default)]
    pub run: Vec<Run>,
    /// For a `message` reply: the value that resolves the page's `invoke()`
    /// promise. Ignored for `render`/`action` responses.
    #[serde(default)]
    pub result: Option<serde_json::Value>,
}

/// A terminal command directive returned by a plugin.
#[derive(Deserialize, Clone)]
pub struct Run {
    pub text: String,
    /// `pane` (default) | `tab` | `split_right` | `split_down`.
    #[serde(default)]
    pub target: Option<String>,
}

/// One node of a plugin-described UI. Maps onto guise components in the
/// sidebar renderer.
#[derive(Deserialize, Clone)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Block {
    /// A dimmed sub-header.
    Section { title: String },
    /// A line of text.
    Text {
        text: String,
        #[serde(default)]
        dimmed: bool,
    },
    /// A horizontal rule.
    Divider,
    /// A key/value row (key dimmed, value normal).
    Kv { key: String, value: String },
    /// A small pill.
    Badge {
        label: String,
        #[serde(default)]
        color: Option<String>,
    },
    /// A clickable button; clicking sends an `action` request with `id`.
    Button {
        id: String,
        label: String,
        #[serde(default)]
        variant: Option<String>,
    },
    /// A horizontal group of blocks.
    Row {
        #[serde(default)]
        children: Vec<Block>,
    },
    /// An unrecognized block type. Kept as a catch-all so one unknown node (a
    /// newer block type, a typo) renders an inline notice instead of failing the
    /// whole panel's parse.
    #[serde(other)]
    Unknown,
}

/// Force-terminate the process `pid` (a plugin runtime that blew its timeout).
#[cfg(unix)]
fn force_kill(pid: u32) {
    unsafe { libc::kill(pid as libc::pid_t, libc::SIGKILL) };
}

/// Force-terminate the process `pid` via `taskkill /F /T` (also reaps its
/// child tree). Dependency-free; `taskkill` ships with Windows.
#[cfg(windows)]
fn force_kill(pid: u32) {
    let _ = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/F", "/T"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// Invoke `plugin`'s runtime with `req`, returning the parsed response. The
/// process runs in the plugin's own directory (so relative entrypoints like
/// `plugin.ts` resolve); the target directory travels in `req.cwd`.
pub fn invoke(plugin: &plugin::Plugin, req: &Request) -> Result<Response, String> {
    let runtime = plugin.runtime.as_ref().ok_or("plugin has no [runtime]")?;
    // The WASM runtime's execution engine is not built yet (see
    // docs/plugins-wasm.md); fail clearly rather than trying to spawn nothing.
    if runtime.kind == plugin::RuntimeKind::Wasm {
        return Err(
            "wasm plugin runtimes are declared but not yet executable — see docs/plugins-wasm.md"
                .to_string(),
        );
    }
    let mut parts = runtime.command.split_whitespace();
    let program = parts.next().ok_or("empty runtime command")?.to_string();
    let args: Vec<String> = parts.map(str::to_string).collect();

    // Serialize before spawning so a serialize error can't leak a live child.
    let body = serde_json::to_vec(req).map_err(|e| e.to_string())?;

    let mut child = Command::new(&program)
        .args(&args)
        .current_dir(&plugin.path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("spawn `{program}`: {e}. Is `{program}` installed and on your PATH?"))?;

    // Feed stdin from a separate thread so a plugin that floods stdout before
    // draining stdin can't deadlock us (we'd block writing while it blocks
    // writing). Dropping the handle at the end closes stdin.
    let Some(mut stdin) = child.stdin.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return Err("no stdin".into());
    };
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(&body);
    });

    // Watchdog: SIGKILL the child if it overruns the budget. `done` guards
    // against killing after a normal exit (and a reused pid).
    let pid = child.id();
    let done = Arc::new(AtomicBool::new(false));
    let flag = done.clone();
    let watchdog = std::thread::spawn(move || {
        let mut waited = Duration::ZERO;
        while waited < TIMEOUT {
            if flag.load(Ordering::Relaxed) {
                return;
            }
            std::thread::sleep(POLL);
            waited += POLL;
        }
        if !flag.load(Ordering::Relaxed) {
            force_kill(pid);
        }
    });

    let out = child.wait_with_output();
    done.store(true, Ordering::Relaxed);
    let _ = writer.join();
    let _ = watchdog.join();
    let out = out.map_err(|e| format!("wait: {e}"))?;

    if !out.status.success() {
        return Err(format!(
            "`{program}` exited with {}{}",
            out.status,
            stderr_tail(&out.stderr)
        ));
    }
    if out.stdout.iter().all(u8::is_ascii_whitespace) {
        return Ok(Response::default());
    }
    serde_json::from_slice::<Response>(&out.stdout)
        .map_err(|e| format!("bad response: {e}{}", stderr_tail(&out.stderr)))
}

/// The last few lines of a plugin's stderr, appended to an error so the author
/// sees why it failed (v1 discarded stderr entirely). Empty when it printed none.
fn stderr_tail(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let text = text.trim();
    if text.is_empty() {
        return String::new();
    }
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(8);
    format!("\n--- stderr ---\n{}", lines[start..].join("\n"))
}

#[cfg(test)]
#[path = "../tests/pluginhost.rs"]
mod tests;
