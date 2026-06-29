//! VS Code / zed.dev-style side drawers. A drawer pairs a thin activity bar of
//! panel icons with the active panel's content; both left and right drawers
//! reuse the same renderers. Panels: Terminals (tab/pane tree), Layouts (tile
//! presets + saved layouts), Relay (live server status from the `RelayStatus`
//! global), and Agents (saved agent definitions to launch). Hidden by default;
//! toggled from the View menu (see `menus::sidebar_menu`).

use super::*;
use gpui::prelude::*;
use guise::prelude::{Size, Text};

/// Attention accent, matching the tab strip's pending-notification dot.
const ATTENTION: theme::Rgb = theme::Rgb::new(255, 196, 0);

const ACTIVITY_W: f32 = 44.0;
const PANEL_W: f32 = 260.0;

impl WorkspaceView {
    /// Render one side drawer: activity bar + the active panel. The activity bar
    /// sits on the window-edge side (far left for the left drawer, far right for
    /// the right drawer); a hairline faces the splits.
    pub(crate) fn drawer(
        &self,
        side: SidebarSide,
        panel: SidebarPanel,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let fg = colors::hsla(self.colors.fg);
        let panelbg = colors::rgba(crate::tabbar::blend(self.colors.bg, self.colors.fg, 0.04));
        let mut border = fg;
        border.a = 0.15;

        let body = match panel {
            SidebarPanel::Terminals => self.panel_terminals(cx),
            SidebarPanel::Layouts => self.panel_layouts(cx),
            SidebarPanel::Relay => self.panel_relay(cx),
            SidebarPanel::Agents => self.panel_agents(cx),
            SidebarPanel::Plugin(_) => self.panel_plugin(panel, cx),
        };
        let content = div()
            .w(px(PANEL_W))
            .h_full()
            .flex()
            .flex_col()
            .min_h(px(0.0))
            .bg(panelbg)
            .text_color(fg)
            .child(self.sidebar_header(&self.panel_label_of(panel)))
            .child(body);

        let activity = self.sidebar_activitybar(side, panel, cx);
        let mut row = div().flex().flex_row().h_full().flex_none();
        row = match side {
            SidebarSide::Left => row.border_r_1().border_color(border).child(activity).child(content),
            SidebarSide::Right => row.border_l_1().border_color(border).child(content).child(activity),
        };
        row.into_any_element()
    }

    /// The vertical icon strip. Each icon selects its panel; clicking the active
    /// one collapses the drawer.
    fn sidebar_activitybar(
        &self,
        side: SidebarSide,
        active: SidebarPanel,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let fg = colors::hsla(self.colors.fg);
        let mut dim = fg;
        dim.a = 0.55;
        let mut hover = fg;
        hover.a = 0.08;
        let barbg = colors::rgba(crate::tabbar::blend(self.colors.bg, self.colors.fg, 0.10));
        let accent = colors::rgba(self.colors.selection_bg);
        let prefix = match side {
            SidebarSide::Left => "left",
            SidebarSide::Right => "right",
        };

        let mut bar = div()
            .w(px(ACTIVITY_W))
            .h_full()
            .flex()
            .flex_col()
            .items_center()
            .py_1()
            .gap_1()
            .bg(barbg);
        for (i, panel) in self.panel_list().into_iter().enumerate() {
            let is_active = panel == active;
            let payload = format!("{prefix}:{}", self.panel_token_of(panel));
            bar = bar.child(
                div()
                    .id(("sb-icon", i))
                    .w(px(34.0))
                    .h(px(34.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.0))
                    .text_size(px(16.0))
                    .text_color(if is_active { fg } else { dim })
                    .when(is_active, |d| d.bg(accent))
                    .hover(|s| s.bg(hover))
                    .on_click(cx.listener(move |this, _: &gpui::ClickEvent, _w, cx| {
                        this.toggle_sidebar(&payload, cx);
                    }))
                    .child(SharedString::from(self.panel_icon_of(panel))),
            );
        }
        bar
    }

