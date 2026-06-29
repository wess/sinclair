use super::*;

impl TerminalView {
    pub(crate) fn write_paste(&mut self, text: &str, cx: &mut Context<Self>) {
        let bracketed = self.session.with_term(|term| term.bracketed_paste());
        self.scroll_to_bottom(cx);
        let _ = self.session.write(&input::encode_paste(text, bracketed));
    }

    /// Right mouse button: open the context menu, unless the app is capturing
    /// the mouse (then the press is reported to the child instead).
    pub(crate) fn right_down(&mut self, e: &MouseDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let reporting = self
            .session
            .with_term(|t| crate::mouse::reports(t.mouse_mode(), e.modifiers.shift));
        if reporting {
            return;
        }
        self.context_menu = Some(e.position);
        cx.stop_propagation();
        cx.notify();
    }

    /// The right-click menu overlay anchored at `pos`, with a full-pane
    /// backdrop that dismisses it on an outside click.
    pub(crate) fn context_menu_overlay(&self, pos: Point<Pixels>, cx: &mut Context<Self>) -> AnyElement {
        let items: [Option<(&str, Action)>; 8] = [
            Some(("Copy", Action::Copy)),
            Some(("Paste", Action::Paste)),
            Some(("Select All", Action::SelectAll)),
            None,
            Some(("Split Right", Action::NewSplit(SplitDirection::Right))),
            Some(("Split Down", Action::NewSplit(SplitDirection::Down))),
            None,
            Some(("Clear", Action::ClearScreen)),
        ];
        let mut menu = div()
            .flex()
            .flex_col()
            .min_w(px(180.0))
            .p_1()
            .rounded(px(8.0))
            .border_1()
            .border_color(colors::rgba(self.colors.selection_bg))
            .bg(colors::rgba(self.colors.bg))
            .text_color(colors::rgba(self.colors.fg))
            .shadow_lg();
        for item in items {
            match item {
                None => {
                    let mut line = colors::rgba(self.colors.fg);
                    line.a = 0.15;
                    menu = menu.child(div().my_1().mx_2().h(px(1.0)).bg(line));
                }
                Some((label, action)) => {
                    let action = action.clone();
                    menu = menu.child(
                        div()
                            .px_2()
                            .py(px(4.0))
                            .rounded(px(5.0))
                            .hover(|s| s.bg(colors::rgba(self.colors.selection_bg)))
                            .child(SharedString::from(label))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _e: &MouseDownEvent, _w, cx| {
                                    this.context_menu = None;
                                    cx.emit(ViewEvent::Action(action.clone()));
                                    cx.stop_propagation();
                                    cx.notify();
                                }),
                            ),
                    );
                }
            }
        }
        let dismiss = |this: &mut Self, _e: &MouseDownEvent, _w: &mut Window, cx: &mut Context<Self>| {
            this.context_menu = None;
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

    /// Copy the current selection to the clipboard, if any.
    pub fn copy_selection(&mut self, cx: &mut Context<Self>) {
        let Some(text) = self.session.with_term(|term| term.selection_text()) else {
            return;
        };
        if !text.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    /// Paste the clipboard into the pty (bracketed when the app requested it).
    pub fn paste_clipboard(&mut self, cx: &mut Context<Self>) {
        if self.read_only {
            return;
        }
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        if text.is_empty() {
            return;
        }
        if self.paste_protection {
            let risk = ::assist::analyze(&text);
            if risk.risky() {
                self.search = None;
                self.assist = Some(Assist::Paste { text, risk });
                cx.notify();
                return;
            }
        }
        self.write_paste(&text, cx);
    }

    /// Place text on the system clipboard on behalf of an OSC 52 write, once
    /// the configured policy has cleared it.
    pub(crate) fn write_clipboard(&self, text: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(text));
    }

    /// Select the entire buffer, every scrollback row plus the live screen,
    /// so it can be copied. Mirrors a top-left to bottom-right cell drag.
    pub fn select_all(&mut self, cx: &mut Context<Self>) {
        self.session.with_term(|term| {
            let grid = term.grid();
            let (rows, cols) = (grid.rows(), grid.cols());
            if rows == 0 || cols == 0 {
                return;
            }
            let top = -(grid.scrollback().len() as isize);
            let bottom = rows as isize - 1;
            term.start_selection(vt::SelectionMode::Cell, vt::Point::new(top, 0));
            term.update_selection(vt::Point::new(bottom, cols - 1));
        });
        cx.notify();
    }
}
