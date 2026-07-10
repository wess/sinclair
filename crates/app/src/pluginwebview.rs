//! Host for a web-view surface. Wraps a guise [`WebView`], injects the
//! `window.Sinclair` JavaScript bridge, and routes messages the page posts
//! (`window.ipc.postMessage`, via the bridge) to the app.
//!
//! The host is driven by a [`WebviewSurface`] descriptor, not a plugin — so both
//! plugin `[webview]`s and first-class built-ins (e.g. Notes) share one host.
//! Plugins build a surface from their manifest ([`WebviewSurface::from_plugin`]);
//! built-ins construct one directly.
//!
//! Routing: a page calls `Sinclair.invoke(method, params)`. Known methods are the
//! app's MCP capabilities (`run_command`, `read_screen`, …) and run through the
//! active workspace's [`crate::root::WorkspaceView::mcp_dispatch`]. Anything else is forwarded
//! to the surface's `runtime` (if any) as a `message` request. The reply
//! resolves the page's promise via `evaluate_script`.

use std::path::PathBuf;

use gpui::prelude::*;
use gpui::{div, Context, Entity, FocusHandle, Subscription, Window};

use guise::{WebView, WebViewEvent};
use serde_json::Value;

use crate::pluginhost;

/// Where a surface gets its content and how it comes up.
pub enum SurfaceContent {
    /// Load a URL as-is.
    Url(String),
    /// Serve `dir` over the `guise://` origin and load `entry` (a real origin,
    /// so the JS bridge works — `file://` pages can't reach native).
    Entry { dir: PathBuf, entry: String },
    /// Show a placeholder, then navigate to the URL produced by `boot`, with
    /// `{port}` in `url_template` substituted.
    Boot { url_template: String, boot: Boot },
}

/// How a [`SurfaceContent::Boot`] surface learns its address.
pub enum Boot {
    /// Invoke the surface's plugin `runtime` `boot` method; it returns `{ url }`
    /// or `{ port }` (a plugin starting its own server).
    Runtime,
    /// A host-managed sidecar (`[[webview]] service = "…"`, and how the Notes
    /// plugin works): the host reserves a loopback port, mints a token, and
    /// spawns the command in `dir` with both in its environment
    /// (`SINCLAIR_SERVICE_PORT` / `SINCLAIR_SERVICE_TOKEN`). The child stays a
    /// tracked child process, shared by every surface with the same id and
    /// reaped when the last one closes — see [`crate::sidecar`].
    Command { command: String, dir: PathBuf },
}

/// What a [`PluginWebView`] hosts: identity, content, and an optional runtime to
/// forward unhandled bridge calls to.
pub struct WebviewSurface {
    /// Stable id; the runtime message target and tab/panel token.
    pub id: String,
    /// Header/tab/window title.
    pub title: String,
    pub content: SurfaceContent,
    /// Plugin to forward unknown `invoke()` methods to (a plugin webview). `None`
    /// for a built-in whose page talks to its own server instead of the bridge.
    pub runtime: Option<plugin::Plugin>,
}

impl WebviewSurface {
    /// Build a surface from a plugin's `[webview]` manifest.
    pub fn from_plugin(plugin: plugin::Plugin) -> Self {
        let decl = plugin.webview.clone();
        let id = decl
            .as_ref()
            .map(|w| w.id.clone())
            .unwrap_or_else(|| plugin.id.clone());
        let title = decl
            .as_ref()
            .map(|w| w.title.clone())
            .unwrap_or_else(|| plugin.name.clone());
        let boot = decl.as_ref().map(|w| w.boot).unwrap_or(false);
        let content = match decl.as_ref().map(|w| &w.source) {
            Some(plugin::WebviewSource::Url(u)) if boot => SurfaceContent::Boot {
                url_template: u.clone(),
                boot: Boot::Runtime,
            },
            Some(plugin::WebviewSource::Url(u)) => SurfaceContent::Url(u.clone()),
            Some(plugin::WebviewSource::Entry(e)) => SurfaceContent::Entry {
                dir: plugin.path.clone(),
                entry: e.clone(),
            },
            Some(plugin::WebviewSource::Service(cmd)) => SurfaceContent::Boot {
                url_template: "http://127.0.0.1:{port}/?token={token}".to_string(),
                boot: Boot::Command {
                    command: cmd.clone(),
                    // A sidecar needs a writable working dir for its state; the
                    // plugin's own dir may be read-only (a bundled plugin lives
                    // inside the app).
                    dir: service_dir(&plugin.id),
                },
            },
            None => SurfaceContent::Url(String::new()),
        };
        Self {
            id,
            title,
            content,
            runtime: Some(plugin),
        }
    }
}

