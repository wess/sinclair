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

    /// The full activity-bar panel list: built-ins followed by plugin panels.
    pub(crate) fn panel_list(&self) -> Vec<SidebarPanel> {
        let mut list = SidebarPanel::ALL.to_vec();
        for i in 0..self.plugin_panel_defs().len() {
            list.push(SidebarPanel::Plugin(i));
        }
        list
    }

    /// Header label for a panel. Plugin panels prefer the title from their
    /// latest response (so a plugin can retitle live), falling back to the
    /// manifest's `[panel] title`.
    pub(crate) fn panel_label_of(&self, panel: SidebarPanel) -> String {
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
        SidebarPanel::from_id(token)
    }

    /// The focused pane's working directory, passed to plugins so they act on
    /// the right place.
    fn focused_cwd(&self, cx: &App) -> Option<std::path::PathBuf> {
        self.panes
            .get(&self.tabs.focused())
            .and_then(|p| p.view.read(cx).cwd_path())
    }

    /// (Re)render a plugin panel by invoking its runtime with a `render`
    /// request and caching the response. Synchronous; the plugin is short-lived.
    pub(crate) fn refresh_plugin_panel(&mut self, index: usize, cx: &mut Context<Self>) {
        let plugin = match self.plugin_panel_defs().get(index) {
            Some(p) => (*p).clone(),
            None => return,
        };
        let panel_id = match plugin.panel.as_ref() {
            Some(pn) => pn.id.clone(),
            None => return,
        };
        let cwd = self.focused_cwd(cx);
        let cwd = cwd.as_ref().map(|p| p.to_string_lossy().into_owned());
        let req = pluginhost::Request {
            kind: "render",
            panel: &panel_id,
            action: None,
            cwd: cwd.as_deref(),
        };
        let resp = pluginhost::invoke(&plugin, &req).unwrap_or_else(|e| error_response(&e));
        self.plugin_panels.insert(panel_id, resp);
        cx.notify();
    }

    /// Handle a button click in a plugin panel: invoke the runtime with an
    /// `action` request, run any returned directives, and re-cache the panel.
    pub(crate) fn plugin_action(
        &mut self,
        panel_id: &str,
        action: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let plugin = match self
            .plugins
            .iter()
            .find(|p| {
                p.runtime.is_some() && p.panel.as_ref().map(|pn| pn.id.as_str()) == Some(panel_id)
            }) {
            Some(p) => p.clone(),
            None => return,
        };
        let cwd = self.focused_cwd(cx);
        let cwd = cwd.as_ref().map(|p| p.to_string_lossy().into_owned());
        let req = pluginhost::Request {
            kind: "action",
            panel: panel_id,
            action: Some(action),
            cwd: cwd.as_deref(),
        };
        let resp = pluginhost::invoke(&plugin, &req).unwrap_or_else(|e| error_response(&e));
        for run in &resp.run {
            let target = run.target.clone().unwrap_or_else(|| "pane".to_string());
            let _ = self.mcp_dispatch(
                "run_command",
                &json!({ "text": run.text, "target": target }),
                window,
                cx,
            );
        }
        self.plugin_panels.insert(panel_id.to_string(), resp);
        cx.notify();
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
                let blocks = resp.blocks.clone();
                for (i, block) in blocks.iter().enumerate() {
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
                    row = row.child(self.render_block(panel_id, idx * 100 + j, child, cx));
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
