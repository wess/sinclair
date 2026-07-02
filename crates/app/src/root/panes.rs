use super::*;
use gpui::prelude::*;

impl WorkspaceView {
    /// Spawn a session, wrap it in a terminal view, wire its event bridge and
    /// subscription, and register it as an item. `None` if the shell failed to
    /// spawn. The caller places the returned item into the group.
    pub(crate) fn spawn(
        &mut self,
        options: terminal::SessionOptions,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<ItemId> {
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

        let id = self.item_ids.next();
        let subscription = cx.subscribe_in(
            &view,
            window,
            move |this: &mut Self, _view, event: &ViewEvent, window, cx| {
                this.itemevent(id, event, window, cx);
            },
        );
        self.items.borrow_mut().insert(
            id,
            Item {
                content: PaneContent::Terminal(view),
                _subscription: Some(subscription),
            },
        );
        Some(id)
    }

    /// Create an item hosting a plugin web view (no terminal, no event bridge),
    /// register it, and return its id. Used for `[webview] placement = "tab"`.
    pub(crate) fn spawn_webview_item(
        &mut self,
        surface: crate::pluginwebview::WebviewSurface,
        cx: &mut Context<Self>,
    ) -> ItemId {
        let view = cx.new(|cx| crate::pluginwebview::PluginWebView::new(surface, cx));
        let id = self.item_ids.next();
        self.items.borrow_mut().insert(
            id,
            Item {
                content: PaneContent::Webview(view),
                _subscription: None,
            },
        );
        id
    }

    /// Spawn an item inheriting the focused item's working directory.
    pub(crate) fn spawn_default(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<ItemId> {
        let inherit = self.focused_cwd_path(cx);
        let options = session::options(&self.opts, SPAWN_COLS, SPAWN_ROWS, inherit);
        self.spawn(options, window, cx)
    }

    fn itemevent(
        &mut self,
        item: ItemId,
        event: &ViewEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            ViewEvent::Title => {
                if item == self.group.read(cx).active_item() {
                    self.settitle(window, cx);
                }
                cx.notify();
            }
            ViewEvent::Exited => self.close_item(item, window, cx),
            ViewEvent::Input(bytes) => self.broadcast(item, bytes, cx),
            ViewEvent::Action(action) => {
                self.activate_item(item, window, cx);
                self.dispatch(action.clone(), window, cx);
            }
            ViewEvent::Attention => cx.notify(),
            ViewEvent::Trigger(ev) => self.fire_triggers(item, ev, window, cx),
        }
    }

    /// Mirror `bytes` (already encoded by the source item) to every other
    /// terminal item in the window. The source already wrote them to its pty.
    fn broadcast(&mut self, source: ItemId, bytes: &[u8], cx: &mut Context<Self>) {
        let targets: Vec<_> = self
            .group
            .read(cx)
            .items()
            .into_iter()
            .filter(|id| *id != source)
            .filter_map(|id| {
                self.items
                    .borrow()
                    .get(&id)
                    .and_then(|it| it.content.as_terminal().cloned())
            })
            .collect();
        for v in targets {
            v.update(cx, |view, cx| view.send_text(bytes, cx));
        }
    }

    /// Toggle broadcast input. Repaints items (for the indicator) and rebuilds
    /// menus (for the checkmark).
    pub(crate) fn toggle_broadcast(&mut self, cx: &mut Context<Self>) {
        let on = cx.try_global::<Broadcast>().is_some_and(|b| b.0);
        cx.set_global(Broadcast(!on));
        self.setmenus(cx);
        let terminals: Vec<_> = self
            .items
            .borrow()
            .values()
            .filter_map(|it| it.content.as_terminal().cloned())
            .collect();
        for v in terminals {
            v.update(cx, |_v, cx| cx.notify());
        }
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

    /// Open another top-level window, cloning this window's current
    /// appearance so the new one matches without re-reading config.
    pub(crate) fn newwindow(&self, cx: &mut Context<Self>) {
        let cwd = self.focused_cwd_path(cx);
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
