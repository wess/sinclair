use super::*;
use gpui::prelude::*;

impl WorkspaceView {
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
