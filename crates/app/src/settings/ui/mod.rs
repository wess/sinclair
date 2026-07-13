//! Rendering for the settings window, styled after Zed's settings UI: a
//! navbar with the search field and flat section list, and a content pane
//! with a page title, monospace group labels, and hairline-divided rows.

use gpui::prelude::*;
use gpui::{div, px, Div, FontWeight, MouseButton, SharedString, Window, WindowControlArea};
use gpui::{Context, Hsla};

use super::schema::Section;
use super::SettingsView;
use crate::colors;

mod ai;
mod controls;
mod lists;
mod rows;

const SIDEBAR: f32 = 226.0;

impl SettingsView {
    fn sidebar_item(&self, section: Section, cx: &mut Context<Self>) -> impl IntoElement {
        let selected = self.section == section && self.search().is_empty();
        let mut bg = hsla(NAV_SELECTED);
        bg.a = if selected { 1.0 } else { 0.0 };
        div()
            .flex()
            .items_center()
            .gap_2()
            .h(px(30.0))
            .px_2()
            .rounded(px(6.0))
            .bg(bg)
            .text_color(hsla(if selected { TEXT } else { NAV_TEXT }))
            .child(
                div()
                    .w(px(10.0))
                    .text_size(px(12.0))
                    .text_color(hsla(MUTED))
                    .child(SharedString::from("\u{203a}")),
            )
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
            .flex()
            .flex_col()
            .w(px(SIDEBAR))
            .flex_none()
            .h_full()
            .px_3()
            .pt(px(44.0))
            .pb_3()
            .gap(px(1.0))
            .bg(hsla(SIDEBAR_BG))
            .border_r_1()
            .border_color(hsla(LINE))
            .child(self.search_bar(cx));
        for section in Section::ALL {
            bar = bar.child(self.sidebar_item(section, cx));
        }
        bar.child(div().flex_1()).child(self.sidebar_hint())
    }

    /// The navbar footer hint: the search box is always live.
    fn sidebar_hint(&self) -> impl IntoElement {
        div()
            .px_2()
            .text_size(px(12.0))
            .italic()
            .font_family(SharedString::from(self.opts.primary_font().to_string()))
            .text_color(hsla(MUTED))
            .child(SharedString::from("Type to search"))
    }

