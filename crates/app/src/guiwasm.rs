//! The in-window plugin runtime: renders a plugin's panel, delivers its UI
//! events, and calls its tools for `[[trigger]] do = { invoke }`, keeping the
//! instance resident.
//!
//! A host call runs while the guest is on the stack, so this host can't hold the
//! gpui context. Anything needing it is handed across in two directions instead:
//! the workspace deposits the focused cwd and the clipboard before the call, and
//! drains queued terminal commands and clipboard writes after it returns.
//! Context-free operations delegate to [`crate::wasmhost`], shared with the
//! CLI-side host.

use std::sync::{Arc, Mutex};

use pluginrt::{
    AppHost, CommandTarget, ExecOutput, ExecRequest, HttpRequest, HttpResponse, LogLevel, Runtime,
};

/// A command a wasm panel asked to run, dispatched after the call returns.
pub struct QueuedCommand {
    pub text: String,
    pub target: String,
}

#[derive(Default)]
struct Shared {
    commands: Vec<QueuedCommand>,
    /// The focused pane's directory, refreshed by the workspace before each
    /// call. The host runs off the gpui context, so it can't look this up
    /// itself — the caller, which has the context, deposits it here.
    cwd: Option<std::path::PathBuf>,
    /// The clipboard as it read at the start of this call, deposited by the
    /// caller for the same reason as `cwd`.
    clipboard: Option<String>,
    /// Text the plugin asked to put on the clipboard, applied by the caller
    /// after the call returns.
    clipboard_write: Option<String>,
}

/// Resident wasm panel instances plus the shared host channel.
pub struct GuiWasm {
    rt: Runtime,
    shared: Arc<Mutex<Shared>>,
}

impl GuiWasm {
    pub fn new() -> Option<Self> {
        Some(Self {
            rt: Runtime::new().ok()?,
            shared: Arc::new(Mutex::new(Shared::default())),
        })
    }

    /// Drain the commands the last call queued, to dispatch on the UI thread.
    pub fn take_commands(&self) -> Vec<QueuedCommand> {
        std::mem::take(&mut lock(&self.shared).commands)
    }

    /// Hand the next call the context it can't reach itself: the focused pane's
    /// directory (so a panel reports on the repo the user is in) and the current
    /// clipboard.
    pub fn set_context(&self, cwd: Option<std::path::PathBuf>, clipboard: Option<String>) {
        let mut shared = lock(&self.shared);
        shared.cwd = cwd;
        shared.clipboard = clipboard;
    }

    /// Take the text the last call asked to put on the clipboard, if any.
    pub fn take_clipboard_write(&self) -> Option<String> {
        lock(&self.shared).clipboard_write.take()
    }

    /// Render a wasm plugin's panel to a node-tree JSON (same shape the block-tree
    /// renderer consumes). Instantiates on first use.
    pub fn render(&mut self, plugin: &plugin::Plugin) -> Result<String, String> {
        self.ensure(plugin)?;
        self.rt.render(&plugin.id, "{}").map_err(|e| e.to_string())
    }

    /// Call one of a wasm plugin's tools. Used by `[[trigger]] do = { invoke }`,
    /// which delivers a terminal event to the tool of that name.
    pub fn call_tool(
        &mut self,
        plugin: &plugin::Plugin,
        tool: &str,
        params_json: &str,
    ) -> Result<String, String> {
        self.ensure(plugin)?;
        match self.rt.call_tool(&plugin.id, tool, params_json) {
            Ok(inner) => inner,
            Err(trap) => Err(format!("plugin trapped: {trap}")),
        }
    }

    /// Deliver a UI event (button click) to a wasm plugin's panel.
    pub fn on_ui_event(&mut self, plugin: &plugin::Plugin, event_json: &str) -> Result<(), String> {
        self.ensure(plugin)?;
        self.rt
            .on_ui_event(&plugin.id, event_json)
            .map_err(|e| e.to_string())
    }

    fn ensure(&mut self, plugin: &plugin::Plugin) -> Result<(), String> {
        if !self.rt.is_resident(&plugin.id) {
            let rel = plugin
                .runtime
                .as_ref()
                .map(|r| r.wasm.as_str())
                .ok_or("plugin has no [runtime]")?;
            let wasm = std::fs::read(plugin.path.join(rel)).map_err(|e| e.to_string())?;
            let host = Box::new(GuiHost {
                shared: self.shared.clone(),
                root: plugin.path.clone(),
                id: plugin.id.clone(),
            });
            // Enforce consent: link only the capabilities the user granted.
            let caps = plugin::Installed::load()
                .effective_capabilities(&plugin.id, &plugin.capabilities);
            self.rt
                .ensure(&plugin.id, &wasm, &caps, host)
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

struct GuiHost {
    shared: Arc<Mutex<Shared>>,
    root: std::path::PathBuf,
    id: String,
}

fn lock(s: &Arc<Mutex<Shared>>) -> std::sync::MutexGuard<'_, Shared> {
    s.lock().unwrap_or_else(|e| e.into_inner())
}

impl AppHost for GuiHost {
    fn log(&mut self, level: LogLevel, message: String) {
        eprintln!("[plugin {}] {level:?}: {message}", self.id);
    }
    fn storage_get(&mut self, key: String) -> Option<String> {
        crate::wasmhost::storage_read(&self.id, &key)
    }
    fn storage_set(&mut self, key: String, value: String) {
        crate::wasmhost::storage_write(&self.id, &key, &value);
    }
    fn run_command(&mut self, text: String, target: CommandTarget) -> Result<(), String> {
        lock(&self.shared).commands.push(QueuedCommand {
            text,
            target: crate::wasmhost::target_token(target).to_string(),
        });
        Ok(())
    }
    fn send_input(&mut self, bytes: Vec<u8>) -> Result<(), String> {
        lock(&self.shared).commands.push(QueuedCommand {
            text: String::from_utf8_lossy(&bytes).into_owned(),
            target: "pane".to_string(),
        });
        Ok(())
    }
    fn read_screen(&mut self, _lines: u32) -> Result<String, String> {
        // A live screen snapshot for panels is a follow-up; empty for now.
        Ok(String::new())
    }
    fn selection(&mut self) -> Option<String> {
        None
    }
    fn fetch(&mut self, request: HttpRequest) -> Result<HttpResponse, String> {
        crate::wasmhost::fetch(request)
    }
    fn read_file(&mut self, path: String) -> Result<Vec<u8>, String> {
        std::fs::read(crate::wasmhost::scoped(&self.root, &path)?).map_err(|e| e.to_string())
    }
    fn write_file(&mut self, path: String, data: Vec<u8>) -> Result<(), String> {
        std::fs::write(crate::wasmhost::scoped(&self.root, &path)?, data).map_err(|e| e.to_string())
    }
    fn clipboard_read(&mut self) -> Result<String, String> {
        Ok(lock(&self.shared).clipboard.clone().unwrap_or_default())
    }
    fn clipboard_write(&mut self, text: String) -> Result<(), String> {
        lock(&self.shared).clipboard_write = Some(text);
        Ok(())
    }
    fn notify(&mut self, title: String, body: String) {
        // Async on purpose: this runs on the gpui main thread (the host is
        // driven synchronously from `render_wasm_panel`), and the macOS backend
        // can block for as long as the system authorization dialog is up.
        crate::view::post_os_notification(&title, &body);
    }
    fn exec(&mut self, request: ExecRequest) -> Result<ExecOutput, String> {
        let cwd = lock(&self.shared).cwd.clone();
        crate::wasmhost::exec(request, cwd)
    }
}
