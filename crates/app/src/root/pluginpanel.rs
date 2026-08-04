//! Plugin side-drawer panels. A plugin with a `[panel]` contributes an
//! activity-bar icon and a drawer rendered from the block tree its `render`
//! export returns.
//!
//! The plugin's instance is resident, so opening a panel or clicking a button in
//! it is an in-process call, not a round trip: render, paint, done. Anything the
//! plugin wants the terminal to do it asks for by calling `run-command`, which
//! is queued during the call and drained afterwards by
//! [`WorkspaceView::drain_wasm_commands`].

use super::*;
use gpui::prelude::*;

use guise::prelude::{Badge, Button, ColorName, Divider, Size, Text, Title, Variant};

use crate::panelui::{Block, Response};

impl WorkspaceView {
    /// Plugins that contribute a panel (have both a `[runtime]` and `[panel]`),
    /// in load order. `SidebarPanel::Plugin(i)` indexes this list.
    pub(crate) fn plugin_panel_defs(&self) -> Vec<&plugin::Plugin> {
        self.plugins
            .iter()
            .filter(|p| p.runtime.is_some() && p.panel.is_some())
            .collect()
    }

    /// Open a web view as a tab in the focused pane. Mirrors `newtab` but with a
    /// webview item instead of a shell.
    pub(crate) fn open_webview_tab(
        &mut self,
        surface: crate::webview::WebviewSurface,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let id = self.spawn_webview_item(surface, cx);
        self.group.update(cx, |g, cx| g.add_to_focused(id, cx));
        self.focusactive(window, cx);
        cx.notify();
    }

    /// Header label for a panel. Plugin panels prefer the title from their
    /// latest response (so a plugin can retitle live), falling back to the
    /// manifest's `[panel] title`.
    pub(crate) fn panel_label_of(&self, panel: SidebarPanel) -> String {
        let SidebarPanel::Plugin(i) = panel else {
            return panel.label().to_string();
        };
        let Some(decl) = panel_defs(&self.plugins).nth(i).and_then(|p| p.panel.as_ref()) else {
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
            SidebarPanel::Plugin(i) => panel_defs(&self.plugins)
                .nth(i)
                .and_then(|p| p.panel.as_ref())
                .map(|pn| pn.icon.clone())
                .unwrap_or_else(|| "\u{25c9}".to_string()),
            other => other.icon().to_string(),
        }
    }

    /// The `Action::Sidebar` token for a panel: `terminals` for built-ins,
    /// `plugin:<id>` for plugin panels.
    pub(crate) fn panel_token_of(&self, panel: SidebarPanel) -> String {
        token_of(&self.plugins, panel)
    }

    /// Resolve a sidebar token (`terminals` or `plugin:<id>`) to a panel.
    pub(crate) fn panel_from_token(&self, token: &str) -> Option<SidebarPanel> {
        from_token(&self.plugins, token)
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
            eprintln!("sinclair: {}: {}", d.key, d.message);
        }
        self.keybinds = keybinds;
        self.applykeybinds(cx);
        self.setmenus(cx);
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

    /// The focused item's working directory, passed to plugins so they act on
    /// the right place.
    pub(crate) fn focused_cwd(&self, cx: &App) -> Option<std::path::PathBuf> {
        let item = self.group.read(cx).active_item();
        self.items
            .borrow()
            .get(&item)
            .and_then(|it| it.content.cwd_path(cx))
    }

    /// (Re)render a plugin panel. The plugin's instance is resident, so this is
    /// an in-process call measured in microseconds — there is nothing to move
    /// off the UI thread.
    pub(crate) fn refresh_plugin_panel(&mut self, index: usize, cx: &mut Context<Self>) {
        let plugin = match self.plugin_panel_defs().get(index) {
            Some(p) => (*p).clone(),
            None => return,
        };
        let Some(panel_id) = plugin.panel.as_ref().map(|pn| pn.id.clone()) else {
            return;
        };
        self.render_wasm_panel(&plugin, &panel_id, cx);
    }

    /// Put whatever the last wasm call asked for onto the clipboard. The host
    /// can't touch the gpui context mid-call, so it parks the text and we apply
    /// it here, the way queued commands are applied.
    pub(crate) fn apply_wasm_clipboard(&mut self, cx: &mut Context<Self>) {
        let Some(text) = self.gui_wasm.as_ref().and_then(|gw| gw.take_clipboard_write()) else {
            return;
        };
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
    }

