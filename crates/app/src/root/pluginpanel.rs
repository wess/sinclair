//! IPC plugin side-drawer panels. A plugin with a `[panel]` contributes an
//! activity-bar icon and a drawer rendered from the block tree its `[runtime]`
//! returns. Opening the panel (or clicking a button in it) invokes the plugin
//! over [`crate::pluginhost`]; the reply's blocks are rendered with guise and
//! its `run` directives are executed in the terminal via the MCP dispatch.

use super::*;
use gpui::prelude::*;

use guise::prelude::{Badge, Button, ColorName, Divider, Size, Text, Title, Variant};

use crate::pluginhost::{self, Block, Response};

impl WorkspaceView {
    /// Plugins that contribute a panel (have both a `[runtime]` and `[panel]`),
    /// in load order. `SidebarPanel::Plugin(i)` indexes this list.
    pub(crate) fn plugin_panel_defs(&self) -> Vec<&plugin::Plugin> {
        self.plugins
            .iter()
            .filter(|p| p.runtime.is_some() && p.panel.is_some())
            .collect()
    }

    /// Plugins with a `[webview]` whose placement is `panel`, in load order.
    /// `SidebarPanel::Webview(i)` indexes this list.
    pub(crate) fn plugin_webview_panel_defs(&self) -> Vec<&plugin::Plugin> {
        self.plugins
            .iter()
            .filter(|p| {
                p.webview
                    .as_ref()
                    .is_some_and(|w| w.placement == plugin::Placement::Panel)
            })
            .collect()
    }

    /// The full activity-bar panel list: built-ins, then plugin block panels,
    /// then plugin webview panels.
    pub(crate) fn panel_list(&self) -> Vec<SidebarPanel> {
        let mut list = SidebarPanel::ALL.to_vec();
        for i in 0..self.plugin_panel_defs().len() {
            list.push(SidebarPanel::Plugin(i));
        }
        for i in 0..self.plugin_webview_panel_defs().len() {
            list.push(SidebarPanel::Webview(i));
        }
        list
    }

    /// (Re)build the webview host entities for panel-placement `[webview]`
    /// plugins. The entity wrapper is cheap; the native page is created on the
    /// panel's first render. Called at load and after a plugin reload.
    pub(crate) fn rebuild_webview_hosts(&mut self, cx: &mut Context<Self>) {
        self.webview_hosts.clear();
        let defs: Vec<plugin::Plugin> = self
            .plugins
            .iter()
            .filter(|p| {
                p.webview
                    .as_ref()
                    .is_some_and(|w| w.placement == plugin::Placement::Panel)
            })
            .cloned()
            .collect();
        for plugin in defs {
            let id = plugin
                .webview
                .as_ref()
                .map(|w| w.id.clone())
                .unwrap_or_else(|| plugin.id.clone());
            let host = cx.new(|cx| crate::pluginwebview::PluginWebView::new(plugin, cx));
            self.webview_hosts.insert(id, host);
        }
    }

    /// Render a plugin webview panel's body: the stored host entity, or a note.
    pub(crate) fn panel_webview(&self, panel: SidebarPanel, _cx: &mut Context<Self>) -> AnyElement {
        let SidebarPanel::Webview(index) = panel else {
            return div().into_any_element();
        };
        let id = match self
            .plugin_webview_panel_defs()
            .get(index)
            .and_then(|p| p.webview.as_ref())
        {
            Some(w) => w.id.clone(),
            None => return div().into_any_element(),
        };
        match self.webview_hosts.get(&id) {
            Some(host) => div()
                .flex_1()
                .min_h(px(0.0))
                .child(host.clone())
                .into_any_element(),
            None => self.plugin_note("Loading\u{2026}"),
        }
    }

    /// Open a plugin's `[webview]` per its manifest placement: a sidebar panel,
    /// or a standalone window (also the current fallback for `tab`).
    pub(crate) fn open_webview(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(plugin) = self
            .plugins
            .iter()
            .find(|p| p.webview.as_ref().map(|w| w.id.as_str()) == Some(id))
            .cloned()
        else {
            return;
        };
        let placement = plugin
            .webview
            .as_ref()
            .map(|w| w.placement)
            .unwrap_or_default();
        match placement {
            plugin::Placement::Panel => self.toggle_sidebar(&format!("right:webview:{id}"), cx),
            plugin::Placement::Tab => self.open_webview_tab(plugin, window, cx),
            plugin::Placement::Window => crate::pluginwindow::open(window, plugin, cx),
        }
    }

