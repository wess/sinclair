//! Schema-driven rendering: every row comes from a [`Setting`] — label,
//! description, the right control for its type, and, when the user's file
//! overrides the key, an inline reset arrow after the label.

use super::super::schema::{self, Control, Section, Setting};
use super::super::{EditTarget, SettingsView};
use super::*;
use gpui::{px, AnyElement, Context};

impl SettingsView {
    /// One settings row: title and description left, the control right.
    pub(crate) fn setting_row(&self, s: &'static Setting, cx: &mut Context<Self>) -> AnyElement {
        let control: AnyElement = match &s.control {
            Control::Toggle(get) => self.switch(s, *get, cx).into_any_element(),
            Control::Slider(n) => self.slider(s, *n, cx).into_any_element(),
            Control::Choice(c) => self.choice_button(s, *c, cx).into_any_element(),
            Control::Text { get, placeholder } => self
                .text_input(EditTarget::Field(s.key), get(&self.opts), placeholder, 230.0, cx)
                .into_any_element(),
            // List settings render as groups, not rows.
            Control::List(_) => div().into_any_element(),
        };
        div()
            .w_full()
            .min_h(px(64.0))
            .py(px(14.0))
            .flex()
            .items_center()
            .justify_between()
            .gap_3()
            .child(self.row_label(s, cx))
            .child(div().flex().items_center().flex_none().child(control))
            .into_any_element()
    }

    /// The label + description column, with the reset arrow inline after the
    /// title while the user's file overrides the key.
    fn row_label(&self, s: &'static Setting, cx: &mut Context<Self>) -> impl IntoElement {
        let mut name = div()
            .flex()
            .items_center()
            .gap_2()
            .text_color(hsla(TEXT))
            .child(SharedString::from(s.label));
        if self.modified(s.key) {
            name = name.child(self.reset_button(s.key, cx));
        }
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w(px(0.0))
            .gap(px(3.0))
            .child(name)
            .child(
                div()
                    .text_size(px(12.5))
                    .text_color(hsla(MUTED))
                    .child(SharedString::from(s.desc)),
            )
    }

    /// The `↺` reset that removes a key from settings.json.
    pub(crate) fn reset_button(&self, key: &'static str, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .text_size(px(13.0))
            .text_color(hsla(MUTED))
            .cursor_pointer()
            .hover(|d| d.text_color(hsla(TEXT)))
            .child(SharedString::from("\u{21ba}"))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _ev, _window, cx| {
                    this.reset(key, cx);
                    cx.stop_propagation();
                }),
            )
    }

    /// The rows and list groups of the selected section (search empty).
    pub(crate) fn section_content(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        match self.section {
            Section::Keyboard => vec![self.keyboard_group(cx).into_any_element()],
            Section::Macros => vec![self.macros_group(cx).into_any_element()],
            Section::Ai => self.ai_content(cx),
            section => {
                let settings: Vec<&'static Setting> = schema::in_section(section).collect();
                self.rows_and_groups(&settings, cx)
            }
        }
    }

    /// Scalar settings collect into one divided run; each List setting
    /// renders as its own labeled group below, in declaration order.
    pub(crate) fn rows_and_groups(
        &self,
        settings: &[&'static Setting],
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let mut rows: Vec<AnyElement> = Vec::new();
        let mut groups: Vec<AnyElement> = Vec::new();
        for s in settings {
            match &s.control {
                Control::List(kind) => groups.push(self.list_group(s, *kind, cx).into_any_element()),
                _ => rows.push(self.setting_row(s, cx)),
            }
        }
        let mut out: Vec<AnyElement> = Vec::new();
        if !rows.is_empty() {
            out.push(self.list(rows).into_any_element());
        }
        out.extend(groups);
        out
    }

    /// Search mode: every matching setting, grouped under section headings.
    pub(crate) fn search_results(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let query = self.search();
        let mut out: Vec<AnyElement> = Vec::new();
        for section in Section::ALL {
            let mut matched: Vec<&'static Setting> =
                schema::in_section(section).filter(|s| s.matches(&query)).collect();
            // The Macros section has no schema entries; match it by name.
            let macros_hit = section == Section::Macros && word_match(&query, "macros replay shortcut");
            if matched.is_empty() && !macros_hit {
                continue;
            }
            out.push(self.heading(section.title()).into_any_element());
            if section == Section::Keyboard {
                // Keep the capture/restore chrome with the keybind list.
                matched.retain(|s| s.key != "keybind");
                out.push(self.keyboard_group(cx).into_any_element());
            }
            if macros_hit {
                out.push(self.macros_group(cx).into_any_element());
            }
            out.extend(self.rows_and_groups(&matched, cx));
        }
        if out.is_empty() {
            out.push(
                div()
                    .pt_4()
                    .text_color(hsla(MUTED))
                    .child(SharedString::from(format!("No settings match \u{201c}{query}\u{201d}")))
                    .into_any_element(),
            );
        }
        out
    }
}

/// Every query word appears in `haystack` (case-insensitive).
fn word_match(query: &str, haystack: &str) -> bool {
    let hay = haystack.to_lowercase();
    query
        .to_lowercase()
        .split_whitespace()
        .all(|w| hay.contains(w))
}
