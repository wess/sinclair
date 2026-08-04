//! Host for a first-party web-view surface — today, Notes.
//!
//! The surface is backed by a **host-managed sidecar**: the host reserves a
//! loopback port, mints a token, spawns the bundled server with both in its
//! environment (`SINCLAIR_SERVICE_PORT` / `SINCLAIR_SERVICE_TOKEN`), and points
//! the page at it. The child is refcounted and shared by every surface with the
//! same id, and reaped when the last one closes — see [`crate::sidecar`].
//!
//! There is no JavaScript bridge here. The page talks to its own server over
//! that authenticated origin, which is why the surface needs no privileged
//! channel into the app.

use std::path::PathBuf;

use gpui::prelude::*;
use gpui::{div, Context, Entity, FocusHandle, Subscription, Window};

use guise::WebView;

/// What a [`SurfaceView`] hosts: identity plus the sidecar that serves it.
pub struct WebviewSurface {
    /// Stable id; the sidecar's refcount key and the tab token.
    pub id: String,
    /// Tab/window title.
    pub title: String,
    /// The command to run, and the writable directory to run it in — a bundled
    /// sidecar's own directory may be read-only (it lives inside the app).
    pub command: String,
    pub dir: PathBuf,
}

/// Shown while the sidecar starts up.
const STARTING_HTML: &str = "<!doctype html><html><head><meta charset=\"utf-8\">\
<style>body{background:#1c1c1e;color:#8a8a90;font:13px -apple-system,system-ui,sans-serif;\
display:flex;align-items:center;justify-content:center;height:100vh;margin:0}</style></head>\
<body>Starting\u{2026}</body></html>";

/// A failure page for a surface whose sidecar didn't come up.
fn failure_html(err: &str) -> String {
    let safe = err.replace('<', "&lt;").replace('>', "&gt;");
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\">\
<style>body{{background:#1c1c1e;color:#e0a0a0;font:13px -apple-system,system-ui,sans-serif;\
display:flex;align-items:center;justify-content:center;height:100vh;margin:0;padding:20px;\
text-align:center}}</style></head><body>Couldn't start: {safe}</body></html>"
    )
}

pub struct SurfaceView {
    surface: WebviewSurface,
    webview: Entity<WebView>,
    focus: FocusHandle,
    /// The sidecar service this surface holds a reference on; given back on
    /// drop so the host reaps the child when the last user closes.
    service: Option<String>,
    _sub: Subscription,
}

impl SurfaceView {
    pub fn new(surface: WebviewSurface, cx: &mut Context<Self>) -> Self {
        let webview = cx.new(|cx| WebView::new(cx).bordered(false).html(STARTING_HTML));
        // Subscribed but ignored: `WebView` emits page messages, and holding the
        // subscription keeps the channel from being torn down under the page.
        let sub = cx.subscribe(&webview, |_this, _wv, _event, _cx| {});

        let mut this = Self {
            surface,
            webview,
            focus: cx.focus_handle(),
            service: None,
            _sub: sub,
        };
        this.boot(cx);
        this
    }

    /// Show or hide the native web view. A tab host must hide it when the
    /// surface is no longer on screen, or the OS view lingers at its last
    /// position over whatever replaced it.
    pub fn set_visible(&self, visible: bool, cx: &mut Context<Self>) {
        self.webview.update(cx, |wv, _| wv.set_visible(visible));
    }

    /// Start the sidecar, then navigate to it. A failure shows a page.
    fn boot(&mut self, cx: &mut Context<Self>) {
        let id = self.surface.id.clone();
        let command = self.surface.command.clone();
        let dir = self.surface.dir.clone();
        let webview = self.webview.clone();
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let key = id.clone();
            let target = executor
                .spawn(async move {
                    crate::sidecar::acquire(&key, &command, &dir)
                        .map(|(port, token)| format!("http://127.0.0.1:{port}/?token={token}"))
                })
                .await;
            match target {
                Ok(url) => {
                    let live = this.update(cx, |view, cx| {
                        view.service = Some(id.clone());
                        view.webview.update(cx, |wv, cx| wv.load_url(url, cx));
                    });
                    // The surface closed while the service was starting: give
                    // the reference straight back so it is reaped.
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

    /// The surface's title, for the tab label.
    pub fn title(&self) -> String {
        self.surface.title.clone()
    }
}

impl Drop for SurfaceView {
    /// Closing the last surface that uses a sidecar reaps its child. Every path
    /// that discards the host lands here: closing the tab, and app teardown.
    fn drop(&mut self) {
        if let Some(id) = self.service.take() {
            crate::sidecar::release(&id);
        }
    }
}

impl gpui::Focusable for SurfaceView {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for SurfaceView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .track_focus(&self.focus)
            .child(self.webview.clone())
    }
}
