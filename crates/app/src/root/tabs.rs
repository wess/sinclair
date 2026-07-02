use super::*;
use gpui::prelude::*;

impl WorkspaceView {
    pub fn activatetab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.tab_overflow = false;
        if self.tabs.activate(index) {
            self.zoomed = false;
            self.focusactive(window, cx);
            cx.notify();
        }
    }

    /// Toggle the tab-overflow `…` dropdown.
    pub fn toggle_tab_overflow(&mut self, cx: &mut Context<Self>) {
        self.tab_overflow = !self.tab_overflow;
        cx.notify();
    }

    /// Apply an [`Action::Sidebar`] payload: show/hide a drawer side or switch
    /// its panel. Re-selecting the active panel (or toggling a side that is
    /// already open) collapses that side.
    pub fn toggle_sidebar(&mut self, payload: &str, cx: &mut Context<Self>) {
        // Parse `side[:token]`; resolve the token (built-in or `plugin:<id>`)
        // against the live plugin set, since plugin panels aren't statically known.
        let (side_str, token) = match payload.split_once(':') {
            Some((s, t)) => (s.trim(), Some(t.trim())),
            None => (payload.trim(), None),
        };
        let side = match side_str {
            "left" => SidebarSide::Left,
            "right" => SidebarSide::Right,
            _ => return,
        };
        let panel = match token {
            None => None,
            Some(t) => match self.panel_from_token(t) {
                Some(p) => Some(p),
                None => return,
            },
        };
        let slot = match side {
            SidebarSide::Left => &mut self.left_panel,
            SidebarSide::Right => &mut self.right_panel,
        };
        let next = match (panel, *slot) {
            // Side-only toggle: open with default panel, or collapse if open.
            (None, Some(_)) => None,
            (None, None) => Some(SidebarPanel::Terminals),
            // Re-selecting the active panel collapses; otherwise switch.
            (Some(p), Some(cur)) if cur == p => None,
            (Some(p), _) => Some(p),
        };
        *slot = next;
        // Render a freshly-opened plugin panel.
        if let Some(SidebarPanel::Plugin(i)) = next {
            self.refresh_plugin_panel(i, cx);
        }
        // Lazily fetch the installable catalog the first time Plugins opens.
        if next == Some(SidebarPanel::Plugins) && self.catalog.is_none() {
            self.fetch_catalog(cx);
        }
        // Refresh the running-container list whenever the Containers panel opens.
        if next == Some(SidebarPanel::Containers) {
            self.refresh_containers();
        }
        // A panel `[webview]` hosts a native surface that only tracks its bounds
        // while painted; hide any whose drawer just closed (or switched away).
        self.reconcile_webview_visibility(cx);
        self.setmenus(cx);
        cx.notify();
    }

    /// Reconcile every native `[webview]` surface with what's actually on screen:
    /// a panel host is visible only while its drawer is the active panel; a
    /// webview *pane* only while it's in the active tab. A native OS view tracks
    /// its bounds only while painted, so one that stops rendering must be hidden
    /// explicitly or it lingers. See [`PluginWebView::set_visible`].
    pub(crate) fn reconcile_webview_visibility(&self, cx: &mut Context<Self>) {
        // Panel-placement webviews, keyed by id in `webview_hosts`.
        let panel_visible = self.active_webview_ids();
        let panel_hosts: Vec<(String, gpui::Entity<crate::pluginwebview::PluginWebView>)> =
            self.webview_hosts.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        for (id, host) in panel_hosts {
            let vis = panel_visible.contains(&id);
            host.update(cx, |h, cx| h.set_visible(vis, cx));
        }

        // Webview panes: visible only if in the active tab's pane tree.
        let active: std::collections::HashSet<workspace::PaneId> =
            self.tabs.active().tree.panes().into_iter().collect();
        let pane_hosts: Vec<(workspace::PaneId, gpui::Entity<crate::pluginwebview::PluginWebView>)> =
            self.panes
                .iter()
                .filter_map(|(id, p)| match &p.content {
                    PaneContent::Webview(host) => Some((*id, host.clone())),
                    PaneContent::Terminal(_) => None,
                })
                .collect();
        for (id, host) in pane_hosts {
            let vis = active.contains(&id);
            host.update(cx, |h, cx| h.set_visible(vis, cx));
        }
    }

    /// The webview ids currently shown as a panel on either side.
    fn active_webview_ids(&self) -> std::collections::HashSet<String> {
        let defs = self.plugin_webview_panel_defs();
        let id_of = |panel: &Option<SidebarPanel>| match panel {
            Some(SidebarPanel::Webview(i)) => defs
                .get(*i)
                .and_then(|p| p.webview.as_ref())
                .map(|w| w.id.clone()),
            _ => None,
        };
        let mut set = std::collections::HashSet::new();
        if let Some(id) = id_of(&self.left_panel) {
            set.insert(id);
        }
        if let Some(id) = id_of(&self.right_panel) {
            set.insert(id);
        }
        set
    }

    /// Toggle one of the trailing tab-bar dropdowns (`+` / split). Clicking the
    /// open menu's own button closes it; clicking the other switches to it.
    pub fn toggle_trailing_menu(&mut self, which: TabBarMenu, cx: &mut Context<Self>) {
        self.tab_overflow = false;
        self.trailing_menu = (self.trailing_menu != Some(which)).then_some(which);
        cx.notify();
    }

    /// The dropdown for a trailing tab-bar button. The `+` menu lists ways to
    /// open something new; the split menu lists the four split directions. Each
    /// menu is anchored under its button and dismisses on outside click.
    pub(crate) fn trailing_menu(
        &self,
        which: TabBarMenu,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let fg = colors::rgba(self.colors.fg);
        let bg = colors::rgba(self.colors.bg);
        let sel = colors::rgba(self.colors.selection_bg);

        // Each entry: label plus the action to run when picked.
        type Run = Box<dyn Fn(&mut WorkspaceView, &mut Window, &mut Context<WorkspaceView>)>;
        let items: Vec<(&'static str, Run)> = match which {
            TabBarMenu::New => vec![
                ("New Tab", Box::new(|this, window, cx| this.newtab(window, cx))),
                ("New Window", Box::new(|this, _window, cx| this.newwindow(cx))),
            ],
            TabBarMenu::Split => {
                let split = |dir: SplitDirection| -> Run {
                    Box::new(move |this, window, cx| {
                        let (axis, first) = match dir {
                            SplitDirection::Right => (Axis::Horizontal, false),
                            SplitDirection::Left => (Axis::Horizontal, true),
                            SplitDirection::Down => (Axis::Vertical, false),
                            SplitDirection::Up => (Axis::Vertical, true),
                        };
                        this.split(axis, first, window, cx);
                    })
                };
                vec![
                    ("Split Right", split(SplitDirection::Right)),
                    ("Split Left", split(SplitDirection::Left)),
                    ("Split Down", split(SplitDirection::Down)),
                    ("Split Up", split(SplitDirection::Up)),
                ]
            }
        };

        let menu_w = 180.0;
        let width = f32::from(window.viewport_size().width);
        // Right edge of the trailing button group; the split button is last.
        let right_pad = match which {
            TabBarMenu::New => 34.0 + 8.0,
            TabBarMenu::Split => 8.0,
        };
        let pos = point(
            px((width - menu_w - right_pad).max(8.0)),
            crate::titlebar::height(window),
        );

        let mut menu = div()
            .id("trailing-menu")
            .flex()
            .flex_col()
            .min_w(px(menu_w))
            .p_1()
            .rounded(px(8.0))
            .border_1()
            .border_color(sel)
            .bg(bg)
            .text_color(fg)
            .shadow_lg();
        for (label, run) in items {
            menu = menu.child(
                div()
                    .id(label)
                    .flex()
                    .items_center()
                    .px_2()
                    .py(px(4.0))
                    .rounded(px(5.0))
                    .hover(|s| s.bg(sel))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _e: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            this.trailing_menu = None;
                            run(this, window, cx);
                        }),
                    )
                    .child(SharedString::from(label)),
            );
        }

        let dismiss = |this: &mut Self, _e: &MouseDownEvent, _w: &mut Window, cx: &mut Context<Self>| {
            this.trailing_menu = None;
            cx.stop_propagation();
            cx.notify();
        };
        deferred(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .on_mouse_down(MouseButton::Left, cx.listener(dismiss))
                .on_mouse_down(MouseButton::Right, cx.listener(dismiss))
                .child(
                    anchored()
                        .position(pos)
                        .snap_to_window_with_margin(px(6.0))
                        .child(menu),
                ),
        )
        .into_any_element()
    }

    /// How many tab slots fit inline before overflowing, from the window width.
    /// Mirrors the titlebar's leading inset so the count matches what renders.
    pub(crate) fn tab_max_visible(&self, window: &Window) -> usize {
        let width = f32::from(window.viewport_size().width);
        let lead = if cfg!(target_os = "macos") && !window.is_fullscreen() {
            crate::titlebar::TRAFFIC_LIGHT_INSET
        } else {
            8.0
        };
        let reserve = lead + 34.0 + 34.0 + 80.0;
        crate::tabbar::fit_count(width - reserve)
    }

    /// The dropdown listing tabs that didn't fit inline. Anchored under the top
    /// edge near the trailing side of the tab strip; click selects, × closes.
    pub(crate) fn tab_overflow_menu(
        &self,
        tabs: &[crate::tabbar::TabInfo],
        overflow: &[usize],
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let fg = colors::rgba(self.colors.fg);
        let bg = colors::rgba(self.colors.bg);
        let sel = colors::rgba(self.colors.selection_bg);
        let mut dim = fg;
        dim.a = 0.6;

        let menu_w = 260.0;
        let width = f32::from(window.viewport_size().width);
        let pos = point(px((width - menu_w - 8.0).max(8.0)), crate::titlebar::height(window));

        let mut menu = div()
            .id("taboverflow-menu")
            .flex()
            .flex_col()
            .min_w(px(menu_w))
            .max_h(px(420.0))
            .overflow_y_scroll()
            .p_1()
            .rounded(px(8.0))
            .border_1()
            .border_color(sel)
            .bg(bg)
            .text_color(fg)
            .shadow_lg();
        for &index in overflow {
            let title = tabs[index].title.clone();
            menu = menu.child(
                div()
                    .id(("overflowtab", index))
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .py(px(4.0))
                    .rounded(px(5.0))
                    .hover(|s| s.bg(sel))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _e: &MouseDownEvent, window, cx| {
                            this.activatetab(index, window, cx);
                            cx.stop_propagation();
                        }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(SharedString::from(title)),
                    )
                    .child(
                        div()
                            .id(("overflowclose", index))
                            .px(px(4.0))
                            .rounded(px(4.0))
                            .text_color(dim)
                            .hover(|s| s.bg(sel).text_color(fg))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _e: &MouseDownEvent, window, cx| {
                                    cx.stop_propagation();
                                    this.closetab(index, window, cx);
                                }),
                            )
                            .child("\u{00d7}"),
                    ),
            );
        }

        let dismiss = |this: &mut Self, _e: &MouseDownEvent, _w: &mut Window, cx: &mut Context<Self>| {
            this.tab_overflow = false;
            cx.stop_propagation();
            cx.notify();
        };
        deferred(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .on_mouse_down(MouseButton::Left, cx.listener(dismiss))
                .on_mouse_down(MouseButton::Right, cx.listener(dismiss))
                .child(
                    anchored()
                        .position(pos)
                        .snap_to_window_with_margin(px(6.0))
                        .child(menu),
                ),
        )
        .into_any_element()
    }

    pub(crate) fn tabindex(&self, pane: PaneId) -> Option<usize> {
        (0..self.tabs.len()).find(|i| self.tabs.get(*i).is_some_and(|t| t.tree.contains(pane)))
    }

    /// One label per tab: its override when set, else its focused pane's title.
    pub(crate) fn titles(&self, cx: &App) -> Vec<String> {
        (0..self.tabs.len())
            .map(|i| {
                let tab = self.tabs.get(i).expect("tab index");
                if let Some(title) = &tab.title {
                    return title.clone();
                }
                self.panes
                    .get(&tab.focused)
                    .map(|pane| pane.content.title(cx))
                    .unwrap_or_default()
            })
            .collect()
    }

    /// Per-tab strip data: the label and whether any pane in the tab has a
    /// pending notification.
    pub(crate) fn tab_infos(&self, cx: &App) -> Vec<crate::tabbar::TabInfo> {
        (0..self.tabs.len())
            .map(|i| {
                let tab = self.tabs.get(i).expect("tab index");
                let title = match &tab.title {
                    Some(t) => t.clone(),
                    None => self
                        .panes
                        .get(&tab.focused)
                        .map(|p| {
                            let full = p.content.title(cx);
                            if self.opts.tab_title_show_host {
                                full
                            } else {
                                strip_user_host(&full).to_string()
                            }
                        })
                        .unwrap_or_default(),
                };
                let attention = tab.tree.panes().iter().any(|id| {
                    self.panes
                        .get(id)
                        .is_some_and(|p| p.content.needs_attention(cx))
                });
                crate::tabbar::TabInfo { title, attention }
            })
            .collect()
    }

    pub fn newtab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.zoomed = false;
        if let Some(id) = self.spawnpane(window, cx) {
            self.tabs.new_tab(id);
            self.focusactive(window, cx);
            cx.notify();
        }
    }

    /// Activate a 1-based tab index; negative counts from the end.
    pub(crate) fn gototab(&mut self, n: i32, window: &mut Window, cx: &mut Context<Self>) {
        let len = self.tabs.len() as i32;
        let index = if n < 0 { len + n } else { n - 1 };
        if (0..len).contains(&index) {
            self.activatetab(index as usize, window, cx);
        }
    }

    /// Reorder the active tab by a signed delta, clamped to the ends.
    pub(crate) fn movetab(&mut self, delta: i32, cx: &mut Context<Self>) {
        let from = self.tabs.active_index();
        let len = self.tabs.len() as i32;
        let to = (from as i32 + delta).clamp(0, len - 1) as usize;
        if self.tabs.move_tab(from, to) {
            cx.notify();
        }
    }

    /// Open the rename window for the active tab or the focused pane.
    pub(crate) fn changetitle(&mut self, tab: bool, window: &mut Window, cx: &mut Context<Self>) {
        if tab {
            let index = self.tabs.active_index();
            let initial = self.tabs.active().title.clone().unwrap_or_default();
            self.open_rename(crate::rename::Target::Tab(index), initial, window, cx);
        } else {
            let pane = self.tabs.focused();
            let initial = self
                .panes
                .get(&pane)
                .map(|p| p.content.title(cx))
                .unwrap_or_default();
            self.open_rename(crate::rename::Target::Pane(pane), initial, window, cx);
        }
    }

    /// Set the active-tab label override (called back from the rename window).
    pub fn rename_tab(&mut self, index: usize, title: &str, cx: &mut Context<Self>) {
        let trimmed = title.trim();
        let value = (!trimmed.is_empty()).then(|| trimmed.to_string());
        if self.tabs.set_title(index, value) {
            cx.notify();
        }
    }

    /// Set a pane's title override (called back from the rename window).
    pub fn rename_pane(&mut self, pane: PaneId, title: &str, cx: &mut Context<Self>) {
        if let Some(v) = self.panes.get(&pane).and_then(|p| p.content.as_terminal()) {
            v.update(cx, |view, cx| view.set_title_override(title, cx));
        }
    }
}
