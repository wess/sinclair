//! Side docks. A dock pairs a thin rail of section icons with a vertical stack
//! of collapsible sections — Zed's dock accordion rather than one panel at a
//! time. **The two sides hold different sections**: each dock's composition
//! comes from `sidebar-left` / `sidebar-right` (see `dock.rs`, which owns that
//! logic and is where its tests live), so the rail on the left lists only the
//! left dock's sections.
//!
//! This file is the rendering half: the rail, the section headers, and one
//! `panel_*` body renderer per section. Composition, reveal and reordering all
//! live in `dock.rs`.
//!
//! Both docks are hidden by default; toggled from the View menu (see
//! `menus::sidebar_menu`) and composed in Settings → Sidebar.

use super::*;
use gpui::prelude::*;
use guise::prelude::{Size, Text};

/// Attention accent, matching the tab strip's pending-notification dot.
const ATTENTION: theme::Rgb = theme::Rgb::new(255, 196, 0);

const ACTIVITY_W: f32 = 44.0;

/// Floor on a configured dock width, so a bad `sidebar-left-width` cannot
/// produce a column too narrow to read.
const MIN_PANEL_W: f32 = 180.0;

/// Vertical clearance reserved at the top of a drawer so the platform's window
/// controls don't cover the activity-bar icons or the panel header. Matches the
/// group's tab-bar height (see `build_group`) so the two line up.
const TITLE_CLEARANCE: f32 = 34.0;

/// The top inset for a drawer on `side`: non-zero only on the side the
/// platform's window controls occupy — the macOS traffic lights sit top-left,
/// the client-side controls elsewhere sit top-right — mirroring the group
/// titlebar's leading/trailing reservation in `build_group`.
fn drawer_top_inset(side: SidebarSide) -> f32 {
    let on_controls_side = if cfg!(target_os = "macos") {
        matches!(side, SidebarSide::Left)
    } else {
        matches!(side, SidebarSide::Right)
    };
    if on_controls_side {
        TITLE_CLEARANCE
    } else {
        0.0
    }
}

impl WorkspaceView {
    /// Render one side dock: the rail plus the section stack. The rail sits on
    /// the window-edge side (far left for the left dock, far right for the
    /// right); a hairline faces the splits.
    pub(crate) fn dock_column(&self, side: SidebarSide, cx: &mut Context<Self>) -> AnyElement {
        let fg = colors::hsla(self.colors.fg);
        let panelbg = colors::rgba(crate::tabbar::blend(self.colors.bg, self.colors.fg, 0.04));
        let mut border = fg;
        border.a = 0.15;
        let dock = &self.docks[side.index()];

        // Reserve top clearance on the window-controls side so the first
        // section header isn't drawn under the traffic lights / caption
        // buttons; the background still fills to the very top behind them.
        let mut content = div()
            .w(px(dock.width.max(MIN_PANEL_W)))
            .h_full()
            .flex()
            .flex_col()
            .min_h(px(0.0))
            .pt(px(drawer_top_inset(side)))
            .bg(panelbg)
            .text_color(fg);

        if dock.sections.is_empty() {
            content = content.child(self.dock_titlebar(side, cx)).child(
                self.sidebar_note("No sections here. Add some in Settings \u{2192} Sidebar."),
            );
        } else {
            content = content.child(self.dock_titlebar(side, cx));
            for (i, section) in dock.sections.iter().enumerate() {
                content = content.child(self.dock_section(side, i, *section, cx));
            }
        }

        let rail = self.sidebar_rail(side, cx);
        let mut row = div().flex().flex_row().h_full().flex_none();
        row = match side {
            SidebarSide::Left => row
                .border_r_1()
                .border_color(border)
                .child(rail)
                .child(content),
            SidebarSide::Right => row
                .border_l_1()
                .border_color(border)
                .child(content)
                .child(rail),
        };
        row.into_any_element()
    }

