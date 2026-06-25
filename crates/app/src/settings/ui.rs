//! Rendering for the settings window: the sidebar, hero, reusable controls,
//! and the per-section row builders.

use gpui::prelude::*;
use gpui::{div, px, AnyElement, Div, MouseButton, SharedString, Window, WindowControlArea};
use gpui::{Context, Hsla};

use super::model::{Bool, Choice, Field, ListKind, Num, Section};
use super::{EditTarget, SettingsView, ToolTest};
use crate::colors;

const SIDEBAR: f32 = 226.0;

impl SettingsView {
    fn sidebar_item(&self, section: Section, cx: &mut Context<Self>) -> impl IntoElement {
        let selected = self.section == section;
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
            .child(self.icon(section.icon(), section.accent(), px(20.0)))
            .child(SharedString::from(section.title()))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _ev, _window, cx| {
                    this.set_section(section, cx);
                    cx.stop_propagation();
                }),
            )
    }

    fn sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut bar = div()
            .w(px(SIDEBAR))
            .h_full()
            .px_3()
            .pt(px(58.0))
            .pb_3()
            .bg(hsla(SIDEBAR_BG))
            .child(self.identity());
        for section in Section::ALL {
            bar = bar.child(self.sidebar_item(section, cx));
        }
        bar
    }

    fn identity(&self) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .gap_2()
            .mb_4()
            .child(
                div()
                    .w(px(38.0))
                    .h(px(38.0))
                    .rounded(px(19.0))
                    .bg(hsla(theme::Rgb::new(232, 235, 241)))
                    .text_color(hsla(theme::Rgb::new(97, 103, 112)))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(SharedString::from("P")),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(SharedString::from("Prompt"))
                    .child(
                        div()
                            .text_color(hsla(MUTED))
                            .child(SharedString::from("Preferences")),
                    ),
            )
    }

    fn icon(&self, glyph: &str, color: theme::Rgb, size: gpui::Pixels) -> impl IntoElement {
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

    fn hero(&self) -> impl IntoElement {
        div()
            .h(px(150.0))
            .rounded(px(10.0))
            .bg(hsla(PANEL))
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .child(self.icon(self.section.icon(), self.section.accent(), px(48.0)))
            .child(
                div()
                    .pt_3()
                    .text_size(px(24.0))
                    .text_color(hsla(TEXT))
                    .child(SharedString::from(self.section.title())),
            )
            .child(
                div()
                    .w(px(420.0))
                    .pt_1()
                    .text_color(hsla(MUTED))
                    .child(SharedString::from(self.section.subtitle())),
            )
    }

    // --- Reusable chrome ---------------------------------------------------

    fn row(
        &self,
        icon: impl IntoElement,
        label: &str,
        control: impl IntoElement,
    ) -> impl IntoElement {
        div()
            .h(px(52.0))
            .px_3()
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(icon)
                    .child(SharedString::from(label.to_string())),
            )
            .child(control)
    }

    fn list(&self, rows: Vec<AnyElement>) -> impl IntoElement {
        let mut list = div().flex().flex_col().rounded(px(10.0)).bg(hsla(PANEL));
        for (i, row) in rows.into_iter().enumerate() {
            if i > 0 {
                list = list.child(div().mx_3().h(px(1.0)).bg(hsla(LINE)));
            }
            list = list.child(row);
        }
        list
    }

    fn heading(&self, text: &str) -> impl IntoElement {
        div()
            .pt_4()
            .pb_1()
            .px_1()
            .text_color(hsla(MUTED))
            .child(SharedString::from(text.to_string()))
    }

    // --- Controls ----------------------------------------------------------

    /// A bordered text field bound to `target`. When that target is the one
    /// being edited it shows a live caret; otherwise it shows the value (or a
    /// muted placeholder) and starts editing on click.
    fn text_input(
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
        let mut field = div()
            .w(px(width))
            .h(px(26.0))
            .px_2()
            .rounded(px(6.0))
            .border_1()
            .border_color(border)
            .bg(hsla(FIELD_BG))
            .flex()
            .items_center();
        if active && self.capturing {
            field = field
                .text_color(hsla(BLUE_TEXT))
                .child(SharedString::from("Press keys\u{2026}"));
        } else if active {
            let (before, after) = self.editing.as_ref().expect("active").1.split();
            field = field
                .text_color(hsla(TEXT))
                .child(SharedString::from(before))
                .child(div().w(px(1.0)).h(px(16.0)).bg(hsla(TEXT)))
                .child(SharedString::from(after));
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

    fn switch(&self, b: Bool, cx: &mut Context<Self>) -> impl IntoElement {
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

    fn stepper(&self, n: Num, cx: &mut Context<Self>) -> impl IntoElement {
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

    fn cycle_control(&self, c: Choice, cx: &mut Context<Self>) -> impl IntoElement {
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

    // --- Row builders ------------------------------------------------------

    fn toggle_row(&self, b: Bool, glyph: &str, color: theme::Rgb, cx: &mut Context<Self>) -> AnyElement {
        self.row(self.icon(glyph, color, px(22.0)), b.label(), self.switch(b, cx))
            .into_any_element()
    }

    fn stepper_row(&self, n: Num, glyph: &str, color: theme::Rgb, cx: &mut Context<Self>) -> AnyElement {
        self.row(self.icon(glyph, color, px(22.0)), n.label(), self.stepper(n, cx))
            .into_any_element()
    }

    fn cycle_row(&self, c: Choice, glyph: &str, color: theme::Rgb, cx: &mut Context<Self>) -> AnyElement {
        self.row(self.icon(glyph, color, px(22.0)), c.label(), self.cycle_control(c, cx))
            .into_any_element()
    }

    fn field_row(&self, f: Field, glyph: &str, color: theme::Rgb, cx: &mut Context<Self>) -> AnyElement {
        let input = self.text_input(
            EditTarget::Field(f),
            f.value(&self.opts),
            f.placeholder(),
            220.0,
            cx,
        );
        self.row(self.icon(glyph, color, px(22.0)), f.label(), input)
            .into_any_element()
    }

    // --- Editable lists ----------------------------------------------------

    fn list_rows(&self, kind: ListKind, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let width = match kind {
            ListKind::Keybind => 420.0,
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

    fn list_group(&self, kind: ListKind, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .child(self.heading(kind.label()))
            .child(self.list(self.list_rows(kind, cx)))
    }

    // --- Sections ----------------------------------------------------------

    fn content(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut content = div()
            .id("settings-content")
            .flex_1()
            .min_w(px(0.0))
            .h_full()
            .px_5()
            .pt(px(52.0))
            .pb(px(40.0))
            .bg(hsla(CONTENT_BG))
            .overflow_y_scroll()
            .child(self.hero())
            .child(div().h(px(12.0)));
        for group in self.section_groups(cx) {
            content = content.child(group);
        }
        content
    }

    fn section_groups(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        match self.section {
            Section::General => vec![self.list(self.general_rows(cx)).into_any_element()],
            Section::Appearance => vec![
                self.list(self.appearance_rows(cx)).into_any_element(),
                self.list_group(ListKind::FontFamily, cx).into_any_element(),
                self.list_group(ListKind::FontFeature, cx).into_any_element(),
                self.list_group(ListKind::Palette, cx).into_any_element(),
            ],
            Section::Terminal => vec![self.list(self.terminal_rows(cx)).into_any_element()],
            Section::Keyboard => vec![self.list_group(ListKind::Keybind, cx).into_any_element()],
            Section::Macros => vec![self.macros_group(cx).into_any_element()],
            Section::Plugins => vec![self.list_group(ListKind::Plugin, cx).into_any_element()],
            Section::Ai => vec![
                self.list(self.ai_rows(cx)).into_any_element(),
                self.tools_group(cx).into_any_element(),
            ],
        }
    }

    fn macros_group(&self, cx: &mut Context<Self>) -> impl IntoElement {
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

    fn ai_rows(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let a = Section::Ai.accent();
        let mut rows = vec![
            self.toggle_row(Bool::AiEnabled, "\u{2728}", a, cx),
            self.toggle_row(Bool::McpServer, "M", theme::Rgb::new(10, 132, 255), cx),
            self.toggle_row(Bool::RelayEnabled, "R", theme::Rgb::new(52, 199, 89), cx),
            self.toggle_row(
                Bool::RelayStartOnLaunch,
                "\u{21aa}",
                theme::Rgb::new(255, 159, 10),
                cx,
            ),
            self.field_row(Field::RelayAddress, "@", theme::Rgb::new(90, 200, 250), cx),
            self.field_row(
                Field::RelayDefaultAgent,
                "\u{2318}",
                theme::Rgb::new(94, 92, 230),
                cx,
            ),
        ];
        if self.opts.relay_enabled {
            rows.push(self.relay_status_row());
            rows.push(self.relay_log_row());
        }
        rows
    }

    /// A live green/red dot for whether the relay server is listening.
    fn relay_status_row(&self) -> AnyElement {
        let running = self.relay_running;
        let color = if running {
            theme::Rgb::new(52, 199, 89)
        } else {
            theme::Rgb::new(255, 69, 58)
        };
        let dot = div()
            .w(px(22.0))
            .h(px(22.0))
            .flex()
            .items_center()
            .justify_center()
            .child(div().w(px(10.0)).h(px(10.0)).rounded(px(5.0)).bg(hsla(color)));
        let status = div()
            .text_color(hsla(color))
            .child(SharedString::from(if running { "Running" } else { "Stopped" }));
        self.row(dot, "Relay server", status).into_any_element()
    }

    /// The "Agent tools" group: each known tool with a Test button + toggle.
    fn tools_group(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .child(self.heading("Agent tools"))
            .child(self.list(vec![
                self.tool_row(Bool::ToolClaude, "claude", cx),
                self.tool_row(Bool::ToolCodex, "codex", cx),
                self.tool_row(Bool::ToolOllama, "ollama", cx),
                self.tool_row(Bool::ToolGemini, "gemini", cx),
            ]))
    }

    fn tool_row(&self, b: Bool, tool: &'static str, cx: &mut Context<Self>) -> AnyElement {
        let test = button_box("Test").on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _ev, _window, cx| {
                this.test_tool(tool, cx);
                cx.stop_propagation();
            }),
        );
        let control = div()
            .flex()
            .items_center()
            .gap_2()
            .child(self.test_result(tool))
            .child(test)
            .child(self.switch(b, cx));
        self.row(self.icon("\u{25cb}", Section::Ai.accent(), px(22.0)), b.label(), control)
            .into_any_element()
    }

    fn test_result(&self, tool: &str) -> AnyElement {
        match self.tool_tests.get(tool) {
            Some(ToolTest::Testing) => div()
                .text_color(hsla(MUTED))
                .child(SharedString::from("testing…"))
                .into_any_element(),
            Some(ToolTest::Ok(m)) => div()
                .text_color(hsla(theme::Rgb::new(52, 199, 89)))
                .child(SharedString::from(format!("\u{2713} {}", trunc(m, 22))))
                .into_any_element(),
            Some(ToolTest::Fail(e)) => div()
                .text_color(hsla(theme::Rgb::new(255, 69, 58)))
                .child(SharedString::from(format!("\u{2717} {}", trunc(e, 30))))
                .into_any_element(),
            None => div().into_any_element(),
        }
    }

    fn relay_log_row(&self) -> AnyElement {
        let path = crate::relay::log_path().display().to_string();
        let text = div()
            .text_color(hsla(MUTED))
            .child(SharedString::from(path));
        self.row(self.icon("\u{2630}", theme::Rgb::new(142, 142, 147), px(22.0)), "Log", text)
            .into_any_element()
    }

    fn general_rows(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let g = Section::General.accent();
        vec![
            self.field_row(Field::Shell, "\u{2318}", g, cx),
            self.field_row(Field::WorkingDirectory, "\u{1f4c1}", g, cx),
            self.field_row(Field::Title, "\u{24c9}", g, cx),
            self.toggle_row(Bool::InheritCwd, "\u{21aa}", theme::Rgb::new(10, 132, 255), cx),
            self.toggle_row(Bool::QuitLast, "Q", theme::Rgb::new(255, 69, 58), cx),
            self.toggle_row(Bool::ConfirmClose, "!", theme::Rgb::new(255, 159, 10), cx),
            self.toggle_row(Bool::ConfirmQuit, "\u{23fb}", theme::Rgb::new(255, 69, 58), cx),
            self.toggle_row(Bool::PasteProtection, "\u{2335}", theme::Rgb::new(255, 214, 10), cx),
            self.toggle_row(Bool::ShellIntegration, "\u{276f}", theme::Rgb::new(48, 209, 88), cx),
            self.toggle_row(Bool::SessionRestore, "\u{21ba}", theme::Rgb::new(94, 92, 230), cx),
            self.toggle_row(Bool::TabTitleShowHost, "@", theme::Rgb::new(100, 210, 255), cx),
            self.toggle_row(Bool::CopyOnSelect, "\u{2713}", theme::Rgb::new(52, 199, 89), cx),
            self.cycle_row(Choice::OptionAsAlt, "\u{2325}", theme::Rgb::new(88, 86, 214), cx),
            self.cycle_row(Choice::ClipboardRead, "R", theme::Rgb::new(90, 200, 250), cx),
            self.cycle_row(Choice::ClipboardWrite, "W", theme::Rgb::new(94, 92, 230), cx),
        ]
    }

    fn appearance_rows(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        vec![
            self.cycle_row(Choice::Theme, "\u{25d0}", Section::Appearance.accent(), cx),
            self.cycle_row(Choice::FontStyle, "B", theme::Rgb::new(255, 159, 10), cx),
            self.cycle_row(Choice::CursorStyle, "C", theme::Rgb::new(255, 69, 58), cx),
            self.toggle_row(Bool::CursorBlink, "\u{2737}", theme::Rgb::new(255, 214, 10), cx),
            self.field_row(Field::Foreground, "\u{25a0}", theme::Rgb::new(94, 92, 230), cx),
            self.field_row(Field::Background, "\u{25a1}", theme::Rgb::new(99, 99, 102), cx),
            self.field_row(Field::CursorColor, "I", theme::Rgb::new(255, 69, 58), cx),
            self.field_row(Field::CursorText, "T", theme::Rgb::new(255, 149, 0), cx),
            self.field_row(Field::SelectionForeground, "S", theme::Rgb::new(10, 132, 255), cx),
            self.field_row(Field::SelectionBackground, "S", theme::Rgb::new(48, 209, 88), cx),
            self.toggle_row(Bool::BoldIsBright, "\u{2600}", theme::Rgb::new(255, 214, 10), cx),
            self.stepper_row(Num::MinContrast, "\u{25d1}", theme::Rgb::new(142, 142, 147), cx),
        ]
    }

    fn terminal_rows(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let t = Section::Terminal.accent();
        let blue = theme::Rgb::new(90, 200, 250);
        vec![
            self.stepper_row(Num::FontSize, "T", t, cx),
            self.stepper_row(Num::CellWidth, "W", blue, cx),
            self.stepper_row(Num::CellHeight, "H", blue, cx),
            self.stepper_row(Num::PaddingX, "X", blue, cx),
            self.stepper_row(Num::PaddingY, "Y", blue, cx),
            self.stepper_row(Num::WindowWidth, "\u{2194}", theme::Rgb::new(88, 86, 214), cx),
            self.stepper_row(Num::WindowHeight, "\u{2195}", theme::Rgb::new(88, 86, 214), cx),
            self.stepper_row(Num::Scrollback, "\u{2630}", theme::Rgb::new(142, 142, 147), cx),
            self.stepper_row(Num::ScrollMultiplier, "\u{2207}", theme::Rgb::new(255, 159, 10), cx),
            self.toggle_row(Bool::MouseHide, "\u{2196}", theme::Rgb::new(170, 170, 170), cx),
            self.stepper_row(Num::SplitOpacity, "\u{25d0}", theme::Rgb::new(94, 92, 230), cx),
            self.field_row(Field::SplitDivider, "\u{2503}", theme::Rgb::new(99, 99, 102), cx),
        ]
    }
}

impl Render for SettingsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key_down))
            .text_color(hsla(TEXT))
            .bg(hsla(CONTENT_BG))
            .child(self.sidebar(cx))
            .child(self.content(cx))
            .child(drag_strip())
    }
}

/// A drag handle across the transparent titlebar so the window can be moved.
/// Left-inset on macOS to clear the traffic lights.
fn drag_strip() -> impl IntoElement {
    let lead = if cfg!(target_os = "macos") { 78.0 } else { 0.0 };
    div()
        .absolute()
        .top_0()
        .left(px(lead))
        .w(px(super::WIDTH - lead))
        .h(px(30.0))
        .window_control_area(WindowControlArea::Drag)
        .on_mouse_down(MouseButton::Left, |_, window, _| window.start_window_move())
}

/// The shared chrome for a small bordered button (no behavior attached yet).
fn button_box(label: impl Into<SharedString>) -> Div {
    div()
        .h(px(26.0))
        .min_w(px(28.0))
        .px_2()
        .rounded(px(6.0))
        .border_1()
        .border_color(hsla(FIELD_BORDER))
        .bg(hsla(FIELD_BG))
        .flex()
        .items_center()
        .justify_center()
        .text_color(hsla(TEXT))
        .child(label.into())
}

fn hsla(rgb: theme::Rgb) -> Hsla {
    colors::hsla(rgb)
}

/// Truncate to `n` chars with an ellipsis.
fn trunc(s: &str, n: usize) -> String {
    if s.chars().count() > n {
        format!("{}\u{2026}", s.chars().take(n).collect::<String>())
    } else {
        s.to_string()
    }
}

const SIDEBAR_BG: theme::Rgb = theme::Rgb::new(30, 35, 38);
const CONTENT_BG: theme::Rgb = theme::Rgb::new(35, 42, 44);
const PANEL: theme::Rgb = theme::Rgb::new(43, 52, 54);
const FIELD_BG: theme::Rgb = theme::Rgb::new(49, 56, 58);
const FIELD_BORDER: theme::Rgb = theme::Rgb::new(76, 84, 88);
const LINE: theme::Rgb = theme::Rgb::new(61, 70, 73);
const TEXT: theme::Rgb = theme::Rgb::new(242, 244, 246);
const MUTED: theme::Rgb = theme::Rgb::new(170, 177, 181);
const BLUE: theme::Rgb = theme::Rgb::new(10, 102, 220);
const BLUE_TEXT: theme::Rgb = theme::Rgb::new(90, 170, 255);