    /// Open a plugin web view as its own tab (a single-pane tab hosting the
    /// webview). Mirrors `newtab` but with a webview pane instead of a shell.
    fn open_webview_tab(
        &mut self,
        plugin: plugin::Plugin,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.zoomed = false;
        let id = self.spawn_webview_pane(plugin, cx);
        self.tabs.new_tab(id);
        self.focusactive(window, cx);
        cx.notify();
    }

    /// Header label for a panel. Plugin panels prefer the title from their
    /// latest response (so a plugin can retitle live), falling back to the
    /// manifest's `[panel] title`.
    pub(crate) fn panel_label_of(&self, panel: SidebarPanel) -> String {
        if let SidebarPanel::Webview(i) = panel {
            return self
                .plugin_webview_panel_defs()
                .get(i)
                .and_then(|p| p.webview.as_ref())
                .map(|w| w.title.clone())
                .unwrap_or_else(|| "Webview".to_string());
        }
        let SidebarPanel::Plugin(i) = panel else {
            return panel.label().to_string();
        };
        let defs = self.plugin_panel_defs();
        let Some(decl) = defs.get(i).and_then(|p| p.panel.as_ref()) else {
            return "Plugin".to_string();
        };
        if let Some(title) = self
            .plugin_panels
            .get(&decl.id)
            .and_then(|r| r.title.as_ref())
            .filter(|s| !s.trim().is_empty())
        {
            return title.clone();
        }
        decl.title.clone()
    }

    /// Activity-bar glyph for a panel, resolving plugin panels by their manifest.
    pub(crate) fn panel_icon_of(&self, panel: SidebarPanel) -> String {
        match panel {
            SidebarPanel::Plugin(i) => self
                .plugin_panel_defs()
                .get(i)
                .and_then(|p| p.panel.as_ref())
                .map(|pn| pn.icon.clone())
                .unwrap_or_else(|| "\u{25c9}".to_string()),
            SidebarPanel::Webview(i) => self
                .plugin_webview_panel_defs()
                .get(i)
                .and_then(|p| p.webview.as_ref())
                .map(|w| w.icon.clone())
                .unwrap_or_else(|| "\u{25f1}".to_string()),
            other => other.icon().to_string(),
        }
    }

    /// The `Action::Sidebar` token for a panel: `terminals` for built-ins,
    /// `plugin:<id>` for plugin panels.
    pub(crate) fn panel_token_of(&self, panel: SidebarPanel) -> String {
        match panel {
            SidebarPanel::Plugin(i) => format!(
                "plugin:{}",
                self.plugin_panel_defs()
                    .get(i)
                    .and_then(|p| p.panel.as_ref())
                    .map(|pn| pn.id.as_str())
                    .unwrap_or("")
            ),
            SidebarPanel::Webview(i) => format!(
                "webview:{}",
                self.plugin_webview_panel_defs()
                    .get(i)
                    .and_then(|p| p.webview.as_ref())
                    .map(|w| w.id.as_str())
                    .unwrap_or("")
            ),
            other => other.id().to_string(),
        }
    }

    /// Resolve a sidebar token (`terminals` or `plugin:<id>`) to a panel.
    pub(crate) fn panel_from_token(&self, token: &str) -> Option<SidebarPanel> {
        if let Some(id) = token.strip_prefix("plugin:") {
            let i = self
                .plugin_panel_defs()
                .iter()
                .position(|p| p.panel.as_ref().map(|pn| pn.id.as_str()) == Some(id))?;
            return Some(SidebarPanel::Plugin(i));
        }
        if let Some(id) = token.strip_prefix("webview:") {
            let i = self
                .plugin_webview_panel_defs()
                .iter()
                .position(|p| p.webview.as_ref().map(|w| w.id.as_str()) == Some(id))?;
            return Some(SidebarPanel::Webview(i));
        }
        SidebarPanel::from_id(token)
    }