/// Injected at document start. Exposes `window.Sinclair` — a small VS Code-style
/// bridge over wry's `window.ipc.postMessage`. `invoke` returns a Promise the
/// host resolves by name via `window.__sinclairResolve`. `window.Prompt` aliases
/// the same object so plugins written before the rename keep working.
const BRIDGE_JS: &str = r#"
(function () {
  if (window.Sinclair) return;
  var pending = {}, seq = 1, listeners = [];
  function send(obj) { window.ipc.postMessage(JSON.stringify(obj)); }
  window.__sinclairResolve = function (id, ok, value) {
    var p = pending[id]; if (!p) return; delete pending[id];
    ok ? p.resolve(value) : p.reject(value);
  };
  window.__sinclairDeliver = function (msg) {
    listeners.forEach(function (cb) { try { cb(msg); } catch (e) {} });
  };
  window.Sinclair = {
    // Fire-and-forget message to the plugin runtime.
    postMessage: function (data) { send({ kind: "post", method: "postMessage", params: data }); },
    // Request/response; resolves with the host or runtime reply.
    invoke: function (method, params) {
      var id = seq++;
      return new Promise(function (resolve, reject) {
        pending[id] = { resolve: resolve, reject: reject };
        send({ kind: "invoke", id: id, method: method, params: params === undefined ? null : params });
      });
    },
    // Subscribe to native -> page pushes.
    onMessage: function (cb) { listeners.push(cb); },
    // Sugar for common built-ins.
    runCommand: function (text, target) { return this.invoke("run_command", { text: text, target: target || "pane" }); },
    readScreen: function (lines) { return this.invoke("read_screen", lines ? { lines: lines } : {}); },
  };
  // Pre-rename alias, so a plugin binding `window.Prompt` still resolves.
  window.Prompt = window.Sinclair;
})();
"#;

/// Shown in a `boot` webview while its server starts up.
const STARTING_HTML: &str = "<!doctype html><html><head><meta charset=\"utf-8\">\
<style>body{background:#1c1c1e;color:#8a8a90;font:13px -apple-system,system-ui,sans-serif;\
display:flex;align-items:center;justify-content:center;height:100vh;margin:0}</style></head>\
<body>Starting\u{2026}</body></html>";

/// A simple failure page for a `boot` surface whose server didn't come up.
fn failure_html(err: &str) -> String {
    let safe = err.replace('<', "&lt;").replace('>', "&gt;");
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\">\
<style>body{{background:#1c1c1e;color:#e0a0a0;font:13px -apple-system,system-ui,sans-serif;\
display:flex;align-items:center;justify-content:center;height:100vh;margin:0;padding:20px;\
text-align:center}}</style></head><body>Couldn't start: {safe}</body></html>"
    )
}

pub struct PluginWebView {
    surface: WebviewSurface,
    webview: Entity<WebView>,
    focus: FocusHandle,
    /// The sidecar service this surface holds a reference on (a booted
    /// `Boot::Command`); given back on drop so the host reaps the child when
    /// the last surface using it closes.
    service: Option<String>,
    _sub: Subscription,
}

impl PluginWebView {
    pub fn new(surface: WebviewSurface, cx: &mut Context<Self>) -> Self {
        // A `Boot` surface shows a placeholder, then navigates once its server
        // reports its http address. `Entry` is served over guise://; `Url` loads
        // as-is.
        let webview = cx.new(|cx| {
            let wv = WebView::new(cx).init_script(BRIDGE_JS).bordered(false);
            match &surface.content {
                SurfaceContent::Boot { .. } => wv.html(STARTING_HTML),
                SurfaceContent::Entry { dir, entry } => wv.serve(dir.clone(), entry.clone()),
                SurfaceContent::Url(u) => wv.url(u.clone()),
            }
        });

        let sub = cx.subscribe(&webview, |this, _wv, event: &WebViewEvent, cx| {
            if let WebViewEvent::Message(payload) = event {
                this.on_message(payload.to_string(), cx);
            }
        });

        let mut this = Self {
            surface,
            webview,
            focus: cx.focus_handle(),
            service: None,
            _sub: sub,
        };
        if matches!(this.surface.content, SurfaceContent::Boot { .. }) {
            this.boot(cx);
        }
        this
    }

