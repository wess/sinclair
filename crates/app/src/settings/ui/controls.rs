use super::*;
use super::super::model::{Bool, Choice, Num};
use super::super::{EditTarget, SettingsView};
use gpui::{div, px, AnyElement, Context, MouseButton, SharedString};

impl SettingsView {
    pub(crate) fn icon(&self, glyph: &str, color: theme::Rgb, size: gpui::Pixels) -> impl IntoElement {
        div()
            .w(size)
            .h(size)
            .rounded(px(5.0))
            .bg(hsla(color))
            .text_color(hsla(TEXT))
            .flex()
            .items_center()
            .justify_center()
            .child(SharedString::from(glyph.to_string()))
    }

    pub(crate) fn row(
        &self,
        icon: impl IntoElement,
        label: &str,
        control: impl IntoElement,
    ) -> impl IntoElement {
        div()
            .w_full()
            .h(px(52.0))
            .px_3()
            .flex()
            .items_center()
            .justify_between()
            .gap_3()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .flex_none()
                    .child(icon)
                    .child(SharedString::from(label.to_string())),
            )
            .child(control)
    }

    pub(crate) fn list(&self, rows: Vec<AnyElement>) -> impl IntoElement {
        let mut list = div().w_full().flex().flex_col().rounded(px(10.0)).bg(hsla(PANEL));
        for (i, row) in rows.into_iter().enumerate() {
            if i > 0 {
                list = list.child(div().mx_3().h(px(1.0)).bg(hsla(LINE)));
            }
            list = list.child(row);
        }
        list
    }

    pub(crate) fn heading(&self, text: &str) -> impl IntoElement {
        div()
            .pt_4()
            .pb_1()
            .px_1()
            .text_color(hsla(MUTED))
            .child(SharedString::from(text.to_string()))
    }

    /// A bordered text field bound to `target`. When that target is the one
    /// being edited it shows a live caret; otherwise it shows the value (or a
    /// muted placeholder) and starts editing on click.
    pub(crate) fn text_input(
        &self,
        target: EditTarget,
        value: String,
        placeholder: &str,
        width: f32,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let active = self.editing.as_ref().map(|(t, _)| t) == Some(&target);
        let mut border = hsla(if active { BLUE } else { FIELD_BORDER });
        border.a = if active { 1.0 } else { 0.75 };
        // Flexible width: grow to `width` but shrink below it when the row is
        // narrow, so long values never overflow the content column.
        let mut field = div()
            .flex_1()
            .min_w(px(0.0))
            .max_w(px(width))
            .h(px(26.0))
            .px_2()
            .rounded(px(6.0))
            .border_1()
            .border_color(border)
            .bg(hsla(FIELD_BG))
            .flex()
            .items_center()
            .overflow_hidden();
        if active && self.capturing {
            field = field
                .text_color(hsla(BLUE_TEXT))
                .child(SharedString::from("Press keys\u{2026}"));
        } else if let Some((_, edit)) = self.editing.as_ref().filter(|_| active) {
            field = field.text_color(hsla(TEXT));
            if let Some((before, selected, after)) = edit.split_selection() {
                let mut sel_bg = hsla(BLUE);
                sel_bg.a = 0.35;
                field = field
                    .child(SharedString::from(before))
                    .child(
                        div()
                            .bg(sel_bg)
                            .rounded(px(2.0))
                            .child(SharedString::from(selected)),
                    )
                    .child(SharedString::from(after));
            } else {
                let (before, after) = edit.split();
                field = field
                    .child(SharedString::from(before))
                    .child(div().w(px(1.0)).h(px(16.0)).bg(hsla(TEXT)))
                    .child(SharedString::from(after));
            }
        } else {
            let empty = value.is_empty();
            field = field
                .text_color(hsla(if empty { MUTED } else { TEXT }))
                .child(SharedString::from(if empty {
                    placeholder.to_string()
                } else {
                    value
                }))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _ev, window, cx| {
                        this.begin_edit(target.clone(), window, cx);
                        cx.stop_propagation();
                    }),
                );
        }
        field
    }

    pub(crate) fn switch(&self, b: Bool, cx: &mut Context<Self>) -> impl IntoElement {
        let on = b.get(&self.opts);
        let knob_x = if on { px(19.0) } else { px(2.0) };
        div()
            .w(px(45.0))
            .h(px(26.0))
            .rounded(px(13.0))
            .bg(hsla(if on { BLUE } else { FIELD_BG }))
            .relative()
            .child(
                div()
                    .absolute()
                    .left(knob_x)
                    .top(px(2.0))
                    .w(px(22.0))
                    .h(px(22.0))
                    .rounded(px(11.0))
                    .bg(hsla(theme::Rgb::new(255, 255, 255))),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _ev, _window, cx| {
                    this.toggle(b, cx);
                    cx.stop_propagation();
                }),
            )
    }

    pub(crate) fn stepper(&self, n: Num, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .gap_1()
            .child(self.step_button("\u{2212}", n, -1, cx))
            .child(
                div()
                    .w(px(76.0))
                    .flex()
                    .justify_center()
                    .text_color(hsla(TEXT))
                    .child(SharedString::from(n.display(&self.opts))),
            )
            .child(self.step_button("+", n, 1, cx))
    }

    fn step_button(&self, glyph: &str, n: Num, dir: i32, cx: &mut Context<Self>) -> impl IntoElement {
        button_box(glyph).on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _ev, _window, cx| {
                this.step(n, dir, cx);
                cx.stop_propagation();
            }),
        )
    }

    pub(crate) fn cycle_control(&self, c: Choice, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .gap_1()
            .child(self.cycle_button("\u{2039}", c, -1, cx))
            .child(
                div()
                    .w(px(132.0))
                    .flex()
                    .justify_center()
                    .text_color(hsla(TEXT))
                    .child(SharedString::from(c.display(&self.opts))),
            )
            .child(self.cycle_button("\u{203a}", c, 1, cx))
    }

    fn cycle_button(
        &self,
        glyph: &str,
        c: Choice,
        dir: i32,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        button_box(glyph).on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _ev, _window, cx| {
                this.cycle(c, dir, cx);
                cx.stop_propagation();
            }),
        )
    }
}