    /// Fetch the installable catalog off-thread (the GitHub API call would
    /// otherwise block the UI), then update the panel.
    pub(crate) fn fetch_catalog(&mut self, cx: &mut Context<Self>) {
        if self.catalog_loading {
            return;
        }
        self.catalog_loading = true;
        self.catalog_status = None;
        cx.notify();
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = executor.spawn(async { crate::catalog::list() }).await;
            let _ = this.update(cx, |view, cx| {
                view.catalog_loading = false;
                match result {
                    Ok(names) => {
                        view.catalog = Some(names);
                        view.catalog_status = None;
                    }
                    Err(e) => {
                        view.catalog = Some(Vec::new());
                        view.catalog_status = Some(format!("Catalog unavailable: {e}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Install a catalog plugin off-thread (download), then reload plugins on
    /// the foreground.
    pub(crate) fn install_catalog_plugin(&mut self, name: &str, cx: &mut Context<Self>) {
        let name = name.to_string();
        self.catalog_status = Some(format!("Installing {name}\u{2026}"));
        cx.notify();
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let target = name.clone();
            let result = executor
                .spawn(async move { crate::catalog::install(&target) })
                .await;
            let _ = this.update(cx, |view, cx| {
                match result {
                    Ok(_) => {
                        view.catalog_status = Some(format!("Installed {name}"));
                        view.reload_plugins(cx);
                    }
                    Err(e) => view.catalog_status = Some(format!("Install {name} failed: {e}")),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Reload plugins from disk and re-resolve keybindings (after an install).
    pub(crate) fn reload_plugins(&mut self, cx: &mut Context<Self>) {
        self.plugins = loadplugins(&self.opts);
        let (keybinds, diags) = resolvekeys(&self.opts, &self.plugins);
        for d in &diags {
            eprintln!("prompt: {}: {}", d.key, d.message);
        }
        self.keybinds = keybinds;
        self.applykeybinds(cx);
        self.rebuild_webview_hosts(cx);
    }

    /// Plugin ids currently installed (folder/manifest ids).
    pub(crate) fn installed_ids(&self) -> std::collections::HashSet<String> {
        self.plugins.iter().map(|p| p.id.clone()).collect()
    }

    /// Catalog names with no matching installed plugin id.
    pub(crate) fn available_plugins(&self) -> Vec<String> {
        let installed = self.installed_ids();
        self.catalog
            .as_ref()
            .map(|names| {
                names
                    .iter()
                    .filter(|n| !installed.contains(*n))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The focused pane's working directory, passed to plugins so they act on
    /// the right place.
    pub(crate) fn focused_cwd(&self, cx: &App) -> Option<std::path::PathBuf> {
        self.panes
            .get(&self.tabs.focused())
            .and_then(|p| p.content.cwd_path(cx))
    }

    /// (Re)render a plugin panel by invoking its runtime with a `render`
    /// request off the UI thread, caching the response when it returns.
    pub(crate) fn refresh_plugin_panel(&mut self, index: usize, cx: &mut Context<Self>) {
        let plugin = match self.plugin_panel_defs().get(index) {
            Some(p) => (*p).clone(),
            None => return,
        };
        let panel_id = match plugin.panel.as_ref() {
            Some(pn) => pn.id.clone(),
            None => return,
        };
        let cwd = self.focused_cwd(cx).map(|p| p.to_string_lossy().into_owned());
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let pid = panel_id.clone();
            let resp = executor
                .spawn(async move {
                    let req = pluginhost::Request {
                        kind: "render",
                        panel: &pid,
                        action: None,
                        cwd: cwd.as_deref(),
                        method: None,
                        params: None,
                    };
                    pluginhost::invoke(&plugin, &req)
                })
                .await;
            let _ = this.update(cx, |view, cx| {
                view.plugin_panels
                    .insert(panel_id, resp.unwrap_or_else(|e| error_response(&e)));
                cx.notify();
            });
        })
        .detach();
    }

    /// Handle a button click in a plugin panel off the UI thread: invoke the
    /// runtime with an `action` request, then run any returned directives and
    /// re-cache the panel on the foreground.
    pub(crate) fn plugin_action(
        &mut self,
        panel_id: &str,
        action: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(handle) = window.window_handle().downcast::<WorkspaceView>() else {
            return;
        };
        let plugin = match self.plugins.iter().find(|p| {
            p.runtime.is_some() && p.panel.as_ref().map(|pn| pn.id.as_str()) == Some(panel_id)
        }) {
            Some(p) => p.clone(),
            None => return,
        };
        let panel = panel_id.to_string();
        let action = action.to_string();
        let cwd = self.focused_cwd(cx).map(|p| p.to_string_lossy().into_owned());
        let executor = cx.background_executor().clone();
        cx.spawn(async move |_this, cx| {
            let p = panel.clone();
            let resp = executor
                .spawn(async move {
                    let req = pluginhost::Request {
                        kind: "action",
                        panel: &p,
                        action: Some(&action),
                        cwd: cwd.as_deref(),
                        method: None,
                        params: None,
                    };
                    pluginhost::invoke(&plugin, &req)
                })
                .await;
            let resp = resp.unwrap_or_else(|e| error_response(&e));
            let _ = handle.update(cx, |view, window, cx| {
                for run in &resp.run {
                    let target = run.target.as_deref().unwrap_or("pane");
                    let _ = view.mcp_dispatch(
                        "run_command",
                        &json!({ "text": run.text, "target": target }),
                        window,
                        cx,
                    );
                }
                view.plugin_panels.insert(panel, resp);
                cx.notify();
            });
        })
        .detach();
    }

    /// Render a plugin panel's body from its cached block tree.
    pub(crate) fn panel_plugin(&self, panel: SidebarPanel, cx: &mut Context<Self>) -> AnyElement {
        let SidebarPanel::Plugin(index) = panel else {
            return div().into_any_element();
        };
        let panel_id = match self.plugin_panel_defs().get(index).and_then(|p| p.panel.as_ref()) {
            Some(pn) => pn.id.clone(),
            None => return div().into_any_element(),
        };
        let mut body = div()
            .id("sb-plugin")
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_1()
            .py_1();
        match self.plugin_panels.get(&panel_id) {
            None => {
                body = body.child(self.plugin_note("Loading\u{2026}"));
            }
            Some(resp) if resp.blocks.is_empty() => {
                body = body.child(self.plugin_note("No content."));
            }
            Some(resp) => {
                for (i, block) in resp.blocks.iter().enumerate() {
                    body = body.child(self.render_block(&panel_id, i, block, cx));
                }
            }
        }
        body.into_any_element()
    }

    fn plugin_note(&self, text: &str) -> AnyElement {
        div()
            .px_3()
            .py_2()
            .child(Text::new(text.to_string()).size(Size::Sm).dimmed())
            .into_any_element()
    }

    fn render_block(
        &self,
        panel_id: &str,
        idx: usize,
        block: &Block,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match block {
            Block::Section { title } => div()
                .px_2()
                .pt_2()
                .pb_1()
                .child(Title::new(title.clone()).order(6))
                .into_any_element(),
            Block::Text { text, dimmed } => {
                let t = Text::new(text.clone()).size(Size::Sm);
                let t = if *dimmed { t.dimmed() } else { t };
                div().px_2().child(t).into_any_element()
            }
            Block::Divider => div().px_2().py_1().child(Divider::new()).into_any_element(),
            Block::Kv { key, value } => div()
                .flex()
                .flex_row()
                .justify_between()
                .gap_2()
                .px_2()
                .child(Text::new(key.clone()).size(Size::Sm).dimmed())
                .child(Text::new(value.clone()).size(Size::Sm))
                .into_any_element(),
            Block::Badge { label, color } => div()
                .px_2()
                .child(Badge::new(label.clone()).color(badge_color(color)))
                .into_any_element(),
            Block::Button { id, label, variant } => {
                let entity = cx.entity();
                let panel = panel_id.to_string();
                let action = id.clone();
                div()
                    .px_2()
                    .py(px(2.0))
                    .child(
                        Button::new(("plugin-btn", idx), label.clone())
                            .size(Size::Sm)
                            .variant(button_variant(variant))
                            .full_width(true)
                            .on_click(move |_ev, window, app| {
                                let panel = panel.clone();
                                let action = action.clone();
                                entity.update(app, |this, cx| {
                                    this.plugin_action(&panel, &action, window, cx);
                                });
                            }),
                    )
                    .into_any_element()
            }
            Block::Row { children } => {
                let mut row = div().flex().flex_row().items_center().gap_2().px_2();
                for (j, child) in children.iter().enumerate() {
                    // Reserve a wide range per parent so child ids can't collide
                    // with sibling top-level block ids.
                    row = row.child(self.render_block(panel_id, idx * 1_000_000 + j, child, cx));
                }
                row.into_any_element()
            }
        }
    }
}

/// A plugin error rendered as a single dimmed line.
fn error_response(msg: &str) -> Response {
    Response {
        title: None,
        blocks: vec![Block::Text {
            text: format!("Plugin error: {msg}"),
            dimmed: true,
        }],
        run: Vec::new(),
        result: None,
    }
}

fn badge_color(color: &Option<String>) -> ColorName {
    match color.as_deref() {
        Some("red") => ColorName::Red,
        Some("green") => ColorName::Green,
        Some("yellow") | Some("amber") => ColorName::Yellow,
        Some("orange") => ColorName::Orange,
        Some("teal") => ColorName::Teal,
        Some("gray") | Some("grey") => ColorName::Gray,
        _ => ColorName::Blue,
    }
}

fn button_variant(variant: &Option<String>) -> Variant {
    match variant.as_deref() {
        Some("filled") => Variant::Filled,
        Some("outline") => Variant::Outline,
        Some("subtle") => Variant::Subtle,
        Some("default") => Variant::Default,
        Some("transparent") => Variant::Transparent,
        Some("white") => Variant::White,
        _ => Variant::Light,
    }
}