    /// Show or hide the native web view. A panel/tab host must hide it when its
    /// surface is no longer on screen (a collapsed drawer, a switched panel), or
    /// the OS view lingers at its last position.
    pub fn set_visible(&self, visible: bool, cx: &mut Context<Self>) {
        self.webview.update(cx, |wv, _| wv.set_visible(visible));
    }

    /// Bring a `Boot` surface up: resolve its address (invoke a plugin runtime,
    /// or ensure a bundled server), then navigate. A failure shows a page.
    fn boot(&mut self, cx: &mut Context<Self>) {
        let SurfaceContent::Boot { url_template, boot } = &self.surface.content else {
            return;
        };
        let url = url_template.clone();
        let webview = self.webview.clone();
        let executor = cx.background_executor().clone();
        match boot {
            Boot::Command { command, dir } => {
                let id = self.surface.id.clone();
                let command = command.clone();
                let dir = dir.clone();
                cx.spawn(async move |this, cx| {
                    let key = id.clone();
                    let target = executor
                        .spawn(async move {
                            crate::sidecar::acquire(&key, &command, &dir).map(|(port, token)| {
                                url.replace("{port}", &port.to_string()).replace("{token}", &token)
                            })
                        })
                        .await;
                    match target {
                        Ok(u) => {
                            let live = this.update(cx, |view, cx| {
                                view.service = Some(id.clone());
                                view.webview.update(cx, |wv, cx| wv.load_url(u, cx));
                            });
                            // The surface closed while the service was starting:
                            // give the reference straight back so it is reaped.
                            if live.is_err() {
                                crate::sidecar::release(&id);
                            }
                        }
                        Err(e) => {
                            webview.update(cx, |wv, cx| wv.load_html(failure_html(&e), cx));
                        }
                    }
                })
                .detach();
            }
            Boot::Runtime => {
                let Some(plugin) = self.surface.runtime.clone() else {
                    return;
                };
                let id = self.surface.id.clone();
                cx.spawn(async move |_this, cx| {
                    let resp = executor
                        .spawn(async move {
                            let req = pluginhost::Request {
                                kind: "message",
                                panel: &id,
                                action: None,
                                cwd: None,
                                method: Some("boot"),
                                params: None,
                            };
                            pluginhost::invoke(&plugin, &req)
                        })
                        .await;
                    let target = boot_target(resp, &url);
                    webview.update(cx, |wv, cx| match target {
                        Ok(u) => wv.load_url(u, cx),
                        Err(e) => wv.load_html(failure_html(&e), cx),
                    });
                })
                .detach();
            }
        }
    }

    /// Handle one `window.ipc.postMessage` payload from the page.
    fn on_message(&mut self, payload: String, cx: &mut Context<Self>) {
        let Ok(msg) = serde_json::from_str::<Value>(&payload) else {
            return;
        };
        let id = msg.get("id").and_then(Value::as_i64);
        let method = msg
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let params = msg.get("params").cloned().unwrap_or(Value::Null);

        // Try the app's built-in capabilities on the active workspace window —
        // with several windows open, bridge commands must land where the user
        // is, not in an arbitrary first window.
        let handle = crate::mcpbridge::active_workspace(cx);
        let dispatched = handle.map(|h| {
            h.update(cx, |ws, window, cx| {
                ws.mcp_dispatch(&method, &params, window, cx)
            })
        });

        match dispatched {
            Some(Ok(Ok(value))) => self.resolve(id, true, &value, cx),
            // Not a built-in op: forward to the surface's runtime, if any.
            Some(Ok(Err(e))) if e.starts_with("unknown op") && self.surface.runtime.is_some() => {
                self.forward_to_runtime(id, method, params, cx);
            }
            Some(Ok(Err(e))) => self.resolve(id, false, &Value::String(e), cx),
            _ => self.resolve(id, false, &Value::String("workspace unavailable".into()), cx),
        }
    }

