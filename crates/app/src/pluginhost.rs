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

use serde::{Deserialize, Serialize};

/// One request sent to a plugin runtime (serialized to stdin as JSON).
#[derive(Serialize)]
pub struct Request<'a> {
    /// `"render"` or `"action"`.
    pub kind: &'a str,
    /// Target panel id.
    pub panel: &'a str,
    /// Action id, when `kind == "action"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<&'a str>,
    /// The focused pane's working directory, so the plugin acts on the right
    /// place (e.g. the repo the user is in).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<&'a str>,
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
}

/// Invoke `plugin`'s runtime with `req`, returning the parsed response. The
/// process runs in the plugin's own directory (so relative entrypoints like
/// `plugin.ts` resolve); the target directory travels in `req.cwd`.
pub fn invoke(plugin: &plugin::Plugin, req: &Request) -> Result<Response, String> {
    let runtime = plugin.runtime.as_ref().ok_or("plugin has no [runtime]")?;
    let mut parts = runtime.command.split_whitespace();
    let program = parts.next().ok_or("empty runtime command")?;
    let args: Vec<&str> = parts.collect();

    let mut child = Command::new(program)
        .args(&args)
        .current_dir(&plugin.path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("spawn `{program}`: {e}"))?;

    let body = serde_json::to_vec(req).map_err(|e| e.to_string())?;
    child
        .stdin
        .take()
        .ok_or("no stdin")?
        .write_all(&body)
        .map_err(|e| format!("write request: {e}"))?;

    let out = child
        .wait_with_output()
        .map_err(|e| format!("wait: {e}"))?;
    if !out.status.success() {
        return Err(format!("`{program}` exited with {}", out.status));
    }
    if out.stdout.iter().all(u8::is_ascii_whitespace) {
        return Ok(Response::default());
    }
    serde_json::from_slice::<Response>(&out.stdout).map_err(|e| format!("bad response: {e}"))
}

#[cfg(test)]
#[path = "../tests/pluginhost.rs"]
mod tests;