    /// Run whatever the last wasm call queued through `run-command`. The host
    /// can't reach the workspace mid-call, so it parks the directives and every
    /// caller drains them here once the guest has returned.
    pub(crate) fn drain_wasm_commands(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let commands = self
            .gui_wasm
            .as_ref()
            .map(|gw| gw.take_commands())
            .unwrap_or_default();
        for c in commands {
            let _ = self.mcp_dispatch(
                "run_command",
                &json!({ "text": c.text, "target": c.target }),
                window,
                cx,
            );
        }
    }

    /// Lazily create the GUI-side wasm runtime (one engine per window).
    pub(crate) fn ensure_gui_wasm(&mut self) -> Option<&mut crate::guiwasm::GuiWasm> {
        if self.gui_wasm.is_none() {
            self.gui_wasm = crate::guiwasm::GuiWasm::new();
        }
        self.gui_wasm.as_mut()
    }

    /// Render a wasm plugin's panel synchronously into the panel cache.
    fn render_wasm_panel(&mut self, plugin: &plugin::Plugin, panel_id: &str, cx: &mut Context<Self>) {
        let cwd = self.focused_cwd(cx);
        let clipboard = cx.read_from_clipboard().and_then(|i| i.text());
        let rendered = match self.ensure_gui_wasm() {
            Some(gw) => {
                gw.set_context(cwd, clipboard);
                gw.render(plugin)
            }
            None => Err("wasm runtime unavailable".to_string()),
        };
        self.apply_wasm_clipboard(cx);
        let response = match rendered {
            Ok(json) => serde_json::from_str::<Response>(&json)
                .unwrap_or_else(|e| error_response(&format!("bad panel: {e}"))),
            Err(e) => error_response(&e),
        };
        self.plugin_panels.insert(panel_id.to_string(), response);
        cx.notify();
    }

    /// Deliver a wasm panel button click: run `on_ui_event`, dispatch any queued
    /// commands, then re-render — all in-process on the UI thread.
    fn wasm_ui_event(
        &mut self,
        plugin: &plugin::Plugin,
        panel_id: &str,
        action: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let event = json!({ "id": action }).to_string();
        let cwd = self.focused_cwd(cx);
        let clipboard = cx.read_from_clipboard().and_then(|i| i.text());
        let result = match self.ensure_gui_wasm() {
            Some(gw) => {
                gw.set_context(cwd, clipboard);
                gw.on_ui_event(plugin, &event)
            }
            None => Err("wasm runtime unavailable".to_string()),
        };
        self.apply_wasm_clipboard(cx);
        self.drain_wasm_commands(window, cx);
        if let Err(e) = result {
            self.plugin_panels.insert(panel_id.to_string(), error_response(&e));
            cx.notify();
        } else {
            self.render_wasm_panel(plugin, panel_id, cx);
        }
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
        let plugin = match self.plugins.iter().find(|p| {
            p.runtime.is_some() && p.panel.as_ref().map(|pn| pn.id.as_str()) == Some(panel_id)
        }) {
            Some(p) => p.clone(),
            None => return,
        };
        self.wasm_ui_event(&plugin, panel_id, action, window, cx);
    }

    /// Render a plugin panel's body from its cached block tree.
    pub(crate) fn panel_plugin(&self, panel: SidebarPanel, cx: &mut Context<Self>) -> AnyElement {
        let SidebarPanel::Plugin(index) = panel else {
            return div().into_any_element();
        };
        let panel_id = match panel_defs(&self.plugins).nth(index).and_then(|p| p.panel.as_ref()) {
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
            Block::Text {
                text,
                dimmed,
                color,
                mono,
            } => {
                let t = Text::new(text.clone()).size(Size::Sm);
                let t = match text_color(color, cx) {
                    // An explicit color wins over `dimmed`; asking for both is
                    // contradictory and the color is the more specific request.
                    Some(c) => t.color(c),
                    None if *dimmed => t.dimmed(),
                    None => t,
                };
                let wrapper = div().px_2();
                let wrapper = if *mono {
                    wrapper.font_family(self.font.family.clone())
                } else {
                    wrapper
                };
                wrapper.child(t).into_any_element()
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
            // An unrecognized block renders an inline notice rather than blanking
            // the whole panel — the resilience the single-parse path now allows.
            Block::Unknown => div()
                .px_2()
                .child(Text::new("(unsupported block)").size(Size::Sm).dimmed())
                .into_any_element(),
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
            color: None,
            mono: false,
        }],
    }
}

