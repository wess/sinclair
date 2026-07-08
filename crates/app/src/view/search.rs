use super::*;

impl TerminalView {
    /// Open/close the scrollback search overlay.
    pub fn toggle_search(&mut self, cx: &mut Context<Self>) {
        self.assist = None;
        self.search = match self.search {
            Some(_) => None,
            None => Some(Search {
                edit: guise::TextEdit::new(""),
                current: 0,
                cached_query: None,
                results: Vec::new(),
                dirty: true,
            }),
        };
        cx.notify();
    }

    /// Current search results, cached until the query changes or new output
    /// marks them stale: the full-buffer scan never runs on an idle repaint.
    pub(crate) fn search_matches(&mut self) -> Vec<vt::Match> {
        let q = match &self.search {
            Some(s) => s.edit.text(),
            None => return Vec::new(),
        };
        if let Some(s) = &self.search {
            if !s.dirty && s.cached_query.as_deref() == Some(q.as_str()) {
                return s.results.clone();
            }
        }
        let hits = if q.is_empty() {
            Vec::new()
        } else {
            self.session.with_term(|t| t.search(&q, false))
        };
        if let Some(s) = &mut self.search {
            s.results = hits.clone();
            s.cached_query = Some(q);
            s.dirty = false;
        }
        hits
    }

    /// Clamp the focused match and scroll it into view.
    fn search_jump(&mut self, cx: &mut Context<Self>) {
        let matches = self.search_matches();
        let Some(s) = self.search.as_mut() else {
            return;
        };
        if matches.is_empty() {
            cx.notify();
            return;
        }
        s.current = s.current.min(matches.len() - 1);
        let line = matches[s.current].line;
        self.session.with_term(|t| {
            let sb = t.grid().scrollback().len();
            t.set_display_offset(sb.saturating_sub(line));
        });
        cx.notify();
    }

    /// Move the focused match by `delta`, wrapping.
    fn search_step(&mut self, delta: i64, cx: &mut Context<Self>) {
        let len = self.search_matches().len() as i64;
        if len == 0 {
            cx.notify();
            return;
        }
        if let Some(s) = self.search.as_mut() {
            s.current = (((s.current as i64 + delta) % len + len) % len) as usize;
        }
        self.search_jump(cx);
    }

    /// Handle a keystroke while the search overlay is open.
    pub(crate) fn search_key(&mut self, ks: &gpui::Keystroke, mods: input::Mods, cx: &mut Context<Self>) {
        if mods.cmd {
            // Clipboard + select-all for the search field.
            match ks.key.as_str() {
                "v" => {
                    let pasted = cx.read_from_clipboard().and_then(|i| i.text());
                    if let (Some(t), Some(s)) = (pasted, self.search.as_mut()) {
                        s.edit.insert(&t.replace(['\n', '\r'], " "));
                        s.current = 0;
                        self.search_jump(cx);
                    }
                }
                "a" => {
                    if let Some(s) = self.search.as_mut() {
                        s.edit.select_all();
                        cx.notify();
                    }
                }
                "c" => {
                    if let Some(t) = self.search.as_ref().and_then(|s| s.edit.selected_text()) {
                        cx.write_to_clipboard(gpui::ClipboardItem::new_string(t));
                    }
                }
                "x" => {
                    if let Some(s) = self.search.as_mut() {
                        if let Some(t) = s.edit.selected_text() {
                            cx.write_to_clipboard(gpui::ClipboardItem::new_string(t));
                            s.edit.delete_selection();
                            s.current = 0;
                            self.search_jump(cx);
                        }
                    }
                }
                _ => {}
            }
            return;
        }
        match ks.key.as_str() {
            "escape" => {
                self.search = None;
                cx.notify();
            }
            "enter" | "down" => self.search_step(1, cx),
            "up" => self.search_step(-1, cx),
            "left" => {
                if let Some(s) = self.search.as_mut() {
                    s.edit.left();
                }
                cx.notify();
            }
            "right" => {
                if let Some(s) = self.search.as_mut() {
                    s.edit.right();
                }
                cx.notify();
            }
            "home" => {
                if let Some(s) = self.search.as_mut() {
                    s.edit.home();
                }
                cx.notify();
            }
            "end" => {
                if let Some(s) = self.search.as_mut() {
                    s.edit.end();
                }
                cx.notify();
            }
            "backspace" | "delete" => {
                if let Some(s) = self.search.as_mut() {
                    if ks.key == "backspace" {
                        s.edit.backspace();
                    } else {
                        s.edit.delete();
                    }
                    s.current = 0;
                }
                self.search_jump(cx);
            }
            _ => {
                let text = ks
                    .key_char
                    .as_deref()
                    .filter(|t| !t.is_empty() && !mods.ctrl && !mods.alt);
                if let Some(text) = text {
                    if let Some(s) = self.search.as_mut() {
                        s.edit.insert(text);
                        s.current = 0;
                    }
                    self.search_jump(cx);
                }
            }
        }
    }

    /// The find bar, pinned full-width to the top of the pane (right below the
    /// tab): a query field with a caret, the match count, and
    /// clickable previous / next / close controls.
    pub(crate) fn search_bar(
        &self,
        before: &str,
        selected: Option<&str>,
        after: &str,
        pos: usize,
        total: usize,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let fg = colors::rgba(self.colors.selection_fg);
        let bg = colors::rgba(self.colors.selection_bg);
        let mut caret = colors::hsla(self.colors.cursor);
        caret.a = 0.9;
        let mut sel_bg = colors::hsla(self.colors.cursor);
        sel_bg.a = 0.35;
        let mut hover = colors::hsla(self.colors.selection_fg);
        hover.a = 0.16;
        let mut border = colors::hsla(self.colors.fg);
        border.a = 0.18;
        let mut dim = colors::hsla(self.colors.selection_fg);
        dim.a = 0.7;

        let button =
            |id: &'static str, glyph: &'static str, delta: Option<i64>| {
                div()
                    .id(id)
                    .flex()
                    .items_center()
                    .justify_center()
                    .w(px(22.0))
                    .h(px(22.0))
                    .rounded(px(4.0))
                    .hover(move |s| s.bg(hover))
                    .child(SharedString::from(glyph))
                    .on_click(cx.listener(move |this, _: &gpui::ClickEvent, _w, cx| {
                        cx.stop_propagation();
                        match delta {
                            Some(d) => this.search_step(d, cx),
                            None => {
                                this.search = None;
                                cx.notify();
                            }
                        }
                    }))
            };

        div()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .px_2()
            .py_1()
            .bg(bg)
            .border_b_1()
            .border_color(border)
            .text_color(fg)
            .text_size(self.font_size)
            .child(SharedString::from("\u{2315}"))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_row()
                    .items_center()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(SharedString::from(before.to_string()))
                    .map(|row| match selected {
                        Some(sel) => row.child(
                            div()
                                .bg(sel_bg)
                                .rounded(px(2.0))
                                .child(SharedString::from(sel.to_string())),
                        ),
                        None => row.child(div().w(px(1.0)).h(px(14.0)).bg(caret)),
                    })
                    .child(SharedString::from(after.to_string())),
            )
            .child(
                div()
                    .flex_none()
                    .text_color(dim)
                    .child(SharedString::from(format!("{pos}/{total}"))),
            )
            .child(button("search-prev", "\u{2039}", Some(-1)))
            .child(button("search-next", "\u{203a}", Some(1)))
            .child(button("search-close", "\u{00d7}", None))
    }
}
