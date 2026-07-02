//! Host for a plugin-owned web view. Wraps a guise [`WebView`], injects the
//! `window.Prompt` JavaScript bridge, and routes messages the page posts
//! (`window.ipc.postMessage`, via the bridge) to the app.
//!
//! Routing: a page calls `Prompt.invoke(method, params)`. Known methods are the
//! app's MCP capabilities (`run_command`, `read_screen`, …) and run through the
//! main workspace's [`WorkspaceView::mcp_dispatch`]. Anything else is forwarded
//! to the plugin's `[runtime]` as a `message` request. The reply resolves the
//! page's promise via `evaluate_script`.
//!
//! The same host backs every placement (panel, window); it always acts on the
//! main workspace window, so terminal effects land on the user's focused pane.

use gpui::prelude::*;
use gpui::{div, Context, Entity, FocusHandle, Subscription, Window};

use guise::{WebView, WebViewEvent};
use serde_json::Value;

use crate::pluginhost;
use crate::root::WorkspaceView;

/// Injected at document start. Exposes `window.Prompt` — a small VS Code-style
/// bridge over wry's `window.ipc.postMessage`. `invoke` returns a Promise the
/// host resolves by name via `window.__promptResolve`.
const BRIDGE_JS: &str = r#"
(function () {
  if (window.Prompt) return;
  var pending = {}, seq = 1, listeners = [];
  function send(obj) { window.ipc.postMessage(JSON.stringify(obj)); }
  window.__promptResolve = function (id, ok, value) {
    var p = pending[id]; if (!p) return; delete pending[id];
    ok ? p.resolve(value) : p.reject(value);
  };
  window.__promptDeliver = function (msg) {
    listeners.forEach(function (cb) { try { cb(msg); } catch (e) {} });
  };
  window.Prompt = {
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
})();
"#;

/// Shown in a `boot` webview while its runtime starts up.
const STARTING_HTML: &str = "<!doctype html><html><head><meta charset=\"utf-8\">\
<style>body{background:#1c1c1e;color:#8a8a90;font:13px -apple-system,system-ui,sans-serif;\
display:flex;align-items:center;justify-content:center;height:100vh;margin:0}</style></head>\
<body>Starting\u{2026}</body></html>";

/// A simple failure page for a `boot` webview whose runtime didn't come up.
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
    plugin: plugin::Plugin,
    /// The `[webview]` surface id, used as the runtime message target.
    webview_id: String,
    webview: Entity<WebView>,
    focus: FocusHandle,
    _sub: Subscription,
}

impl PluginWebView {
    pub fn new(plugin: plugin::Plugin, cx: &mut Context<Self>) -> Self {
        let decl = plugin.webview.clone();
        let webview_id = decl
            .as_ref()
            .map(|w| w.id.clone())
            .unwrap_or_else(|| plugin.id.clone());
        let boot = decl.as_ref().map(|w| w.boot).unwrap_or(false);
        // The URL template a boot webview navigates to once its runtime reports
        // the port (see `boot_runtime`).
        let boot_url = decl
            .as_ref()
            .and_then(|w| match &w.source {
                plugin::WebviewSource::Url(u) => Some(u.clone()),
                plugin::WebviewSource::Entry(_) => None,
            })
            .unwrap_or_default();

        // A `boot` webview shows a placeholder, then navigates once its runtime
        // reports its http address. A plain `entry` is served over guise:// (a
        // real origin, so the JS bridge works — file:// pages can't reach
        // native). A `url` loads as-is.
        let source = decl.as_ref().map(|w| w.source.clone());
        let dir = plugin.path.clone();
        let webview = cx.new(|cx| {
            let wv = WebView::new(cx).init_script(BRIDGE_JS).bordered(false);
            match (boot, source) {
                (true, _) => wv.html(STARTING_HTML),
                (false, Some(plugin::WebviewSource::Entry(entry))) => wv.serve(dir, entry),
                (false, Some(plugin::WebviewSource::Url(u))) => wv.url(u),
                (false, None) => wv,
            }
        });

        let sub = cx.subscribe(&webview, |this, _wv, event: &WebViewEvent, cx| {
            if let WebViewEvent::Message(payload) = event {
                this.on_message(payload.to_string(), cx);
            }
        });

        let mut this = Self {
            plugin,
            webview_id,
            webview,
            focus: cx.focus_handle(),
            _sub: sub,
        };
        if boot {
            this.boot_runtime(boot_url, cx);
        }
        this
    }