/// Resolve a block's `color` name to a themed text color. Shade 4 reads on both
/// the light and dark surfaces; the palette's mid shades are tuned for fills,
/// not for type on a panel.
fn text_color(color: &Option<String>, cx: &App) -> Option<guise::theme::Color> {
    let name = color.as_ref()?;
    let named = badge_color(&Some(name.clone()));
    Some(guise::theme::theme(cx).palette.get(named, 4))
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

// --- token encoding, without a WorkspaceView -------------------------------
//
// The dock composition is built before `WorkspaceView` exists (in `new`) and is
// also edited from the settings window, which has no workspace at all. Both
// only ever need the loaded plugin list, so the encode/decode pair lives here
// as free functions and the methods above delegate to them.

// These are lookups, not lists: every caller wants one entry by index, or a
// count. Iterating to `nth` avoids collecting a `Vec<&Plugin>` on a path the
// render loop reaches once per visible section per repaint.

/// Plugins contributing a `[panel]`, in load order — what `Plugin(i)` indexes.
fn panel_defs(plugins: &[plugin::Plugin]) -> impl Iterator<Item = &plugin::Plugin> {
    plugins
        .iter()
        .filter(|p| p.runtime.is_some() && p.panel.is_some())
}

/// A panel's stable token. Plugin sections encode their manifest id, never
/// their index, because the index moves when the plugin set changes.
pub(crate) fn token_of(plugins: &[plugin::Plugin], panel: SidebarPanel) -> String {
    match panel {
        SidebarPanel::Plugin(i) => format!(
            "plugin:{}",
            panel_defs(plugins)
                .nth(i)
                .and_then(|p| p.panel.as_ref())
                .map(|pn| pn.id.as_str())
                .unwrap_or("")
        ),
        other => other.id().to_string(),
    }
}

/// Resolve a token back to a panel, or `None` when nothing answers to it —
/// which is what an uninstalled plugin looks like. Callers skip those rather
/// than rewriting the config, so a reinstall restores the section's slot.
pub(crate) fn from_token(plugins: &[plugin::Plugin], token: &str) -> Option<SidebarPanel> {
    if let Some(id) = token.strip_prefix("plugin:") {
        let i = panel_defs(plugins)
            .position(|p| p.panel.as_ref().map(|pn| pn.id.as_str()) == Some(id))?;
        return Some(SidebarPanel::Plugin(i));
    }
    SidebarPanel::from_id(token)
}

/// A section's display name without a live workspace. Plugin sections read
/// their manifest title; `WorkspaceView::panel_label_of` additionally prefers a
/// title the running plugin has reported, which only the workspace can know.
pub(crate) fn label_of(plugins: &[plugin::Plugin], panel: SidebarPanel) -> String {
    match panel {
        SidebarPanel::Plugin(i) => panel_defs(plugins)
            .nth(i)
            .and_then(|p| p.panel.as_ref())
            .map(|pn| pn.title.clone())
            .unwrap_or_else(|| "Plugin".to_string()),
        other => other.label().to_string(),
    }
}

/// Every plugin-contributed section, in load order.
pub(crate) fn contributed(plugins: &[plugin::Plugin]) -> Vec<SidebarPanel> {
    (0..panel_defs(plugins).count())
        .map(SidebarPanel::Plugin)
        .collect()
}

/// Build both docks from settings plus the loaded plugins: resolve the
/// configured tokens, then give any newly installed plugin panel a home.
pub(crate) fn compose_docks(opts: &config::Options, plugins: &[plugin::Plugin]) -> Docks {
    let resolve = |t: &str| from_token(plugins, t);
    let tokenize = |p: SidebarPanel| token_of(plugins, p);
    let mut docks = [
        dock::compose(
            SidebarSide::Left,
            &opts.sidebar_left,
            &opts.sidebar_collapsed,
            opts.sidebar_left_width as f32,
            resolve,
            tokenize,
        ),
        dock::compose(
            SidebarSide::Right,
            &opts.sidebar_right,
            &opts.sidebar_collapsed,
            opts.sidebar_right_width as f32,
            resolve,
            tokenize,
        ),
    ];
    dock::place_unplaced(&mut docks, &contributed(plugins));
    docks
}
