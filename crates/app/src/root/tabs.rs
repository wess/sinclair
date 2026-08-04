use super::*;
use gpui::prelude::*;

impl WorkspaceView {
    /// Apply an [`Action::Sidebar`] payload.
    ///
    /// - `left` / `right` — toggle that whole dock open or closed.
    /// - `<side>:<token>` — **reveal** that section: expand it wherever it
    ///   actually lives (opening that dock), collapse it if it was already
    ///   showing. The side in the payload only decides where a section that is
    ///   in neither dock gets added, so a binding like `sidebar:left:containers`
    ///   keeps working after Containers has been moved to the right.
    pub fn toggle_sidebar(&mut self, payload: &str, cx: &mut Context<Self>) {
        // Parse `side[:token]`; resolve the token (built-in or `plugin:<id>`)
        // against the live plugin set, since plugin panels aren't statically known.
        let (side_str, token) = match payload.split_once(':') {
            Some((s, t)) => (s.trim(), Some(t.trim())),
            None => (payload.trim(), None),
        };
        let Some(side) = SidebarSide::parse(side_str) else {
            return;
        };

        let revealed = match token {
            None => {
                dock::toggle_side(&mut self.docks, side);
                None
            }
            Some(t) => match self.panel_from_token(t) {
                // An unknown token is a stale binding or an uninstalled plugin;
                // do nothing rather than opening the wrong thing.
                None => return,
                Some(panel) => Some((panel, dock::reveal(&mut self.docks, side, panel))),
            },
        };

        // Load what the newly-visible section needs. Only on the way *open* —
        // collapsing a section should never kick off a fetch.
        if let Some((panel, out)) = revealed {
            if out.expanded {
                self.on_section_shown(panel, cx);
            }
        } else if self.docks[side.index()].open {
            // A whole dock just opened; prime every section showing in it.
            for section in self.docks[side.index()].sections.clone() {
                if section.expanded {
                    self.on_section_shown(section.panel, cx);
                }
            }
        }

        // A panel `[webview]` hosts a native surface that only tracks its bounds
        // while painted; hide any whose section just collapsed or whose dock closed.
        self.reconcile_webview_visibility(cx);
        self.setmenus(cx);
        cx.notify();
    }

    /// Expand or collapse the section at `index` on `side` — the click path for
    /// the rail icons and the section headers.
    ///
    /// Index-based on purpose: those two draw a known section, so routing them
    /// through [`Self::toggle_sidebar`] would mean formatting a payload only to
    /// parse it apart and re-resolve it. Both are rebuilt on every repaint, and
    /// the workspace repaints on terminal output.
    pub(crate) fn toggle_section(
        &mut self,
        side: SidebarSide,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        let Some((panel, expanded)) = dock::toggle_at(&mut self.docks, side, index) else {
            return;
        };
        if expanded {
            self.on_section_shown(panel, cx);
        }
        self.reconcile_webview_visibility(cx);
        self.setmenus(cx);
        cx.notify();
    }

    /// Refresh whatever a section needs the moment it becomes visible. Every
    /// one of these reads disk, shells out, or hits the network, which is why
    /// it happens here and never in `render`.
    pub(crate) fn on_section_shown(&mut self, panel: SidebarPanel, cx: &mut Context<Self>) {
        match panel {
            SidebarPanel::Plugin(i) => self.refresh_plugin_panel(i, cx),
            // The catalog is a GitHub call; fetch it once, lazily.
            SidebarPanel::Plugins if self.catalog.is_none() => self.fetch_catalog(cx),
            SidebarPanel::Containers => self.refresh_containers(),
            // These render from the off-thread menu-data cache; re-snapshot so
            // the section reflects current disk state.
            SidebarPanel::Agents | SidebarPanel::Layouts => self.refresh_menu_data(cx),
            SidebarPanel::Worktrees => self.refresh_worktrees(cx),
            SidebarPanel::Notes => self.refresh_notes(),
            _ => {}
        }
    }

