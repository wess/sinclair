use super::*;
use gpui::prelude::*;

impl WorkspaceView {
    /// Spawn a session, wrap it in a pane view, wire its event bridge and
    /// subscription, and register it. `None` if the shell failed to spawn.
    pub(crate) fn spawn(
        &mut self,
        options: terminal::SessionOptions,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<PaneId> {
        let (session, events) = match Session::spawn(options) {
            Ok(pair) => pair,
            Err(error) => {
                eprintln!("prompt: failed to spawn shell: {error}");
                return None;
            }
        };
        let session = Arc::new(session);
        let fallback = session::shellname(self.opts.shell.as_deref());
        let view = cx.new(|cx| {
            TerminalView::new(
                session,
                self.colors.clone(),
                self.font.clone(),
                self.font_size,
                self.cell,
                self.pad,
                self.opts.cursor_style,
                self.opts.copy_on_select,
                self.opts.smart_select,
                self.opts.middle_click_paste,
                self.opts.macos_option_as_alt,
                self.opts.clipboard_paste_protection,
                self.opts.clipboard_write,
                fallback,
                window,
                cx,
            )
        });

        let weak = view.downgrade();
        let mut events = bridge::forward(events);
        window
            .spawn(cx, async move |cx| {
                while let Some(event) = events.next().await {
                    if weak.update(cx, |view, cx| view.apply(event, cx)).is_err() {
                        break;
                    }
                }
            })
            .detach();

        let id = self.ids.next();
        let subscription = cx.subscribe_in(
            &view,
            window,
            move |this: &mut Self, _view, event: &ViewEvent, window, cx| {
                this.paneevent(id, event, window, cx);
            },
        );
        self.panes.insert(
            id,
            Pane {
                content: PaneContent::Terminal(view),
                _subscription: Some(subscription),
            },
        );
        Some(id)
    }

    /// Create a pane hosting a plugin web view (no terminal, no event bridge),
    /// register it, and return its id. Used for `[webview] placement = "tab"`.
    pub(crate) fn spawn_webview_pane(
        &mut self,
        surface: crate::pluginwebview::WebviewSurface,
        cx: &mut Context<Self>,
    ) -> PaneId {
        let view = cx.new(|cx| crate::pluginwebview::PluginWebView::new(surface, cx));
        let id = self.ids.next();
        self.panes.insert(
            id,
            Pane {
                content: PaneContent::Webview(view),
                _subscription: None,
            },
        );
        id
    }

    /// Spawn a pane inheriting the focused pane's working directory.
    pub(crate) fn spawnpane(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<PaneId> {
        let inherit = self
            .panes
            .get(&self.tabs.focused())
            .and_then(|pane| pane.content.cwd(cx))
            .and_then(|osc| session::cwdpath(&osc));
        let options = session::options(&self.opts, SPAWN_COLS, SPAWN_ROWS, inherit);
        self.spawn(options, window, cx)
    }

    fn paneevent(
        &mut self,
        pane: PaneId,
        event: &ViewEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            ViewEvent::Title => {
                if pane == self.tabs.focused() {
                    self.settitle(window, cx);
                }
                cx.notify();
            }
            ViewEvent::Exited => self.closepane(pane, window, cx),
            ViewEvent::Input(bytes) => self.broadcast(pane, bytes, cx),
            ViewEvent::Action(action) => {
                self.tabs.focus(pane);
                self.focusactive(window, cx);
                self.dispatch(action.clone(), window, cx);
            }
            ViewEvent::Attention => cx.notify(),
            ViewEvent::Trigger(ev) => self.fire_triggers(pane, ev, window, cx),
        }
    }

    /// Mirror `bytes` (already encoded by the source pane) to every other pane
    /// in the active tab. The source already wrote them to its own pty.
    fn broadcast(&mut self, source: PaneId, bytes: &[u8], cx: &mut Context<Self>) {
        for id in self.tabs.active().tree.panes() {
            if id == source {
                continue;
            }
            if let Some(v) = self.panes.get(&id).and_then(|p| p.content.as_terminal()) {
                v.update(cx, |view, cx| view.send_text(bytes, cx));
            }
        }
    }

    /// Toggle broadcast input. Repaints panes (for the indicator) and rebuilds
    /// menus (for the checkmark).
    pub(crate) fn toggle_broadcast(&mut self, cx: &mut Context<Self>) {
        let on = cx.try_global::<Broadcast>().is_some_and(|b| b.0);
        cx.set_global(Broadcast(!on));
        self.setmenus(cx);
        for pane in self.panes.values() {
            if let Some(v) = pane.content.as_terminal() {
                v.update(cx, |_v, cx| cx.notify());
            }
        }
        cx.notify();
    }

    /// Close one pane: collapse its split, or close its tab when it is the
    /// last pane there, or quit when it is the last pane of the last tab.
    pub(crate) fn closepane(&mut self, pane: PaneId, window: &mut Window, cx: &mut Context<Self>) {
        self.zoomed = false;
        self.on_pane_closed(pane);
        let Some(index) = self.tabindex(pane) else {
            return;
        };
        let lastpane = self.tabs.get(index).expect("tab").tree.panes().len() == 1;
        if lastpane && self.tabs.len() == 1 {
            self.close_window(window, cx);
            return;
        }
        if lastpane {
            self.tabs.close_tab(index);
        } else {
            let previous = self.tabs.active_index();
            self.tabs.activate(index);
            let next = (self.tabs.focused() == pane)
                .then(|| workspace::next(&self.tabs.active().tree, pane))
                .flatten();
            self.tabs.active_mut().tree.remove(pane);
            if let Some(next) = next {
                self.tabs.focus(next);
            }
            self.tabs.activate(previous);
        }
        self.panes.remove(&pane);
        self.focusactive(window, cx);
        cx.notify();
    }

    /// Close a whole tab (tab-bar close glyph), dropping all its panes.
    pub fn closetab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tab) = self.tabs.get(index) else {
            return;
        };
        let removed = tab.tree.panes();
        if self.tabs.len() == 1 {
            self.close_window(window, cx);
            return;
        }
        self.tabs.close_tab(index);
        for pane in removed {
            self.on_pane_closed(pane);
            self.panes.remove(&pane);
        }
        self.focusactive(window, cx);
        cx.notify();
    }

    /// Close just this window. The app keeps running while other windows are
    /// open; only when this is the last window do we honor
    /// `quit-after-last-window-closed` (macOS keeps the app alive otherwise).
    pub(crate) fn close_window(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.save_state(cx);
        let last_window = cx.windows().len() <= 1;
        if last_window && self.opts.quit_after_last_window_closed {
            cx.quit();
        } else {
            window.remove_window();
        }
    }

    pub fn focuspane(&mut self, pane: PaneId, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.focus(pane) {
            self.focusactive(window, cx);
            cx.notify();
        }
    }

    /// Open another top-level window, cloning this window's current
    /// appearance so the new one matches without re-reading config.
    pub(crate) fn newwindow(&self, cx: &mut Context<Self>) {
        let cwd = self
            .panes
            .get(&self.tabs.focused())
            .and_then(|pane| pane.content.cwd(cx))
            .and_then(|osc| session::cwdpath(&osc));
        crate::open_window(
            self.opts.clone(),
            self.colors.clone(),
            self.font.clone(),
            self.font_size,
            self.cell,
            self.pad,
            cwd,
            cx,
        );
    }
}
