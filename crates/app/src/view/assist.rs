use super::*;

/// How many trailing buffer rows the assist overlays rank. Bounds the text
/// materialized under the terminal lock on each rescan; line numbers stay
/// global so jumping to a hit still lands on the right row.
const ASSIST_SCAN_ROWS: usize = 1000;

impl TerminalView {
    /// The most recent [`ASSIST_SCAN_ROWS`] rows (scrollback + live grid) as
    /// assist lines, numbered in global-row space.
    fn lines(&self) -> Vec<::assist::Line> {
        self.session.with_term(|term| {
            let sb_len = term.grid().scrollback().len();
            let total = sb_len + term.grid().rows();
            let skip = total.saturating_sub(ASSIST_SCAN_ROWS);
            let mut out = Vec::with_capacity(total - skip);
            for i in skip..sb_len {
                if let Some(row) = term.grid_mut().scrollback_mut().row(i) {
                    out.push(::assist::Line {
                        number: i,
                        text: row.text(),
                        prompt: row.prompt,
                    });
                }
            }
            for i in sb_len.max(skip)..total {
                let row = term.grid().row(i - sb_len);
                out.push(::assist::Line {
                    number: i,
                    text: row.text(),
                    prompt: row.prompt,
                });
            }
            out
        })
    }

    /// Current semantic-search hits, cached until the query changes or new
    /// output marks them stale: the full-buffer scan and its per-line String
    /// allocations never run on an idle repaint.
    pub(crate) fn semantic_hits(&mut self) -> Vec<::assist::context::Hit> {
        let q = match &self.assist {
            Some(Assist::Semantic { edit, .. }) => edit.text(),
            _ => return Vec::new(),
        };
        if let Some(Assist::Semantic {
            cached_query,
            hits,
            dirty,
            ..
        }) = &self.assist
        {
            if !*dirty && cached_query.as_deref() == Some(q.as_str()) {
                return hits.clone();
            }
        }
        let computed = ::assist::search(&q, &self.lines(), 6);
        if let Some(Assist::Semantic {
            cached_query,
            hits,
            dirty,
            ..
        }) = &mut self.assist
        {
            *hits = computed.clone();
            *cached_query = Some(q);
            *dirty = false;
        }
        computed
    }

    pub fn toggle_semantic_search(&mut self, cx: &mut Context<Self>) {
        self.search = None;
        self.assist = match self.assist {
            Some(Assist::Semantic { .. }) => None,
            _ => Some(Assist::Semantic {
                edit: guise::TextEdit::new(""),
                current: 0,
                cached_query: None,
                hits: Vec::new(),
                dirty: true,
            }),
        };
        cx.notify();
    }

    pub fn explain_output(&mut self, cx: &mut Context<Self>) {
        let selected = self.session.with_term(|term| term.selection_text());
        let text = selected
            .filter(|text| !text.trim().is_empty())
            .or_else(|| ::assist::lastblock(&self.lines()).map(|block| block.text))
            .unwrap_or_default();
        self.search = None;
        self.assist = Some(Assist::Message {
            title: "Explain".to_string(),
            body: ::assist::explain(&text),
        });
        cx.notify();
    }

    pub fn compose_command(&mut self, cx: &mut Context<Self>) {
        self.search = None;
        self.assist = Some(Assist::Compose {
            edit: guise::TextEdit::new(""),
            miss: false,
        });
        cx.notify();
    }

    fn semantic_jump(&mut self, cx: &mut Context<Self>) {
        let hits = self.semantic_hits();
        let Some(Assist::Semantic { current, .. }) = self.assist.as_mut() else {
            return;
        };
        if hits.is_empty() {
            cx.notify();
            return;
        }
        *current = (*current).min(hits.len() - 1);
        let line = hits[*current].block.start;
        self.session.with_term(|term| {
            let sb = term.grid().scrollback().len();
            term.set_display_offset(sb.saturating_sub(line));
        });
        cx.notify();
    }

    fn semantic_step(&mut self, delta: i64, cx: &mut Context<Self>) {
        let len = self.semantic_hits().len() as i64;
        if len == 0 {
            cx.notify();
            return;
        }
        if let Some(Assist::Semantic { current, .. }) = self.assist.as_mut() {
            *current = (((*current as i64 + delta) % len + len) % len) as usize;
        }
        self.semantic_jump(cx);
    }