    /// Invoke the plugin runtime's `boot` method (Rust -> process, so it works
    /// even though the JS bridge can't from `file://`), then navigate the view
    /// to the address it returns: `{ url }`, or `{ port }` substituted into the
    /// manifest url's `{port}` placeholder. `{ error }` shows a failure page.
    fn boot_runtime(&mut self, url: String, cx: &mut Context<Self>) {
        let plugin = self.plugin.clone();
        let webview_id = self.webview_id.clone();
        let webview = self.webview.clone();
        let executor = cx.background_executor().clone();
        cx.spawn(async move |_this, cx| {
            let resp = executor
                .spawn(async move {
                    let req = pluginhost::Request {
                        kind: "message",
                        panel: &webview_id,
                        action: None,
                        cwd: None,
                        method: Some("boot"),
                        params: None,
                    };
                    pluginhost::invoke(&plugin, &req)
                })
                .await;
            let target: Result<String, String> = match resp {
                Ok(r) => match r.result {
                    Some(res) => {
                        if let Some(e) = res.get("error").and_then(Value::as_str) {
                            Err(e.to_string())
                        } else if let Some(u) = res.get("url").and_then(Value::as_str) {
                            Ok(u.to_string())
                        } else if let Some(p) = res.get("port").and_then(Value::as_u64) {
                            Ok(url.replace("{port}", &p.to_string()))
                        } else {
                            Ok(url.clone())
                        }
                    }
                    None => Ok(url.clone()),
                },
                Err(e) => Err(e),
            };
            let _ = webview.update(cx, |wv, cx| match target {
                Ok(u) => wv.load_url(u, cx),
                Err(e) => wv.load_html(failure_html(&e), cx),
            });
        })
        .detach();
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

        // Try the app's built-in capabilities on the main workspace window.
        let handle = cx
            .windows()
            .into_iter()
            .find_map(|w| w.downcast::<WorkspaceView>());
        let dispatched = handle.map(|h| {
            h.update(cx, |ws, window, cx| {
                ws.mcp_dispatch(&method, &params, window, cx)
            })
        });

        match dispatched {
            Some(Ok(Ok(value))) => self.resolve(id, true, &value, cx),
            // Not a built-in op: forward to the plugin's runtime, if any.
            Some(Ok(Err(e))) if e.starts_with("unknown op") && self.plugin.runtime.is_some() => {
                self.forward_to_runtime(id, method, params, cx);
            }
            Some(Ok(Err(e))) => self.resolve(id, false, &Value::String(e), cx),
            _ => self.resolve(id, false, &Value::String("workspace unavailable".into()), cx),
        }
    }

    /// Forward an unknown method to the plugin's `[runtime]` as a `message`
    /// request, off the UI thread, then resolve the page's promise.
    fn forward_to_runtime(
        &mut self,
        id: Option<i64>,
        method: String,
        params: Value,
        cx: &mut Context<Self>,
    ) {
        let plugin = self.plugin.clone();
        let webview_id = self.webview_id.clone();
        let cwd = self.focused_cwd(cx);
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let resp = executor
                .spawn(async move {
                    let req = pluginhost::Request {
                        kind: "message",
                        panel: &webview_id,
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
            .evaluate_script(&format!("window.__promptResolve({id}, {ok}, {vjson});"));
    }

    /// The focused pane's working directory on the main workspace, so a plugin
    /// runtime acts on the right place.
    fn focused_cwd(&self, cx: &mut Context<Self>) -> Option<String> {
        let handle = cx
            .windows()
            .into_iter()
            .find_map(|w| w.downcast::<WorkspaceView>())?;
        handle
            .update(cx, |ws, _window, cx| ws.focused_cwd(cx))
            .ok()
            .flatten()
            .map(|p| p.to_string_lossy().into_owned())
    }
}

impl PluginWebView {
    /// The webview surface's title, for the tab/pane label.
    pub fn title(&self) -> String {
        self.plugin
            .webview
            .as_ref()
            .map(|w| w.title.clone())
            .unwrap_or_else(|| self.plugin.name.clone())
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
