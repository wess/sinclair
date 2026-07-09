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

    /// One clickable menu row that runs `on_click` (and closes the menu).
    fn menu_row(
        &self,
        label: impl Into<SharedString>,
        cx: &mut Context<Self>,
        on_click: impl Fn(&mut Self, &mut Context<Self>) + 'static,
    ) -> gpui::Div {
        div()
            .px_2()
            .py(px(4.0))
            .rounded(px(5.0))
            .hover(|s| s.bg(colors::rgba(self.colors.selection_bg)))
            .child(label.into())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _e: &MouseDownEvent, _w, cx| {
                    this.context_menu = None;
                    on_click(this, cx);
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
    }

    /// A menu row that dispatches a bound [`Action`] (so it matches keybinds).
    fn menu_action(&self, label: &'static str, action: Action, cx: &mut Context<Self>) -> gpui::Div {
        self.menu_row(label, cx, move |_this, cx| cx.emit(ViewEvent::Action(action.clone())))
    }

    fn menu_divider(&self) -> gpui::Div {
        let mut line = colors::rgba(self.colors.fg);
        line.a = 0.15;
        div().my_1().mx_2().h(px(1.0)).bg(line)
    }

    /// The link under a window-space `pos`, mapping through the grid bounds
    /// captured in `render`. Used to offer Open/Copy Link on right-click.
    fn link_at_pos(&self, pos: Point<Pixels>) -> Option<String> {
        let (cols, rows) = self.session.with_term(|t| (t.cols(), t.rows()));
        let (row, col) = crate::metrics::cell_at(
            (f32::from(pos.x), f32::from(pos.y)),
            (
                f32::from(self.grid_bounds.origin.x),
                f32::from(self.grid_bounds.origin.y),
            ),
            self.pad,
            self.cell,
            cols,
            rows,
        );
        self.session.with_term(|t| t.link_at(row, col)).map(|l| l.url)
    }

    /// Open a URL via the OS, refusing schemes outside the allow-list.
    pub(crate) fn open_url(&mut self, url: String, cx: &mut Context<Self>) {
        if crate::pointer::openable(&url) {
            cx.open_url(&url);
        } else {
            eprintln!("sinclair: refused to open link with disallowed scheme: {url}");
        }
    }

    fn copy_text(&mut self, text: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(text));
    }

    /// Search the web for `query` with the default browser's search.
    fn search_web(&mut self, query: &str, cx: &mut Context<Self>) {
        let q: String = query.chars().take(256).collect();
        cx.open_url(&format!("https://www.google.com/search?q={}", percent_encode(&q)));
    }

    /// macOS "Look Up": open the system dictionary for `term`.
    #[cfg(target_os = "macos")]
    fn look_up(&mut self, term: &str, cx: &mut Context<Self>) {
        let t: String = term.chars().take(64).collect();
        cx.open_url(&format!("dict://{}", percent_encode(&t)));
    }

    /// The right-click menu overlay anchored at `pos`, with a full-pane
    /// backdrop that dismisses it on an outside click. Context-aware: shows
    /// Open/Copy Link over a URL and Look Up / Search Web over a selection.
    pub(crate) fn context_menu_overlay(&self, pos: Point<Pixels>, cx: &mut Context<Self>) -> AnyElement {
        let link = self.link_at_pos(pos);
        let selection = self
            .session
            .with_term(|t| t.selection_text())
            .filter(|s| !s.trim().is_empty());

        let mut menu = div()
            .flex()
            .flex_col()
            .min_w(px(200.0))
            .p_1()
            .rounded(px(8.0))
            .border_1()
            .border_color(colors::rgba(self.colors.selection_bg))
            .bg(colors::rgba(self.colors.bg))
            .text_color(colors::rgba(self.colors.fg))
            .shadow_lg();

        if let Some(url) = link {
            let open = url.clone();
            menu = menu.child(self.menu_row("Open Link", cx, move |this, cx| {
                this.open_url(open.clone(), cx)
            }));
            let copy = url.clone();
            menu = menu.child(self.menu_row("Copy Link Address", cx, move |this, cx| {
                this.copy_text(copy.clone(), cx)
            }));
            menu = menu.child(self.menu_divider());
        }

        menu = menu
            .child(self.menu_action("Copy", Action::Copy, cx))
            .child(self.menu_action("Paste", Action::Paste, cx))
            .child(self.menu_action("Select All", Action::SelectAll, cx));

        if let Some(sel) = selection {
            menu = menu.child(self.menu_divider());
            let label = trunc(&sel);
            #[cfg(target_os = "macos")]
            {
                let term = sel.clone();
                menu = menu.child(self.menu_row(format!("Look Up \u{201c}{label}\u{201d}"), cx, move |this, cx| {
                    this.look_up(&term, cx)
                }));
            }
            let query = sel.clone();
            menu = menu.child(self.menu_row(format!("Search Web for \u{201c}{label}\u{201d}"), cx, move |this, cx| {
                this.search_web(&query, cx)
            }));
        }

        menu = menu
            .child(self.menu_divider())
            .child(self.menu_action("New Tab", Action::NewTab, cx))
            .child(self.menu_action("Split Right", Action::NewSplit(SplitDirection::Right), cx))
            .child(self.menu_action("Split Down", Action::NewSplit(SplitDirection::Down), cx))
            .child(self.menu_action("Close Pane", Action::CloseSurface, cx))
            .child(self.menu_divider())
            .child(self.menu_action("Clear", Action::ClearScreen, cx));

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
            let text = crate::redact::mask(text, cx);
            crate::clipboard::remember(&text, cx);
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    /// Copy the most recent finished command's output — the rows between the two
    /// newest OSC 133 prompt marks (or the last prompt to the bottom when a
    /// command is still running). A no-op without shell integration / marks.
    pub fn copy_command_output(&mut self, cx: &mut Context<Self>) {
        let text = self.session.with_term(|t| {
            let prompts = t.prompt_lines();
            let lines = t.text_lines();
            let total = lines.len();
            // Half-open output range in global-row space, skipping the command row.
            let (start, end) = match prompts.len() {
                0 => return None,
                1 => (prompts[0] + 1, total),
                n => (prompts[n - 2] + 1, prompts[n - 1]),
            };
            let mut out = String::new();
            for (idx, text, _) in &lines {
                if *idx >= start && *idx < end {
                    out.push_str(text.trim_end());
                    out.push('\n');
                }
            }
            let out = out.trim_end().to_string();
            (!out.is_empty()).then_some(out)
        });
        if let Some(text) = text {
            let text = crate::redact::mask(text, cx);
            crate::clipboard::remember(&text, cx);
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

    /// Extend the active selection one step in `dir`. With no active
    /// selection this falls through to the key's normal escape sequence, so
    /// shift+navigation extends a selection when one exists and otherwise
    /// behaves exactly as if unbound, keeping shift+navigation
    /// "performable".
    pub fn adjust_selection(&mut self, dir: config::SelectAdjust, cx: &mut Context<Self>) {
        // `key` (gpui's spelling, used only on the fall-through) cannot be
        // derived from `config::SelectAdjust::as_str`, which spells the paged
        // keys `page_up`/`page_down` rather than `pageup`/`pagedown`.
        let (vtdir, key) = match dir {
            config::SelectAdjust::Left => (vt::SelectionAdjust::Left, "left"),
            config::SelectAdjust::Right => (vt::SelectionAdjust::Right, "right"),
            config::SelectAdjust::Up => (vt::SelectionAdjust::Up, "up"),
            config::SelectAdjust::Down => (vt::SelectionAdjust::Down, "down"),
            config::SelectAdjust::Home => (vt::SelectionAdjust::Home, "home"),
            config::SelectAdjust::End => (vt::SelectionAdjust::End, "end"),
            config::SelectAdjust::PageUp => (vt::SelectionAdjust::PageUp, "pageup"),
            config::SelectAdjust::PageDown => (vt::SelectionAdjust::PageDown, "pagedown"),
            config::SelectAdjust::WordLeft => (vt::SelectionAdjust::WordLeft, ""),
            config::SelectAdjust::WordRight => (vt::SelectionAdjust::WordRight, ""),
            // To line start/end reuses the Home/End motion but, like the word
            // motions, begins a selection at the cursor rather than falling
            // through (these are GUI-only ⌘⇧ bindings).
            config::SelectAdjust::LineStart => (vt::SelectionAdjust::Home, ""),
            config::SelectAdjust::LineEnd => (vt::SelectionAdjust::End, ""),
        };
        // These motions (⌘⇧←/→ to line edges, ⌥⇧←/→ by word) begin a
        // selection at the cursor when none exists and always act — their
        // modifiers are GUI-only, never sent to the pty, so there's nothing
        // to fall through to.
        if matches!(
            dir,
            config::SelectAdjust::WordLeft
                | config::SelectAdjust::WordRight
                | config::SelectAdjust::LineStart
                | config::SelectAdjust::LineEnd
        ) {
            self.session.with_term(|term| term.extend_selection(vtdir));
            cx.notify();
            return;
        }
        if self.session.with_term(|term| term.adjust_selection(vtdir)) {
            cx.notify();
            return;
        }
        // No selection: send the key's standard sequence so shells and TUI
        // apps behave as if the binding didn't exist. The default trigger is
        // shift+<key>, so that is what we reproduce.
        let mods = input::Mods {
            shift: true,
            alt: false,
            ctrl: false,
            cmd: false,
        };
        if let Some(bytes) = input::encode_key(key, None, mods, self.term_state()) {
            self.send_text(&bytes, cx);
        }
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

/// Percent-encode a query string for a URL (RFC 3986 unreserved set kept).
fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Truncate menu text (a selection or link) to a short, single-line label.
fn trunc(s: &str) -> String {
    let flat = s.replace(['\n', '\r', '\t'], " ");
    let flat = flat.trim();
    let short: String = flat.chars().take(28).collect();
    if flat.chars().count() > 28 {
        format!("{short}\u{2026}")
    } else {
        short
    }
}