    pub(crate) fn assist_key(&mut self, ks: &gpui::Keystroke, mods: input::Mods, cx: &mut Context<Self>) {
        if mods.cmd || mods.ctrl {
            return;
        }
        let Some(assist) = self.assist.as_mut() else {
            return;
        };
        match assist {
            Assist::Semantic { edit, current, .. } => match ks.key.as_str() {
                "escape" => self.assist = None,
                "enter" => self.semantic_jump(cx),
                "down" => self.semantic_step(1, cx),
                "up" => self.semantic_step(-1, cx),
                "left" => edit.left(),
                "right" => edit.right(),
                "home" => edit.home(),
                "end" => edit.end(),
                "backspace" => {
                    edit.backspace();
                    *current = 0;
                }
                "delete" => {
                    edit.delete();
                    *current = 0;
                }
                _ => {
                    if let Some(text) = ks
                        .key_char
                        .as_deref()
                        .filter(|t| !t.is_empty() && !mods.alt)
                    {
                        edit.insert(text);
                        *current = 0;
                    }
                }
            },
            Assist::Compose { edit, miss } => match ks.key.as_str() {
                "escape" => self.assist = None,
                "enter" => {
                    // A miss keeps the panel open with a "no match" note
                    // instead of echoing the request into the shell.
                    match ::assist::compose_match(&edit.text()) {
                        Some(command) if !command.trim().is_empty() => {
                            self.scroll_to_bottom(cx);
                            let _ = self.session.write(command.as_bytes());
                            self.assist = None;
                        }
                        _ => *miss = true,
                    }
                }
                "left" => edit.left(),
                "right" => edit.right(),
                "home" => edit.home(),
                "end" => edit.end(),
                "backspace" => {
                    edit.backspace();
                    *miss = false;
                }
                "delete" => {
                    edit.delete();
                    *miss = false;
                }
                _ => {
                    if let Some(text) = ks
                        .key_char
                        .as_deref()
                        .filter(|t| !t.is_empty() && !mods.alt)
                    {
                        edit.insert(text);
                        *miss = false;
                    }
                }
            },
            Assist::Message { .. } => {
                if matches!(ks.key.as_str(), "escape" | "enter") {
                    self.assist = None;
                }
            }
            Assist::Paste { text, .. } => match ks.key.as_str() {
                "escape" | "n" => self.assist = None,
                "enter" | "y" => {
                    let text = text.clone();
                    self.assist = None;
                    self.write_paste(&text, cx);
                }
                _ => {}
            },
            Assist::ClipboardWrite { text, primary } => match ks.key.as_str() {
                "escape" | "n" => self.assist = None,
                "enter" | "y" => {
                    let (text, primary) = (text.clone(), *primary);
                    self.assist = None;
                    self.write_clipboard(text, primary, cx);
                }
                _ => {}
            },
        }
        cx.notify();
    }

    fn input_line(&self, label: &str, edit: &guise::TextEdit) -> impl IntoElement {
        let (before, after) = edit.split();
        let mut caret = colors::hsla(self.colors.cursor);
        caret.a = 0.9;
        div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .text_color(colors::rgba(self.colors.selection_fg))
                    .child(label.to_string()),
            )
            .child(SharedString::from(before))
            .child(div().w(px(1.0)).h(px(14.0)).bg(caret))
            .child(SharedString::from(after))
    }

    pub(crate) fn assist_panel(&self, hits: &[::assist::context::Hit]) -> Option<AnyElement> {
        let assist = self.assist.as_ref()?;
        let mut border = colors::hsla(self.colors.fg);
        border.a = 0.24;
        let panel = div()
            .absolute()
            .bottom(px(8.0))
            .left(px(8.0))
            .max_w(px(620.0))
            // Occlude the grid: a press on the panel must not fall through
            // and start a selection at the cell beneath it.
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .bg(colors::rgba(self.colors.bg))
            .border_1()
            .border_color(border)
            .p_3()
            .flex()
            .flex_col()
            .gap_2()
            .text_size(self.font_size)
            .text_color(colors::rgba(self.colors.fg));
        let panel = match assist {
            Assist::Semantic { edit, current, .. } => {
                let mut panel = panel.child(self.input_line("Semantic", edit));
                for (i, hit) in hits.iter().enumerate() {
                    let color = if i == *current {
                        colors::rgba(self.colors.selection_fg)
                    } else {
                        colors::rgba(self.colors.fg)
                    };
                    let bg = if i == *current {
                        colors::rgba(self.colors.selection_bg)
                    } else {
                        colors::rgba(self.colors.bg)
                    };
                    let first = hit
                        .block
                        .text
                        .lines()
                        .find(|line| !line.trim().is_empty())
                        .unwrap_or("")
                        .trim()
                        .chars()
                        .take(96)
                        .collect::<String>();
                    panel = panel.child(div().px_1().py_1().bg(bg).text_color(color).child(
                        SharedString::from(format!("{:.0}%  {}", hit.score * 100.0, first)),
                    ));
                }
                panel
            }
            Assist::Compose { edit, miss } => {
                let panel = panel.child(self.input_line("Command", edit));
                if *miss {
                    let mut dim = colors::hsla(self.colors.fg);
                    dim.a = 0.7;
                    panel.child(
                        div()
                            .text_color(dim)
                            .child("No matching command \u{00b7} try rephrasing"),
                    )
                } else {
                    panel
                }
            }
            Assist::Message { title, body } => panel
                .child(
                    div()
                        .text_color(colors::rgba(self.colors.selection_fg))
                        .child(title.clone()),
                )
                .child(SharedString::from(body.clone())),
            Assist::Paste { risk, .. } => {
                let reasons = if risk.reasons.is_empty() {
                    "unknown risk".to_string()
                } else {
                    risk.reasons.join(", ")
                };
                panel
                    .child(
                        div()
                            .text_color(colors::rgba(self.colors.selection_fg))
                            .child(format!("Paste risk: {:?}", risk.level)),
                    )
                    .child(SharedString::from(reasons))
            }
            Assist::ClipboardWrite { .. } => panel
                .child(
                    div()
                        .text_color(colors::rgba(self.colors.selection_fg))
                        .child("Allow clipboard write?"),
                )
                .child(SharedString::from(
                    "A program wants to set the system clipboard.  enter/y allow · esc/n deny",
                )),
        };
        Some(panel.into_any_element())
    }
}