    /// A panel's title strip.
    fn sidebar_header(&self, label: &str) -> impl IntoElement {
        let fg = colors::hsla(self.colors.fg);
        let mut dim = fg;
        dim.a = 0.6;
        let mut border = fg;
        border.a = 0.12;
        div()
            .flex_none()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(border)
            .text_size(px(11.0))
            .text_color(dim)
            .child(SharedString::from(label.to_uppercase()))
    }

    /// Scrollable body wrapper shared by all panels.
    fn sidebar_body(&self, id: &'static str) -> gpui::Stateful<gpui::Div> {
        div()
            .id(id)
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .py_1()
            .text_size(px(13.0))
    }

    /// A clickable row; `indent` shifts panes under their tab.
    fn sidebar_row(
        &self,
        id: impl Into<gpui::ElementId>,
        label: String,
        indent: bool,
        active: bool,
        attention: bool,
    ) -> gpui::Stateful<gpui::Div> {
        let fg = colors::hsla(self.colors.fg);
        let mut hover = fg;
        hover.a = 0.08;
        let sel = colors::rgba(self.colors.selection_bg);
        let attn = colors::hsla(ATTENTION);
        let row = div()
            .id(id)
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .mx_1()
            .px_2()
            .py(px(4.0))
            .rounded(px(5.0))
            .when(indent, |d| d.pl_4())
            .when(active, |d| d.bg(sel))
            .hover(move |s| s.bg(hover));
        let dot = div()
            .flex_none()
            .w(px(6.0))
            .h(px(6.0))
            .rounded_full()
            .when(attention, |d| d.bg(attn));
        row.child(dot).child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(SharedString::from(label)),
        )
    }

    /// Terminals panel: tree of tabs and their panes.
    fn panel_terminals(&self, cx: &mut Context<Self>) -> AnyElement {
        let titles = self.titles(cx);
        let active_tab = self.tabs.active_index();
        let focused = self.tabs.focused();
        let mut body = self.sidebar_body("sb-terminals");
        let mut row = 0usize;
        for ti in 0..self.tabs.len() {
            let Some(tab) = self.tabs.get(ti) else { continue };
            let tab_title = titles.get(ti).cloned().unwrap_or_default();
            body = body.child(
                self.sidebar_row(("sb-tab", ti), tab_title, false, ti == active_tab, false)
                    .on_click(cx.listener(move |this, _: &gpui::ClickEvent, window, cx| {
                        this.activatetab(ti, window, cx);
                    })),
            );
            for pid in tab.tree.panes() {
                let view = self.panes.get(&pid).map(|p| p.view.read(cx));
                let title = view.as_ref().map(|v| v.title().to_string()).unwrap_or_default();
                let attention = view.as_ref().is_some_and(|v| v.needs_attention());
                body = body.child(
                    self.sidebar_row(("sb-pane", row), title, true, pid == focused, attention)
                        .on_click(cx.listener(move |this, _: &gpui::ClickEvent, window, cx| {
                            this.activatetab(ti, window, cx);
                            this.focuspane(pid, window, cx);
                        })),
                );
                row += 1;
            }
        }
        body.into_any_element()
    }

    /// Layouts panel: tile presets, saved custom layouts, and a save action.
    fn panel_layouts(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut body = self.sidebar_body("sb-layouts");
        for (i, (id, label, _, _)) in crate::tiles::presets().iter().enumerate() {
            let id = id.to_string();
            body = body.child(
                self.sidebar_row(("sb-preset", i), label.to_string(), false, false, false)
                    .on_click(cx.listener(move |this, _: &gpui::ClickEvent, window, cx| {
                        this.apply_tile(&id, window, cx);
                    })),
            );
        }
        for (i, name) in crate::tiles::list_custom().into_iter().enumerate() {
            let id = name.clone();
            body = body.child(
                self.sidebar_row(("sb-custom", i), name, false, false, false)
                    .on_click(cx.listener(move |this, _: &gpui::ClickEvent, window, cx| {
                        this.apply_tile(&id, window, cx);
                    })),
            );
        }
        body = body.child(
            self.sidebar_row(("sb-savelayout", 0usize), "Save Current Layout…".into(), false, false, false)
                .on_click(cx.listener(|this, _: &gpui::ClickEvent, window, cx| {
                    this.open_save_layout(window, cx);
                })),
        );
        body.into_any_element()
    }

    /// Relay panel: live server status, connected agents, and workers.
    fn panel_relay(&self, cx: &mut Context<Self>) -> AnyElement {
        let status = cx.try_global::<RelayStatus>().cloned().unwrap_or_default();
        let mut dim = colors::hsla(self.colors.fg);
        dim.a = 0.6;
        let mut body = self.sidebar_body("sb-relay");

        if !crate::relay::enabled(&self.opts) {
            return body
                .child(self.sidebar_note("Relay is disabled. Enable it in Settings."))
                .into_any_element();
        }
        let server = if status.connected {
            "\u{25cf} Server connected"
        } else {
            "\u{25cb} Server stopped"
        };
        body = body.child(
            div()
                .px_3()
                .py_1()
                .text_color(dim)
                .child(SharedString::from(server)),
        );

        body = body.child(self.sidebar_section("Agents"));
        if status.agents.is_empty() {
            body = body.child(self.sidebar_note("No agents connected."));
        } else {
            for (i, a) in status.agents.iter().enumerate() {
                let label = format!(
                    "{} {}  ·  {}  ch:{}",
                    if a.online { "\u{25cf}" } else { "\u{25cb}" },
                    a.name,
                    a.role,
                    a.channels
                );
                body = body.child(self.sidebar_row(("sb-agentconn", i), label, false, false, false));
            }
        }

        if !status.workers.is_empty() {
            body = body.child(self.sidebar_section("Workers"));
            for (i, w) in status.workers.iter().enumerate() {
                let restarts = if w.restarts > 0 {
                    format!("  ↻{}", w.restarts)
                } else {
                    String::new()
                };
                let role = if w.role.is_empty() { String::new() } else { format!("  ·  {}", w.role) };
                let label = format!("{}{}  ·  {}{}", w.name, role, w.status, restarts);
                body = body.child(self.sidebar_row(("sb-worker", i), label, false, false, false));
            }
        }
        body.into_any_element()
    }

    /// Agents panel: saved agent definitions; click to launch in a split.
    fn panel_agents(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut body = self.sidebar_body("sb-agents");
        let defs = crate::relay::list_agent_defs();
        if defs.is_empty() {
            return body
                .child(self.sidebar_note("No saved agents. Define one from the AI menu."))
                .into_any_element();
        }
        for (i, d) in defs.into_iter().enumerate() {
            let name = d.name.clone();
            let label = format!("{}  ·  {}", d.name, d.provider);
            body = body.child(
                self.sidebar_row(("sb-agentdef", i), label, false, false, false)
                    .on_click(cx.listener(move |this, _: &gpui::ClickEvent, window, cx| {
                        crate::relay::ensure_running(&this.opts);
                        if let Some(cmd) = crate::relay::launch_saved_command(&this.opts, &name) {
                            this.splitcommand(&cmd, Axis::Horizontal, false, window, cx);
                        }
                    })),
            );
        }
        body.into_any_element()
    }

    /// A small dimmed sub-header inside a panel (guise typography).
    fn sidebar_section(&self, label: &str) -> impl IntoElement {
        div()
            .px_3()
            .pt_2()
            .pb_1()
            .child(Text::new(label.to_uppercase()).size(Size::Xs).dimmed())
    }

    /// A dimmed empty-state line (guise typography).
    fn sidebar_note(&self, text: &str) -> impl IntoElement {
        div()
            .px_3()
            .py_2()
            .child(Text::new(text.to_string()).size(Size::Sm).dimmed())
    }
}
