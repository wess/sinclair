//! Rendering for the settings window: the sidebar, hero, reusable controls,
//! and the per-section row builders.

use gpui::prelude::*;
use gpui::{div, px, AnyElement, Div, MouseButton, SharedString, Window, WindowControlArea};
use gpui::{Context, Hsla};

use super::model::{ListKind, Section};
use super::SettingsView;
use crate::colors;

mod ai;
mod controls;
mod lists;
mod rows;

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
            Section::Terminal => vec![
                self.list(self.terminal_rows(cx)).into_any_element(),
                self.list_group(ListKind::Redact, cx).into_any_element(),
                self.list_group(ListKind::Trigger, cx).into_any_element(),
                self.list_group(ListKind::Snippet, cx).into_any_element(),
            ],
            Section::Keyboard => vec![self.keyboard_group(cx).into_any_element()],
            Section::Macros => vec![self.macros_group(cx).into_any_element()],
            Section::Plugins => vec![self.list_group(ListKind::Plugin, cx).into_any_element()],
            Section::Ai => vec![
                self.list(self.ai_rows(cx)).into_any_element(),
                self.tools_group(cx).into_any_element(),
                self.list_group(ListKind::AgentTool, cx).into_any_element(),
            ],
        }
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