    /// The dock's own thin strip: just the `×` that closes the whole dock.
    /// Naming the side is redundant — which column you're looking at is not
    /// something you need told — so the strip carries only the control.
    fn dock_titlebar(&self, side: SidebarSide, cx: &mut Context<Self>) -> impl IntoElement {
        let fg = colors::hsla(self.colors.fg);
        let mut dim = fg;
        dim.a = 0.45;
        let mut border = fg;
        border.a = 0.12;
        let mut hover = fg;
        hover.a = 0.12;
        // A bare side token tells `toggle_sidebar` to close the dock.
        let close_payload = side.token();
        div()
            .flex_none()
            .flex()
            .flex_row()
            .items_center()
            .justify_end()
            .px_3()
            .py_1()
            .border_b_1()
            .border_color(border)
            .text_size(px(10.0))
            .text_color(dim)
            .child(
                div()
                    .id(("sb-close", side.index()))
                    .flex_none()
                    .w(px(18.0))
                    .h(px(18.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(4.0))
                    .text_size(px(14.0))
                    .hover(|s| s.bg(hover).text_color(fg))
                    .on_click(cx.listener(move |this, _: &gpui::ClickEvent, _w, cx| {
                        this.toggle_sidebar(close_payload, cx);
                    }))
                    .child(SharedString::from("\u{00d7}")),
            )
    }

    /// One section: a flush header (chevron + label) over its body when
    /// expanded. Expanded sections are `flex_1` so they share whatever height
    /// is left over; collapsed ones are just their header.
    fn dock_section(
        &self,
        side: SidebarSide,
        index: usize,
        section: DockSection,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let panel = section.panel;
        let fg = colors::hsla(self.colors.fg);
        let mut dim = fg;
        dim.a = 0.6;
        let mut border = fg;
        border.a = 0.10;
        let mut hover = fg;
        hover.a = 0.08;

        let chevron: &'static str = if section.expanded {
            "\u{25be}" // ▾
        } else {
            "\u{25b8}" // ▸
        };
        let header = div()
            .id(("sb-sec", side.index() * 64 + index))
            .flex_none()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .px_2()
            .py(px(5.0))
            .border_b_1()
            .border_color(border)
            .text_size(px(10.0))
            .text_color(dim)
            .hover(move |s| s.bg(hover).text_color(fg))
            .on_click(cx.listener(move |this, _: &gpui::ClickEvent, _w, cx| {
                this.toggle_section(side, index, cx);
            }))
            .child(div().w(px(12.0)).child(SharedString::new_static(chevron)))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(self.section_title(panel)),
            );