    /// The always-live search box. It has no click-to-focus state: whenever
    /// no field editor is active, typing lands here.
    fn search_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let idle = self.editing.is_some();
        let text = self.query.text();
        let mut field = div()
            .flex_1()
            .h(px(30.0))
            .px_2()
            .rounded(px(6.0))
            .border_1()
            .border_color(hsla(FIELD_BORDER))
            .bg(hsla(CONTENT_BG))
            .flex()
            .items_center()
            .gap_1()
            .overflow_hidden()
            .child(
                div()
                    .text_color(hsla(MUTED))
                    .child(SharedString::from("\u{2315}")),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _ev, window, cx| {
                    // Commit any field edit; keys then flow to the search box.
                    this.commit_edit(cx);
                    window.focus(&this.focus, cx);
                    cx.stop_propagation();
                }),
            );
        if text.is_empty() && idle {
            field = field
                .text_color(hsla(MUTED))
                .child(SharedString::from("Search settings\u{2026}"));
        } else if idle {
            field = field.text_color(hsla(TEXT)).child(SharedString::from(text));
        } else {
            // Live: draw the caret (and selection) like the field editors do.
            field = field.text_color(hsla(TEXT));
            if let Some((before, selected, after)) = self.query.split_selection() {
                let mut sel_bg = hsla(BLUE);
                sel_bg.a = 0.35;
                field = field
                    .child(SharedString::from(before))
                    .child(div().bg(sel_bg).rounded(px(2.0)).child(SharedString::from(selected)))
                    .child(SharedString::from(after));
            } else {
                let (before, after) = self.query.split();
                if before.is_empty() && after.is_empty() {
                    field = field
                        .child(div().w(px(1.0)).h(px(16.0)).bg(hsla(BLUE)))
                        .child(
                            div()
                                .text_color(hsla(MUTED))
                                .child(SharedString::from("Search settings\u{2026}")),
                        );
                } else {
                    field = field
                        .child(SharedString::from(before))
                        .child(div().w(px(1.0)).h(px(16.0)).bg(hsla(BLUE)))
                        .child(SharedString::from(after));
                }
            }
        }
        let mut bar = div().flex().items_center().gap_2().mb_2().child(field);
        if !self.query.is_empty() {
            bar = bar.child(
                div()
                    .text_color(hsla(MUTED))
                    .cursor_pointer()
                    .child(SharedString::from("\u{2715}"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _ev, _window, cx| {
                            this.query = guise::TextEdit::new("");
                            cx.notify();
                            cx.stop_propagation();
                        }),
                    ),
            );
        }
        bar
    }

    /// The content header: the settings-file badge and the escape hatch into
    /// the backing settings.json.
    fn content_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .justify_between()
            .pt(px(16.0))
            .child(
                div()
                    .h(px(24.0))
                    .px_2()
                    .rounded(px(5.0))
                    .bg(hsla(PILL_BG))
                    .flex()
                    .items_center()
                    .text_size(px(12.5))
                    .text_color(hsla(BLUE))
                    .child(SharedString::from("User")),
            )
            .child(
                button_box("Edit in settings.json").on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|_this, _ev, _window, cx| {
                        super::open_settings_file();
                        cx.stop_propagation();
                    }),
                ),
            )
    }

    /// The page title and monospace group label shown when browsing. Sections
    /// that are a single labeled group (Keyboard, Macros, Plugins) skip the
    /// label — the group's own header carries it.
    fn section_header(&self) -> impl IntoElement {
        let grouped = matches!(
            self.section,
            Section::Keyboard | Section::Macros | Section::Plugins
        );
        let mut header = div().flex().flex_col().pt(px(18.0)).child(
            div()
                .text_size(px(22.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(hsla(TEXT))
                .child(SharedString::from(self.section.title())),
        );
        if !grouped {
            header = header.child(self.heading(self.section.subtitle()));
        }
        header
    }

    fn content(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let searching = !self.search().is_empty();
        let mut body = div()
            .id("settings-content")
            .flex_1()
            .min_h(px(0.0))
            .pb(px(40.0))
            .overflow_y_scroll();
        if searching {
            for group in self.search_results(cx) {
                body = body.child(group);
            }
        } else {
            body = body.child(self.section_header());
            for group in self.section_content(cx) {
                body = body.child(group);
            }
        }
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w(px(0.0))
            .h_full()
            .px(px(32.0))
            .pt(px(12.0))
            .bg(hsla(CONTENT_BG))
            .child(self.content_header(cx))
            .child(body)
    }
}

impl Render for SettingsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key_down))
            .text_size(px(14.0))
            .text_color(hsla(TEXT))
            .bg(hsla(CONTENT_BG))
            .child(self.sidebar(cx))
            .child(self.content(cx))
            .children(self.choice_overlay(cx))
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
        .right_0()
        .h(px(30.0))
        .window_control_area(WindowControlArea::Drag)
        .on_mouse_down(MouseButton::Left, |_, window, _| window.start_window_move())
}

/// The shared chrome for a small bordered button (no behavior attached yet).
fn button_box(label: impl Into<SharedString>) -> Div {
    div()
        .h(px(28.0))
        .min_w(px(28.0))
        .px_3()
        .rounded(px(6.0))
        .border_1()
        .border_color(hsla(LINE))
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(13.0))
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

// The palette, sampled from Zed's dark settings window.
const SIDEBAR_BG: theme::Rgb = theme::Rgb::new(48, 52, 61); // #30343d
const CONTENT_BG: theme::Rgb = theme::Rgb::new(41, 44, 50); // #292c32
const NAV_SELECTED: theme::Rgb = theme::Rgb::new(59, 63, 73); // #3b3f49
const LINE: theme::Rgb = theme::Rgb::new(55, 60, 69); // #373c45
const FIELD_BG: theme::Rgb = theme::Rgb::new(47, 52, 61); // #2f343d
const FIELD_BORDER: theme::Rgb = theme::Rgb::new(71, 75, 86); // #474b56
const PILL_BG: theme::Rgb = theme::Rgb::new(50, 57, 68); // #323944
const TEXT: theme::Rgb = theme::Rgb::new(221, 224, 228); // #dde0e4
const MUTED: theme::Rgb = theme::Rgb::new(143, 148, 159); // #8f949f
const NAV_TEXT: theme::Rgb = theme::Rgb::new(170, 175, 187); // #aaafbb
const BLUE: theme::Rgb = theme::Rgb::new(128, 171, 227); // #80abe3
const BLUE_TEXT: theme::Rgb = theme::Rgb::new(128, 171, 227); // #80abe3
const TOGGLE_ON: theme::Rgb = theme::Rgb::new(75, 95, 121); // #4b5f79
const TOGGLE_ON_EDGE: theme::Rgb = theme::Rgb::new(86, 110, 142); // #566e8e
const KNOB_OFF: theme::Rgb = theme::Rgb::new(134, 138, 145); // #868a91
