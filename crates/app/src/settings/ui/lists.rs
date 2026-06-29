use super::*;
use super::super::model::{ListKind, Section};
use super::super::{EditTarget, SettingsView};
use gpui::{div, px, AnyElement, Context, MouseButton, SharedString};

impl SettingsView {
    fn list_rows(&self, kind: ListKind, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let width = match kind {
            ListKind::Keybind | ListKind::AgentTool => 420.0,
            ListKind::Plugin => 380.0,
            _ => 320.0,
        };
        let entries = kind.values(&self.opts);
        let mut rows: Vec<AnyElement> = entries
            .iter()
            .enumerate()
            .map(|(i, val)| self.entry_row(kind, i, val.clone(), width, cx).into_any_element())
            .collect();
        rows.push(self.add_row(kind, width, cx).into_any_element());
        rows
    }

    fn entry_row(
        &self,
        kind: ListKind,
        idx: usize,
        value: String,
        width: f32,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let input = self.text_input(EditTarget::Item(kind, idx), value, kind.placeholder(), width, cx);
        let mut row = div()
            .h(px(44.0))
            .px_3()
            .flex()
            .items_center()
            .gap_2()
            .justify_between()
            .child(input);
        if kind == ListKind::Keybind {
            row = row.child(
                button_box("\u{2328}")
                    .text_color(hsla(BLUE_TEXT))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _ev, window, cx| {
                            this.record_item(kind, idx, window, cx);
                            cx.stop_propagation();
                        }),
                    ),
            );
        }
        row.child(
                button_box("\u{2715}")
                    .text_color(hsla(MUTED))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _ev, _window, cx| {
                            this.remove_item(kind, idx, cx);
                            cx.stop_propagation();
                        }),
                    ),
            )
    }

    fn add_row(&self, kind: ListKind, width: f32, cx: &mut Context<Self>) -> impl IntoElement {
        let editing_new = matches!(
            self.editing.as_ref().map(|(t, _)| t),
            Some(EditTarget::NewItem(k)) if *k == kind
        );
        let mut row = div()
            .h(px(44.0))
            .px_3()
            .flex()
            .items_center()
            .gap_2();
        if editing_new {
            row = row.child(self.text_input(
                EditTarget::NewItem(kind),
                String::new(),
                kind.placeholder(),
                width,
                cx,
            ));
        } else {
            row = row.child(
                button_box(SharedString::from(format!("+  {}", kind.add_label())))
                    .px_3()
                    .text_color(hsla(BLUE_TEXT))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _ev, window, cx| {
                            this.start_new_item(kind, window, cx);
                            cx.stop_propagation();
                        }),
                    ),
            );
        }
        row
    }

    pub(crate) fn list_group(&self, kind: ListKind, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .child(self.heading(kind.label()))
            .child(self.list(self.list_rows(kind, cx)))
    }

    /// The Keybindings list plus a syntax hint and a Restore-defaults button.
    pub(crate) fn keyboard_group(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let hint = div()
            .px_1()
            .pb_1()
            .text_color(hsla(MUTED))
            .child(SharedString::from(
                "trigger=action, e.g. cmd+shift+t=new_tab. Chain keys with > for a chord \
                 (ctrl+a>n=new_tab). Use =unbind to remove one.",
            ));
        let header = div()
            .flex()
            .items_center()
            .justify_between()
            .child(self.heading(ListKind::Keybind.label()))
            .child(
                button_box("\u{21ba}  Restore defaults")
                    .px_3()
                    .text_color(hsla(MUTED))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _ev, _window, cx| {
                            this.reset_keybinds(cx);
                            cx.stop_propagation();
                        }),
                    ),
            );
        div()
            .flex()
            .flex_col()
            .child(header)
            .child(hint)
            .child(self.list(self.list_rows(ListKind::Keybind, cx)))
    }

    pub(crate) fn macros_group(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut rows: Vec<AnyElement> = Vec::new();
        let names: Vec<String> = self.macros.iter().map(|m| m.name.clone()).collect();
        if names.is_empty() {
            rows.push(self.macro_empty_row());
        } else {
            for name in &names {
                rows.push(self.macro_row(name, cx));
            }
        }
        div()
            .flex()
            .flex_col()
            .child(self.heading("Macros"))
            .child(self.list(rows))
    }

    fn macro_empty_row(&self) -> AnyElement {
        self.row(
            self.icon("\u{25b6}", Section::Macros.accent(), px(22.0)),
            "No macros recorded yet",
            div()
                .text_color(hsla(MUTED))
                .child(SharedString::from("Record one, then assign a shortcut here")),
        )
        .into_any_element()
    }

    /// One macro: its name, its assigned shortcut (or capture prompt), a button
    /// to (re)capture the shortcut, a clear button, and a delete button.
    fn macro_row(&self, name: &str, cx: &mut Context<Self>) -> AnyElement {
        let capturing = self.capture_macro.as_deref() == Some(name);
        let shortcut = self.macro_shortcut(name);
        let (text, color) = if capturing {
            ("Press keys\u{2026}".to_string(), BLUE_TEXT)
        } else if let Some(s) = &shortcut {
            (s.clone(), TEXT)
        } else {
            ("Not set".to_string(), MUTED)
        };
        let label = div()
            .min_w(px(110.0))
            .flex()
            .justify_end()
            .text_color(hsla(color))
            .child(SharedString::from(text));

        let for_record = name.to_string();
        let record = button_box("\u{2328}").text_color(hsla(BLUE_TEXT)).on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _ev, window, cx| {
                this.start_macro_capture(for_record.clone(), window, cx);
                cx.stop_propagation();
            }),
        );

        let mut control = div().flex().items_center().gap_2().child(label).child(record);
        if shortcut.is_some() {
            let for_clear = name.to_string();
            control = control.child(
                button_box("\u{21ba}").text_color(hsla(MUTED)).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _ev, _window, cx| {
                        this.clear_macro_shortcut(&for_clear, cx);
                        cx.stop_propagation();
                    }),
                ),
            );
        }
        let for_delete = name.to_string();
        control = control.child(
            button_box("\u{2715}").text_color(hsla(MUTED)).on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _ev, _window, cx| {
                    this.delete_macro(&for_delete, cx);
                    cx.stop_propagation();
                }),
            ),
        );

        self.row(
            self.icon("\u{25b6}", Section::Macros.accent(), px(22.0)),
            name,
            control,
        )
        .into_any_element()
    }
}