        let mut wrapper = div().flex().flex_col().min_h(px(0.0)).child(header);
        if section.expanded {
            // flex_1 + min_h(0) is what lets several expanded sections share the
            // column and each scroll on its own instead of one pushing the rest
            // off the bottom.
            wrapper = wrapper.flex_1().child(self.panel_body(panel, cx));
        } else {
            wrapper = wrapper.flex_none();
        }
        wrapper.into_any_element()
    }

    /// Dispatch to the renderer for one section's contents.
    fn panel_body(&self, panel: SidebarPanel, cx: &mut Context<Self>) -> AnyElement {
        match panel {
            SidebarPanel::Terminals => self.panel_terminals(cx),
            SidebarPanel::Layouts => self.panel_layouts(cx),
            SidebarPanel::Containers => self.panel_containers(cx),
            SidebarPanel::Activity => self.panel_activity(cx),
            SidebarPanel::Relay => self.panel_relay(cx),
            SidebarPanel::Agents => self.panel_agents(cx),
            SidebarPanel::Plugins => self.panel_pluginlist(cx),
            SidebarPanel::Worktrees => self.panel_worktrees(cx),
            SidebarPanel::Notes => self.panel_notes(cx),
            SidebarPanel::Plugin(_) => self.panel_plugin(panel, cx),
        }
    }

    /// The vertical icon strip — **only this side's sections**, which is what
    /// makes the left and right docks genuinely different columns. Clicking an
    /// icon expands that section (or collapses it if it was already showing).
    fn sidebar_rail(&self, side: SidebarSide, cx: &mut Context<Self>) -> impl IntoElement {
        let fg = colors::hsla(self.colors.fg);
        let mut dim = fg;
        dim.a = 0.55;
        let mut hover = fg;
        hover.a = 0.08;
        let barbg = colors::rgba(crate::tabbar::blend(self.colors.bg, self.colors.fg, 0.10));
        let accent = colors::rgba(self.colors.selection_bg);

        // Drop the first icon below the window controls on the controls side
        // (the bar's own background still fills up behind them); elsewhere keep
        // the normal 4px top pad.
        let top = drawer_top_inset(side);
        let mut bar = div()
            .w(px(ACTIVITY_W))
            .h_full()
            .flex()
            .flex_col()
            .items_center()
            .pt(px(if top > 0.0 { top } else { 4.0 }))
            .pb_1()
            .gap_1()
            .bg(barbg);
        for (i, section) in self.docks[side.index()].sections.iter().enumerate() {
            let panel = section.panel;
            let is_active = section.expanded;
            bar = bar.child(
                div()
                    .id(("sb-icon", side.index() * 64 + i))
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
                        this.toggle_section(side, i, cx);
                    }))
                    .child(self.panel_glyph(panel)),
            );
        }
        bar
    }

    /// A section header's title. Free for a built-in — the uppercased form is a
    /// constant — and one allocation for a plugin, whose title comes from its
    /// manifest or its latest live response.
    fn section_title(&self, panel: SidebarPanel) -> SharedString {
        match panel {
            SidebarPanel::Plugin(_) => {
                SharedString::from(self.panel_label_of(panel).to_uppercase())
            }
            builtin => SharedString::new_static(builtin.label_upper()),
        }
    }

    /// A rail glyph. Same split: static for a built-in, owned for a plugin.
    fn panel_glyph(&self, panel: SidebarPanel) -> SharedString {
        match panel {
            SidebarPanel::Plugin(_) => SharedString::from(self.panel_icon_of(panel)),
            builtin => SharedString::new_static(builtin.icon()),
        }
    }

    /// Scrollable body wrapper shared by all sections. `flex_1` here would
    /// fight the section wrapper for height, so the wrapper owns the sizing and
    /// this only owns the scrolling.
    fn sidebar_body(&self, id: &'static str) -> gpui::Stateful<gpui::Div> {
        div()
            .id(id)
            .w_full()
            // `flex_1` + `min_h(0)`, not `h_full`: the body is the *second*
            // child of the section wrapper, after the header, so filling the
            // wrapper's full height would overflow it by exactly the header.
            // Flexing takes the leftover instead, and min_h(0) is what lets it
            // shrink below its content so the scroll actually engages.
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

    /// Terminals panel: each split pane with its tabbed items nested under it.
    fn panel_terminals(&self, cx: &mut Context<Self>) -> AnyElement {
        let (panes, focused_pane, active_item) = {
            let g = self.group.read(cx);
            (g.tree().panes(), g.focused_pane(), g.active_item())
        };
        let mut body = self.sidebar_body("sb-terminals");
        let mut row = 0usize;
        for (pi, &pane) in panes.iter().enumerate() {
            body = body.child(self.sidebar_row(
                ("sb-pane-hdr", pi),
                format!("Pane {}", pi + 1),
                false,
                pane == focused_pane,
                false,
            ));
            let items = self
                .group
                .read(cx)
                .pane_items(pane)
                .map(<[_]>::to_vec)
                .unwrap_or_default();
            for item in items {
                let (title, attention) = {
                    let map = self.items.borrow();
                    match map.get(&item) {
                        Some(it) => (it.content.title(cx), it.content.needs_attention(cx)),
                        None => (String::new(), false),
                    }
                };
                let active = pane == focused_pane && item == active_item;
                body = body.child(
                    self.sidebar_row(("sb-item", row), title, true, active, attention)
                        .on_click(cx.listener(move |this, _: &gpui::ClickEvent, window, cx| {
                            this.activate_item(item, window, cx);
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
        for (i, name) in self.menu_custom_tiles.clone().into_iter().enumerate() {
            let id = name.clone();
            body = body.child(
                self.sidebar_row(("sb-custom", i), name, false, false, false)
                    .on_click(cx.listener(move |this, _: &gpui::ClickEvent, window, cx| {
                        this.apply_tile(&id, window, cx);
                    })),
            );
        }
        body = body.child(
            self.sidebar_row(
                ("sb-savelayout", 0usize),
                "Save Current Layout…".into(),
                false,
                false,
                false,
            )
            .on_click(cx.listener(|this, _: &gpui::ClickEvent, window, cx| {
                this.open_save_layout(window, cx);
            })),
        );
        body.into_any_element()
    }

    /// Containers panel: the running Docker/Podman containers. Click a row to
    /// attach a tab (focusing an existing one if already open); the header row
    /// spins up a fresh OS tab via the picker, and a refresh row re-lists.
    fn panel_containers(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut body = self.sidebar_body("sb-containers");
        // Resolved when the panel opened / was refreshed; resolving stats
        // `$PATH`, so it never happens here in render.
        let engine = self.engine.flatten();

        let Some(engine) = engine else {
            return body
                .child(self.sidebar_note(
                    "No container engine found. Install Docker or Podman to manage containers.",
                ))
                .into_any_element();
        };

        body = body.child(self.sidebar_section("Project sandbox"));
        body = body.child(self.sidebar_note(&self.sandbox_summary()));
        if self.opts.sandbox_enabled {
            let label = if self.sandbox.is_some() {
                "\u{25b8} Open shell in sandbox"
            } else {
                "\u{25b8} Start sandbox"
            };
            body = body.child(
                self.sidebar_row(("sb-sbx-open", 0usize), label.into(), false, false, false)
                    .on_click(cx.listener(|this, _: &gpui::ClickEvent, window, cx| {
                        this.open_sandbox_shell(window, cx);
                    })),
            );
            if self.sandbox.is_some() {
                body = body.child(
                    self.sidebar_row(
                        ("sb-sbx-stop", 0usize),
                        "\u{25a0} Stop sandbox".into(),
                        false,
                        false,
                        false,
                    )
                    .on_click(cx.listener(
                        |this, _: &gpui::ClickEvent, _w, cx| {
                            this.sandbox_stop(cx);
                        },
                    )),
                );
            }
            // Advisories from the last resolve: a devcontainer that stops when
            // the editor closes, a mount that did not parse. Worth seeing
            // before a team starts working in here.
            for note in &self.sandbox_notes {
                body = body.child(self.sidebar_note(note));
            }
        } else {
            body = body.child(
                self.sidebar_row(
                    ("sb-sbx-on", 0usize),
                    "\u{25b8} Use a sandbox here".into(),
                    false,
                    false,
                    false,
                )
                .on_click(cx.listener(|this, _: &gpui::ClickEvent, window, cx| {
                    this.toggle_sandbox(window, cx);
                })),
            );
        }

        body = body.child(self.sidebar_section(&format!("Running \u{00b7} {}", engine.label())));
        if self.containers.is_empty() {
            body = body.child(self.sidebar_note("No running containers."));
        } else {
            body = body.child(self.sidebar_note("Double-click to attach a shell."));
            for (i, c) in self.containers.iter().enumerate() {
                let running = c.clone();
                let active = self
                    .container_tabs
                    .get(&c.id)
                    .is_some_and(|iid| self.items.borrow().contains_key(iid));
                let name = if c.name.is_empty() {
                    c.id.clone()
                } else {
                    c.name.clone()
                };
                let label = format!("{name}  \u{00b7}  {}", c.image);
                body = body.child(
                    self.sidebar_row(("sb-ctr", i), label, false, active, false)
                        // Double-click attaches (opens/focuses the container's tab).
                        .on_click(cx.listener(move |this, ev: &gpui::ClickEvent, window, cx| {
                            if ev.click_count() == 2 {
                                this.attach_container(&running, window, cx);
                            }
                        })),
                );
            }
        }

        body = body.child(
            self.sidebar_row(
                ("sb-ctr-refresh", 0usize),
                "\u{21bb} Refresh".into(),
                false,
                false,
                false,
            )
            .on_click(cx.listener(|this, _: &gpui::ClickEvent, _w, cx| {
                this.refresh_containers();
                cx.notify();
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

        if !crate::relay::available(&self.opts) {
            return body
                .child(self.sidebar_note("Enable AI in Settings to use Relay."))
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
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            for (i, a) in status.agents.iter().enumerate() {
                // Truthful liveness: a filled dot only while the agent is parked
                // or recently active; otherwise hollow with how long it's been
                // quiet, so a crashed agent no longer reads as alive (issue #9).
                let state = if a.online {
                    // Prefer the agent's self-reported work state over bare liveness.
                    if a.status.is_empty() {
                        String::new()
                    } else {
                        format!("  ·  {}", a.status)
                    }
                } else if !a.registered {
                    "  ·  pending".to_string()
                } else if a.last_seen > 0 {
                    format!("  ·  quiet {}", rel_since(now, a.last_seen))
                } else {
                    "  ·  offline".to_string()
                };
                // A semantic dot for a known work state, else bare liveness.
                let dot = match crate::agentstate::AgentState::parse(&a.status) {
                    Some(st) if a.online => st.glyph(),
                    _ if a.online => "\u{25cf}", // ●
                    _ => "\u{25cb}",             // ○
                };
                let label = format!(
                    "{} {}  ·  {}  ch:{}{}",
                    dot, a.name, a.role, a.channels, state
                );
                body =
                    body.child(self.sidebar_row(("sb-agentconn", i), label, false, false, false));
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
                let role = if w.role.is_empty() {
                    String::new()
                } else {
                    format!("  ·  {}", w.role)
                };
                let label = format!("{}{}  ·  {}{}", w.name, role, w.status, restarts);
                body = body.child(self.sidebar_row(("sb-worker", i), label, false, false, false));
            }
        }
        body.into_any_element()
    }

    /// Activity panel: every tab rolled up to a status dot — who's blocked,
    /// working, done, or idle at a glance across the whole session. An agent's
    /// self-reported state (🔴 blocked, 🟡 working, 🔵 done, 🟢 idle) wins when
    /// present; otherwise the dot falls back to the terminal's own signals — 🔴
    /// attention (a bell / OSC 9 fired), 🟡 a foreground command is running, 🟢
    /// idle. Click a row to focus that tab.
    fn panel_activity(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut body = self.sidebar_body("sb-activity");
        let active_item = self.group.read(cx).active_item();
        for (i, item) in self.group.read(cx).items().into_iter().enumerate() {
            let (agent, working, attention, title) = {
                let map = self.items.borrow();
                match map.get(&item) {
                    Some(it) => (
                        it.agent,
                        it.content.has_running_process(cx),
                        it.content.needs_attention(cx),
                        it.content.title(cx),
                    ),
                    None => (None, false, false, String::new()),
                }
            };
            let dot = match agent {
                Some(state) => state.glyph(),
                None if attention => "\u{1f534}", // 🔴
                None if working => "\u{1f7e1}",   // 🟡
                None => "\u{1f7e2}",              // 🟢
            };
            let name = if title.trim().is_empty() {
                format!("Terminal {}", i + 1)
            } else {
                title
            };
            let label = match agent {
                Some(state) => format!("{dot}  {name}  \u{00b7}  {}", state.label()),
                None => format!("{dot}  {name}"),
            };
            body = body.child(
                self.sidebar_row(("sb-activity", i), label, item == active_item, false, false)
                    .on_click(cx.listener(move |this, _: &gpui::ClickEvent, window, cx| {
                        this.activate_item(item, window, cx);
                    })),
            );
        }
        body.into_any_element()
    }

    /// Agents panel: saved agent definitions (from the off-thread menu-data
    /// cache — never a disk read per repaint); click to launch in a split.
    fn panel_agents(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut body = self.sidebar_body("sb-agents");
        let defs = self.menu_agent_defs.clone();
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
                        let name = name.clone();
                        this.with_relay_running(window, cx, move |this, window, cx| {
                            let sandbox = this.sandbox_ref();
                            if let Some(cmd) = crate::relay::launch_saved_command(
                                &this.opts,
                                &name,
                                sandbox.as_ref(),
                            ) {
                                this.splitcommand(&cmd, SplitAxis::Horizontal, false, window, cx);
                            }
                        });
                    })),
            );
        }
        body.into_any_element()
    }

    /// Plugins panel: installed plugins (their panels + commands) on top, the
    /// installable catalog below.
    fn panel_pluginlist(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut body = self.sidebar_body("sb-plugins");
        let mut row_id = 0usize;

        body = body.child(self.sidebar_section("Installed"));
        if self.plugins.is_empty() {
            body = body.child(self.sidebar_note("No plugins installed."));
        }
        for (pi, plugin) in self.plugins.iter().enumerate() {
            // Clicking the plugin name opens its primary surface: a panel, else
            // its first command.
            let primary = plugin
                .panel
                .as_ref()
                .map(|p| config::Action::Sidebar(format!("right:plugin:{}", p.id)))
                .or_else(|| {
                    plugin
                        .commands
                        .first()
                        .map(|c| config::Action::PluginCommand(plugin::actionid(&plugin.id, &c.id)))
                });
            let name_row =
                self.sidebar_row(("sb-pl-name", pi), plugin.name.clone(), false, false, false);
            body = body.child(match primary {
                Some(action) => {
                    name_row.on_click(cx.listener(move |this, _: &gpui::ClickEvent, window, cx| {
                        this.run_action(action.clone(), window, cx);
                    }))
                }
                None => name_row,
            });
            if let Some(panel) = plugin.panel.as_ref() {
                let token = format!("right:plugin:{}", panel.id);
                body = body.child(
                    self.sidebar_row(
                        ("sb-pl-open", row_id),
                        "\u{25a4} Open panel".to_string(),
                        true,
                        false,
                        false,
                    )
                    .on_click(cx.listener(
                        move |this, _: &gpui::ClickEvent, _w, cx| {
                            this.toggle_sidebar(&token, cx);
                        },
                    )),
                );
                row_id += 1;
            }
            for cmd in &plugin.commands {
                let action = config::Action::PluginCommand(plugin::actionid(&plugin.id, &cmd.id));
                body = body.child(
                    self.sidebar_row(
                        ("sb-pl-cmd", row_id),
                        format!("\u{25b8} {}", cmd.title),
                        true,
                        false,
                        false,
                    )
                    .on_click(cx.listener(
                        move |this, _: &gpui::ClickEvent, window, cx| {
                            this.run_action(action.clone(), window, cx);
                        },
                    )),
                );
                row_id += 1;
            }
        }

        body = body.child(self.sidebar_section("Available"));
        if self.catalog_loading && self.catalog.is_none() {
            body = body.child(self.sidebar_note("Loading catalog\u{2026}"));
        } else if self.catalog.is_none() {
            body = body.child(self.sidebar_note("Open to load the catalog."));
        } else {
            let available = self.available_plugins();
            if available.is_empty() {
                body = body.child(self.sidebar_note("Everything is installed."));
            }
            for name in available {
                let n = name.clone();
                body = body.child(
                    self.sidebar_row(
                        ("sb-pl-avail", row_id),
                        format!("\u{2913} {name}"),
                        false,
                        false,
                        false,
                    )
                    .on_click(cx.listener(
                        move |this, _: &gpui::ClickEvent, _w, cx| {
                            this.install_catalog_plugin(&n, cx);
                        },
                    )),
                );
                row_id += 1;
            }
            body = body.child(
                self.sidebar_row(
                    ("sb-pl-refresh", row_id),
                    "\u{21bb} Refresh catalog".to_string(),
                    false,
                    false,
                    false,
                )
                .on_click(cx.listener(|this, _: &gpui::ClickEvent, _w, cx| this.fetch_catalog(cx))),
            );
        }
        if let Some(status) = self.catalog_status.as_ref() {
            body = body.child(self.sidebar_note(status));
        }
        body.into_any_element()
    }

    /// Worktrees panel: this repository's git worktrees. Click one to open it
    /// in a tab, which is the same path `worktree_open` takes.
    ///
    /// The list is whatever `refresh_worktrees` last cached — `git worktree
    /// list` is a subprocess, and running one per repaint would be a subprocess
    /// per frame.
    fn panel_worktrees(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut body = self.sidebar_body("sb-worktrees");
        let current = self.focused_cwd_path(cx);

        match self.worktrees.as_ref() {
            None => {
                return body
                    .child(self.sidebar_note("Looking for a repository\u{2026}"))
                    .into_any_element()
            }
            Some(Err(message)) => return body.child(self.sidebar_note(message)).into_any_element(),
            Some(Ok(list)) if list.is_empty() => {
                body = body.child(self.sidebar_note("No worktrees in this repository."));
            }
            Some(Ok(list)) => {
                for (i, tree) in list.iter().enumerate() {
                    let name = tree
                        .path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| tree.path.to_string_lossy().into_owned());
                    let label = match tree.branch.as_deref() {
                        Some(branch) => format!("{name}  \u{00b7}  {branch}"),
                        None => format!("{name}  \u{00b7}  detached"),
                    };
                    // Mark the worktree the focused tab is actually sitting in.
                    let active = current.as_deref() == Some(tree.path.as_path());
                    let path = tree.path.to_string_lossy().into_owned();
                    body = body.child(
                        self.sidebar_row(("sb-wt", i), label, false, active, false)
                            .on_click(cx.listener(
                                move |this, _: &gpui::ClickEvent, window, cx| {
                                    if let Err(message) = this.worktree_open(&path, window, cx) {
                                        eprintln!("sinclair: worktree: {message}");
                                    }
                                },
                            )),
                    );
                }
            }
        }

        body = body.child(
            self.sidebar_row(
                ("sb-wt-new", 0usize),
                "\u{002b} New worktree\u{2026}".into(),
                false,
                false,
                false,
            )
            .on_click(cx.listener(|this, _: &gpui::ClickEvent, window, cx| {
                this.open_new_worktree(window, cx);
            })),
        );
        body = body.child(
            self.sidebar_row(
                ("sb-wt-refresh", 0usize),
                "\u{21bb} Refresh".into(),
                false,
                false,
                false,
            )
            .on_click(cx.listener(|this, _: &gpui::ClickEvent, _w, cx| {
                this.refresh_worktrees(cx);
                cx.notify();
            })),
        );
        body.into_any_element()
    }

    /// Notes panel: the vaults the Notes editor has opened before.
    ///
    /// Vaults rather than individual notes on purpose — `notes::Vault::recents`
    /// tracks vault directories, and the editor itself opens in a tab because a
    /// markdown editor is unusable in a column this narrow. This is the index
    /// that gets you into it, not a second editor.
    fn panel_notes(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut body = self.sidebar_body("sb-notes");

        body = body.child(self.sidebar_section("Vaults"));
        if self.notes_recent.is_empty() {
            body = body.child(self.sidebar_note("No vaults opened yet."));
        }
        for (i, recent) in self.notes_recent.iter().enumerate() {
            let label = recent.name.clone();
            body = body.child(
                self.sidebar_row(("sb-note", i), label, false, false, false)
                    .on_click(cx.listener(move |this, _: &gpui::ClickEvent, window, cx| {
                        this.open_notes(window, cx);
                    })),
            );
        }

        body = body.child(
            self.sidebar_row(
                ("sb-notes-open", 0usize),
                "\u{25a4} Open Notes".into(),
                false,
                false,
                false,
            )
            .on_click(cx.listener(|this, _: &gpui::ClickEvent, window, cx| {
                this.open_notes(window, cx);
            })),
        );
        body.into_any_element()
    }

    /// Re-read the vault list the Notes section renders from. Reads a small
    /// JSON file, so it runs when the section is revealed rather than in render.
    pub(crate) fn refresh_notes(&mut self) {
        self.notes_recent = crate::notes::vaults();
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

/// Compact "5s"/"2m"/"1h"/"3d" since `then` (both epoch seconds).
fn rel_since(now: i64, then: i64) -> String {
    let s = (now - then).max(0);
    if s < 60 {
        format!("{s}s")
    } else if s < 3600 {
        format!("{}m", s / 60)
    } else if s < 86400 {
        format!("{}h", s / 3600)
    } else {
        format!("{}d", s / 86400)
    }
}
