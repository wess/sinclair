use super::content::{Article, Block, ARTICLES};
use super::*;
use gpui::{div, px, Context, FontWeight, MouseButton, SharedString};

impl HelpView {
    fn icon(&self, glyph: &str, color: theme::Rgb, sz: gpui::Pixels) -> impl IntoElement {
        div()
            .w(sz)
            .h(sz)
            .rounded(px(5.0))
            .bg(hsla(color))
            .text_color(hsla(TEXT))
            .flex()
            .items_center()
            .justify_center()
            .child(SharedString::from(glyph.to_string()))
    }

    fn search(&self) -> impl IntoElement {
        let (before, after) = self.query.split();
        let empty = self.query.text().is_empty();
        let mut placeholder = hsla(TEXT);
        placeholder.a = 0.64;
        let mut row = div()
            .h(px(28.0))
            .mb_3()
            .rounded(px(14.0))
            .px_3()
            .flex()
            .items_center()
            .gap_2()
            .bg(hsla(SEARCH_BG))
            .text_color(if empty { placeholder } else { hsla(TEXT) })
            .child(SharedString::from("\u{2315}"));
        if empty {
            row = row.child(SharedString::from("Search help"));
        } else {
            row = row
                .child(SharedString::from(before))
                .child(div().w(px(1.0)).h(px(15.0)).bg(hsla(TEXT)))
                .child(SharedString::from(after));
        }
        row
    }

    fn sidebar_item(&self, index: usize, selected: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let article = &ARTICLES[index];
        let mut bg = hsla(if selected { BLUE } else { SIDEBAR_BG });
        bg.a = if selected { 1.0 } else { 0.0 };
        div()
            .flex()
            .items_center()
            .gap_2()
            .h(px(32.0))
            .px_2()
            .rounded(px(7.0))
            .bg(bg)
            .text_color(hsla(TEXT))
            .child(self.icon(article.icon, article.accent, px(20.0)))
            .child(SharedString::from(article.title))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _ev, _window, cx| {
                    this.selected = index;
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
    }

    pub(crate) fn sidebar(&self, visible: &[usize], current: Option<usize>, cx: &mut Context<Self>) -> impl IntoElement {
        let mut col = div()
            .w(px(SIDEBAR))
            .h_full()
            .px_3()
            .pt(px(58.0))
            .pb_3()
            .bg(hsla(SIDEBAR_BG))
            .child(self.search());
        if visible.is_empty() {
            col = col.child(
                div()
                    .px_2()
                    .py_2()
                    .text_color(hsla(MUTED))
                    .child(SharedString::from("No matching topics")),
            );
        }
        for &index in visible {
            col = col.child(self.sidebar_item(index, current == Some(index), cx));
        }
        col
    }

    /// Left-aligned article header: icon, title, summary, and a hairline rule.
    fn header(&self, article: &Article) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .pb_4()
            .mb_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(self.icon(article.icon, article.accent, px(34.0)))
                    .child(
                        div()
                            .text_size(px(24.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(hsla(TEXT))
                            .child(SharedString::from(article.title)),
                    ),
            )
            .child(
                div()
                    .pt_2()
                    .text_color(hsla(MUTED))
                    .line_height(px(21.0))
                    .child(SharedString::from(article.summary)),
            )
            .child(div().mt_4().h(px(1.0)).bg(hsla(LINE)))
    }

    fn heading(&self, text: &str) -> gpui::AnyElement {
        div()
            .pt_5()
            .pb_2()
            .text_size(px(12.0))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(hsla(MUTED))
            .child(SharedString::from(text.to_uppercase()))
            .into_any_element()
    }

    fn paragraph(&self, text: &str) -> gpui::AnyElement {
        div()
            .pb_2()
            .text_color(hsla(BODY))
            .line_height(px(22.0))
            .child(SharedString::from(text.to_string()))
            .into_any_element()
    }

    fn bullet(&self, text: &str) -> gpui::AnyElement {
        div()
            .flex()
            .gap_2()
            .pb_1()
            .text_color(hsla(BODY))
            .line_height(px(22.0))
            .child(div().w(px(12.0)).flex_none().child(SharedString::from("\u{2022}")))
            .child(div().flex_1().child(SharedString::from(text.to_string())))
            .into_any_element()
    }

    /// A run of keybindings rendered as one rounded panel with separators,
    /// matching the Settings list style.
    fn keylist(&self, keys: &[(&str, &str)]) -> gpui::AnyElement {
        let mut panel = div().mt_1().flex().flex_col().rounded(px(10.0)).bg(hsla(PANEL));
        for (i, (label, shortcut)) in keys.iter().enumerate() {
            if i > 0 {
                panel = panel.child(div().mx_3().h(px(1.0)).bg(hsla(LINE)));
            }
            panel = panel.child(
                div()
                    .h(px(44.0))
                    .px_3()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().text_color(hsla(TEXT)).child(SharedString::from(*label)))
                    .child(
                        div()
                            .px_2()
                            .py(px(3.0))
                            .rounded(px(6.0))
                            .border_1()
                            .border_color(hsla(FIELD_BORDER))
                            .bg(hsla(FIELD_BG))
                            .text_size(px(13.0))
                            .text_color(hsla(TEXT))
                            .child(SharedString::from(*shortcut)),
                    ),
            );
        }
        panel.into_any_element()
    }

    pub(crate) fn content(&self, article: &Article) -> impl IntoElement {
        let mut body = div().flex().flex_col().min_w(px(0.0));
        let blocks = article.blocks;
        let mut i = 0;
        while i < blocks.len() {
            match &blocks[i] {
                Block::Key(..) => {
                    let mut keys = Vec::new();
                    while let Some(Block::Key(label, shortcut)) = blocks.get(i) {
                        keys.push((*label, *shortcut));
                        i += 1;
                    }
                    body = body.child(self.keylist(&keys));
                }
                Block::Heading(text) => {
                    body = body.child(self.heading(text));
                    i += 1;
                }
                Block::Para(text) => {
                    body = body.child(self.paragraph(text));
                    i += 1;
                }
                Block::Bullet(text) => {
                    body = body.child(self.bullet(text));
                    i += 1;
                }
            }
        }
        div()
            .id("help-content")
            .flex_1()
            .min_w(px(0.0))
            .h_full()
            .px(px(40.0))
            .pt(px(56.0))
            .pb(px(40.0))
            .bg(hsla(CONTENT_BG))
            .overflow_y_scroll()
            .child(self.header(article))
            .child(body)
    }
}
