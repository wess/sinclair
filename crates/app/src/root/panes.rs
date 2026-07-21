use std::sync::atomic::{AtomicU64, Ordering};

use super::*;
use gpui::prelude::*;

/// Process-global allocator for pane tokens: a stable id, unique across every
/// window, injected into each spawned session as `SINCLAIR_PANE` so an agent's
/// hooks can report state for exactly this pane (see `agenthooks.rs`).
static PANE_SEQ: AtomicU64 = AtomicU64::new(0);

impl WorkspaceView {
    /// Spawn a session, wrap it in a terminal view, wire its event bridge and
    /// subscription, and register it as an item. `None` if the shell failed to
    /// spawn. The caller places the returned item into the group.
    pub(crate) fn spawn(
        &mut self,
        mut options: terminal::SessionOptions,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<ItemId> {
        // Tag the session so an agent's hooks can address this exact pane. The
        // `PROMPT_*` twins are the pre-rename names, kept so existing user
        // scripts and hooks keep resolving; drop them a release after 1.26.
        let token = PANE_SEQ.fetch_add(1, Ordering::Relaxed) + 1;
        let socket = crate::ipc::socket_env();
        for (key, value) in [
            ("SINCLAIR_PANE", token.to_string()),
            ("PROMPT_PANE", token.to_string()),
            ("SINCLAIR_SOCKET", socket.clone()),
            ("PROMPT_SOCKET", socket),
        ] {
            options.spawn.env.push((key.to_string(), value));
        }
        let (session, events) = match Session::spawn(options) {
            Ok(pair) => pair,
            Err(error) => {
                eprintln!("sinclair: failed to spawn shell: {error}");
                // Remembered for the in-window error view (shown when the
                // window ends up with no live items at all).
                self.spawn_error = Some(error.to_string());
                return None;
            }
        };
        self.spawn_error = None;
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
                self.opts.unfocused_split_opacity,
                crate::suggest::SuggestConfig::from_opts(&self.opts),
                fallback,
                window,
                cx,
            )
        });

        let weak = view.downgrade();
        let mut events = bridge::forward(events);
        // App-scoped (not window-scoped) so the pty event pump survives the item
        // being torn off into another window.
        cx.spawn(async move |_, cx| {
            while let Some(event) = events.next().await {
                if weak.update(cx, |view, cx| view.apply(event, cx)).is_err() {
                    break;
                }
            }
        })
        .detach();

        Some(self.insert_item(PaneContent::Terminal(view), token, window, cx))
    }

    /// Register existing content as an item with no pane token. Used by tear-off
    /// adoption (re-homing a live terminal into a new window; its original token
    /// stays in the session's environment but is not re-tracked here).
    pub(crate) fn register_item(
        &mut self,
        content: PaneContent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> ItemId {
        // The content's focus listeners still watch the window it was torn out
        // of; point them at this one before it can be focused here.
        if let PaneContent::Terminal(view) = &content {
            view.update(cx, |v, cx| v.rehome(window, cx));
        }
        self.insert_item(content, 0, window, cx)
    }

    /// Allocate an id, wire the terminal `ViewEvent` bridge to this workspace
    /// (webviews emit none), and insert the item into `items` under `token`.
    fn insert_item(
        &mut self,
        content: PaneContent,
        token: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> ItemId {
        let id = self.item_ids.next();
        let subscription = match &content {
            PaneContent::Terminal(view) => Some(cx.subscribe_in(
                view,
                window,
                move |this: &mut Self, _view, event: &ViewEvent, window, cx| {
                    this.itemevent(id, event, window, cx);
                },
            )),
            PaneContent::Webview(_) => None,
        };
        self.items.borrow_mut().insert(
            id,
            Item {
                content,
                _subscription: subscription,
                pane_token: token,
                agent: None,
                agent_session: None,
                command: None,
            },
        );
        id
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
                pane_token: 0,
                agent: None,
                agent_session: None,
                command: None,
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
            ViewEvent::Focused => {
                if self.group.read(cx).active_item() != item {
                    self.activate_item(item, window, cx);
                }
            }
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
    /// Only the *last* window persists its session: a per-window save would
    /// make whichever window closed last overwrite the surviving windows'
    /// state (the quit path saves separately, see `request_quit`).
    pub(crate) fn close_window(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let last_window = cx.windows().len() <= 1;
        if last_window {
            self.save_state(cx);
        }
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
            None,
            None,
            cx,
        );
    }
}