    /// Forward an unknown method to the surface's `runtime` as a `message`
    /// request, off the UI thread, then resolve the page's promise.
    fn forward_to_runtime(
        &mut self,
        id: Option<i64>,
        method: String,
        params: Value,
        cx: &mut Context<Self>,
    ) {
        let Some(plugin) = self.surface.runtime.clone() else {
            return;
        };
        let surface_id = self.surface.id.clone();
        let cwd = self.focused_cwd(cx);
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let resp = executor
                .spawn(async move {
                    let req = pluginhost::Request {
                        kind: "message",
                        panel: &surface_id,
                        action: None,
                        cwd: cwd.as_deref(),
                        method: Some(&method),
                        params: Some(&params),
                    };
                    pluginhost::invoke(&plugin, &req)
                })
                .await;
            let _ = this.update(cx, |this, cx| match resp {
                Ok(r) => this.resolve(id, true, &r.result.unwrap_or(Value::Null), cx),
                Err(e) => this.resolve(id, false, &Value::String(e), cx),
            });
        })
        .detach();
    }

    /// Resolve (or reject) the page-side promise for `id` via the bridge.
    fn resolve(&self, id: Option<i64>, ok: bool, value: &Value, cx: &Context<Self>) {
        let Some(id) = id else {
            return;
        };
        let vjson = serde_json::to_string(value).unwrap_or_else(|_| "null".to_string());
        self.webview
            .read(cx)
            .evaluate_script(&format!("window.__sinclairResolve({id}, {ok}, {vjson});"));
    }

    /// Push a message to the page's `Sinclair.onMessage(cb)` listeners — the
    /// host→page direction of the bridge. `__sinclairDeliver` was defined in the
    /// injected JS but had no Rust caller (a v1 dead end); this is it. Available
    /// for a feature that pushes to a plugin webview (e.g. a subscribed event).
    #[allow(dead_code)]
    pub(crate) fn post_to_page(&self, message: &Value, cx: &Context<Self>) {
        let json = serde_json::to_string(message).unwrap_or_else(|_| "null".to_string());
        self.webview
            .read(cx)
            .evaluate_script(&format!("window.__sinclairDeliver({json});"));
    }

    /// The focused pane's working directory on the active workspace, so a
    /// plugin runtime acts on the right place.
    fn focused_cwd(&self, cx: &mut Context<Self>) -> Option<String> {
        let handle = crate::mcpbridge::active_workspace(cx)?;
        handle
            .update(cx, |ws, _window, cx| ws.focused_cwd(cx))
            .ok()
            .flatten()
            .map(|p| p.to_string_lossy().into_owned())
    }

    /// The surface's title, for the tab/pane label.
    pub fn title(&self) -> String {
        self.surface.title.clone()
    }
}

/// Writable working directory for a plugin's host-managed sidecar service:
/// `<config>/sinclair/data/<plugin-id>`. Kept separate from the (possibly
/// read-only, bundled) plugin dir; created on demand at spawn.
fn service_dir(plugin_id: &str) -> PathBuf {
    crate::paths::data_dir(plugin_id)
}

/// Interpret a plugin runtime's `boot` reply: `{ error }` fails; `{ url }` wins;
/// `{ port }` fills `{port}` in the template; anything else uses the template.
fn boot_target(resp: Result<pluginhost::Response, String>, url: &str) -> Result<String, String> {
    match resp {
        Ok(r) => match r.result {
            Some(res) => {
                if let Some(e) = res.get("error").and_then(Value::as_str) {
                    Err(e.to_string())
                } else if let Some(u) = res.get("url").and_then(Value::as_str) {
                    Ok(u.to_string())
                } else if let Some(p) = res.get("port").and_then(Value::as_u64) {
                    Ok(url.replace("{port}", &p.to_string()))
                } else {
                    Ok(url.to_string())
                }
            }
            None => Ok(url.to_string()),
        },
        Err(e) => Err(e),
    }
}

impl Drop for PluginWebView {
    /// Closing the last surface that uses a sidecar service reaps its child.
    /// Every path that discards the host lands here: closing the tab/window,
    /// a plugin reload rebuilding the panel hosts, and app teardown.
    fn drop(&mut self) {
        if let Some(id) = self.service.take() {
            crate::sidecar::release(&id);
        }
    }
}

impl gpui::Focusable for PluginWebView {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for PluginWebView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .track_focus(&self.focus)
            .child(self.webview.clone())
    }
}