    /// Reconcile every native `[webview]` surface with what's actually on screen:
    /// a panel host is visible only while its drawer is the active panel; a
    /// webview *item* only while it is a live item in the group. A native OS view
    /// tracks its bounds only while painted, so one that stops rendering must be
    /// hidden explicitly or it lingers. See [`PluginWebView::set_visible`].
    pub(crate) fn reconcile_webview_visibility(&self, cx: &mut Context<Self>) {
        // Panel-placement webviews, keyed by id in `webview_hosts`.
        let panel_visible = self.active_webview_ids();
        let panel_hosts: Vec<(String, gpui::Entity<crate::pluginwebview::PluginWebView>)> =
            self.webview_hosts.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        for (id, host) in panel_hosts {
            let vis = panel_visible.contains(&id);
            host.update(cx, |h, cx| h.set_visible(vis, cx));
        }

        // Webview items: visible while they exist in the group's item set.
        let active: std::collections::HashSet<ItemId> =
            self.group.read(cx).items().into_iter().collect();
        let item_hosts: Vec<(ItemId, gpui::Entity<crate::pluginwebview::PluginWebView>)> = self
            .items
            .borrow()
            .iter()
            .filter_map(|(id, it)| match &it.content {
                PaneContent::Webview(host) => Some((*id, host.clone())),
                PaneContent::Terminal(_) => None,
            })
            .collect();
        for (id, host) in item_hosts {
            let vis = active.contains(&id);
            host.update(cx, |h, cx| h.set_visible(vis, cx));
        }
    }

    /// The webview ids on screen right now: a webview section counts only when
    /// its dock is open *and* the section itself is expanded, since a collapsed
    /// section paints nothing but its header.
    fn active_webview_ids(&self) -> std::collections::HashSet<String> {
        let defs = self.plugin_webview_panel_defs();
        self.docks
            .iter()
            .filter(|d| d.open)
            .flat_map(|d| d.sections.iter())
            .filter(|s| s.expanded)
            .filter_map(|s| match s.panel {
                SidebarPanel::Webview(i) => defs
                    .get(i)
                    .and_then(|p| p.webview.as_ref())
                    .map(|w| w.id.clone()),
                _ => None,
            })
            .collect()
    }

    /// One title per item, in the group's layout order (for the MCP bridge).
    pub(crate) fn titles(&self, cx: &App) -> Vec<String> {
        self.group
            .read(cx)
            .items()
            .into_iter()
            .map(|id| {
                self.items
                    .borrow()
                    .get(&id)
                    .map(|it| it.content.title(cx))
                    .unwrap_or_default()
            })
            .collect()
    }

    /// Open a new terminal as a tab in the focused pane.
    pub fn newtab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.spawn_default(window, cx) {
            self.group.update(cx, |g, cx| g.add_to_focused(id, cx));
            self.focusactive(window, cx);
            cx.notify();
        }
    }

    /// Activate a 1-based item index (across all panes); negative counts from
    /// the end.
    pub(crate) fn gototab(&mut self, n: i32, window: &mut Window, cx: &mut Context<Self>) {
        let items = self.group.read(cx).items();
        let len = items.len() as i32;
        let index = if n < 0 { len + n } else { n - 1 };
        if let Some(&item) = items.get(index.clamp(0, len.max(1) - 1) as usize) {
            if (0..len).contains(&index) {
                self.activate_item(item, window, cx);
            }
        }
    }

    /// Reorder the focused item within its pane by a signed delta, clamped.
    pub(crate) fn movetab(&mut self, delta: i32, cx: &mut Context<Self>) {
        let item = self.group.read(cx).active_item();
        let pane = self.group.read(cx).focused_pane();
        let Some(items) = self.group.read(cx).pane_items(pane).map(<[_]>::to_vec) else {
            return;
        };
        let Some(from) = items.iter().position(|i| *i == item) else {
            return;
        };
        let to = (from as i32 + delta).clamp(0, items.len() as i32 - 1) as usize;
        if to != from {
            self.group.update(cx, |g, cx| g.reorder_in_pane(item, to, cx));
            cx.notify();
        }
    }

    /// Open the rename dialog for the focused item's terminal title. (Tab and
    /// terminal titles are one and the same now that tabs live per-pane.)
    pub(crate) fn changetitle(&mut self, _tab: bool, window: &mut Window, cx: &mut Context<Self>) {
        let item = self.group.read(cx).active_item();
        let initial = self
            .items
            .borrow()
            .get(&item)
            .map(|it| it.content.title(cx))
            .unwrap_or_default();
        self.open_rename(crate::rename::Target::Item(item), initial, window, cx);
    }

    /// Set an item's title override (called back from the rename dialog).
    pub fn rename_item(&mut self, item: ItemId, title: &str, cx: &mut Context<Self>) {
        let view = self
            .items
            .borrow()
            .get(&item)
            .and_then(|it| it.content.as_terminal().cloned());
        if let Some(v) = view {
            v.update(cx, |view, cx| view.set_title_override(title, cx));
        }
    }
}
