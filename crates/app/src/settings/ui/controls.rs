//! The per-type controls: switch, slider, choice dropdown, and text field,
//! plus the shared flat-row chrome (dividers, monospace group labels).

use super::super::schema::{self, Choice, Control, Setting, Slider};
use super::super::{EditTarget, SettingsView};
use super::*;
use gpui::{
    anchored, canvas, deferred, div, point, px, relative, AnyElement, Context, DragMoveEvent,
    Empty, MouseButton, MouseDownEvent, SharedString,
};

/// Drag payload identifying which slider a scrub belongs to, so the shared
/// `on_drag_move` listener only acts on the track the drag started on.
struct SliderDrag(&'static str);

impl SettingsView {
    pub(crate) fn icon(&self, glyph: &str, size: gpui::Pixels) -> impl IntoElement {
        div()
            .w(size)
            .h(size)
            .flex()
            .items_center()
            .justify_center()
            .text_color(hsla(MUTED))
            .child(SharedString::from(glyph.to_string()))
    }

    /// A plain label/control row, for the hand-built groups (macros, relay
    /// status, agent tools).
    pub(crate) fn row(
        &self,
        icon: impl IntoElement,
        label: &str,
        control: impl IntoElement,
    ) -> impl IntoElement {
        div()
            .w_full()
            .h(px(52.0))
            .flex()
            .items_center()
            .justify_between()
            .gap_3()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .flex_none()
                    .child(icon)
                    .child(SharedString::from(label.to_string())),
            )
            .child(control)
    }

    /// A full-width hairline, the row and group separator.
    pub(crate) fn divider(&self) -> impl IntoElement {
        div().w_full().h(px(1.0)).bg(hsla(LINE))
    }

    /// Rows separated by full-width hairlines (no panel chrome).
    pub(crate) fn list(&self, rows: Vec<AnyElement>) -> impl IntoElement {
        let mut list = div().w_full().flex().flex_col();
        for (i, row) in rows.into_iter().enumerate() {
            if i > 0 {
                list = list.child(self.divider());
            }
            list = list.child(row);
        }
        list
    }

    /// A monospace group label, like Zed's subsection headers.
    pub(crate) fn mono_label(&self, text: &str) -> impl IntoElement {
        div()
            .font_family(SharedString::from(self.opts.primary_font().to_string()))
            .text_size(px(12.5))
            .text_color(hsla(MUTED))
            .child(SharedString::from(text.to_string()))
    }

    /// A group label with its underline divider.
    pub(crate) fn heading(&self, text: &str) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .pt(px(18.0))
            .gap(px(10.0))
            .child(self.mono_label(text))
            .child(self.divider())
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
        let border = hsla(if active { BLUE } else { LINE });
        // Flexible width: grow to `width` but shrink below it when the row is
        // narrow, so long values never overflow the content column.
        let mut field = div()
            .flex_1()
            .min_w(px(0.0))
            .max_w(px(width))
            .h(px(28.0))
            .px_2()
            .rounded(px(6.0))
            .border_1()
            .border_color(border)
            .bg(hsla(FIELD_BG))
            .text_size(px(13.0))
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
                    .child(div().w(px(1.0)).h(px(16.0)).bg(hsla(BLUE)))
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

    pub(crate) fn switch(
        &self,
        s: &'static Setting,
        get: fn(&config::Options) -> bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let on = get(&self.opts);
        div()
            .w(px(32.0))
            .h(px(18.0))
            .rounded(px(9.0))
            .border_1()
            .border_color(hsla(if on { TOGGLE_ON_EDGE } else { FIELD_BORDER }))
            .bg(hsla(if on { TOGGLE_ON } else { FIELD_BG }))
            .relative()
            .cursor_pointer()
            .child(
                div()
                    .absolute()
                    .left(px(if on { 15.0 } else { 1.0 }))
                    .top(px(1.5))
                    .w(px(13.0))
                    .h(px(13.0))
                    .rounded(px(6.5))
                    .bg(hsla(if on { TEXT } else { KNOB_OFF })),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _ev, _window, cx| {
                    this.toggle(s, cx);
                    cx.stop_propagation();
                }),
            )
    }

    /// A draggable value track for a numeric option. Press anywhere on the
    /// track to jump to that value; press and drag to scrub — the drag follows
    /// the pointer anywhere in the window (like a real slider), not just while
    /// it stays over the track.
    pub(crate) fn slider(
        &self,
        s: &'static Setting,
        n: Slider,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let frac = n.fraction(&self.opts);

        let bar = div()
            .absolute()
            .left(px(0.0))
            .right(px(0.0))
            .top(px(7.0))
            .h(px(6.0))
            .rounded(px(3.0))
            .bg(hsla(FIELD_BG));

        let fill = div()
            .absolute()
            .left(px(0.0))
            .top(px(7.0))
            .w(relative(frac))
            .h(px(6.0))
            .rounded(px(3.0))
            .bg(hsla(TOGGLE_ON));

        let knob = div()
            .absolute()
            .left(relative(frac))
            .ml(px(-7.0))
            .top(px(3.0))
            .w(px(14.0))
            .h(px(14.0))
            .rounded(px(7.0))
            .bg(hsla(TEXT))
            .border_2()
            .border_color(hsla(TOGGLE_ON_EDGE));

        // Invisible probe that records the track's window-space bounds each
        // frame, so a mouse-down (position only, no bounds) maps to a value.
        let key = s.key;
        let entity = cx.entity();
        let probe = canvas(
            move |bounds, _window, cx| {
                entity.update(cx, |view, _| {
                    view.slider_bounds.insert(key, bounds);
                });
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full();

        let track = div()
            .id(s.key)
            .relative()
            .w(px(150.0))
            .h(px(20.0))
            .cursor_pointer()
            .child(probe)
            .child(bar)
            .child(fill)
            .child(knob)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, _window, cx| {
                    if let Some(b) = this.slider_bounds.get(s.key).copied() {
                        let width = f32::from(b.size.width);
                        if width > 0.0 {
                            let frac = (f32::from(event.position.x - b.left()) / width).clamp(0.0, 1.0);
                            this.slide_to(s, frac, cx);
                        }
                    }
                }),
            )
            .on_drag(SliderDrag(s.key), |_drag, _offset, _window, cx| cx.new(|_| Empty))
            .on_drag_move::<SliderDrag>(cx.listener(
                move |this, event: &DragMoveEvent<SliderDrag>, _window, cx| {
                    // Every track's listener fires for any slider drag; act only
                    // on the one the drag started on, using this track's bounds.
                    if event.drag(cx).0 != s.key {
                        return;
                    }
                    let b = event.bounds;
                    let width = f32::from(b.size.width);
                    if width > 0.0 {
                        let frac = (f32::from(event.event.position.x - b.left()) / width).clamp(0.0, 1.0);
                        this.slide_to(s, frac, cx);
                    }
                },
            ));

        div()
            .flex()
            .items_center()
            .gap_3()
            .child(track)
            .child(
                div()
                    .w(px(56.0))
                    .flex()
                    .justify_end()
                    .text_size(px(13.0))
                    .text_color(hsla(TEXT))
                    .child(SharedString::from(n.display(&self.opts))),
            )
    }

    /// The closed dropdown: the current value plus a stacked chevron pair,
    /// like a macOS popup button. Click to open the menu below it.
    pub(crate) fn choice_button(
        &self,
        s: &'static Setting,
        c: Choice,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        // Bounds probe so the open menu can anchor below this button.
        let key = s.key;
        let entity = cx.entity();
        let probe = canvas(
            move |bounds, _window, cx| {
                entity.update(cx, |view, _| {
                    view.choice_bounds.insert(key, bounds);
                });
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full();

        let chevrons = div()
            .flex()
            .flex_col()
            .flex_none()
            .items_center()
            .text_color(hsla(MUTED))
            .text_size(px(8.0))
            .child(div().h(px(8.0)).child(SharedString::from("\u{25b2}")))
            .child(div().h(px(8.0)).child(SharedString::from("\u{25bc}")));

        div()
            .relative()
            .min_w(px(140.0))
            .h(px(28.0))
            .px_2()
            .rounded(px(6.0))
            .border_1()
            .border_color(hsla(LINE))
            .bg(hsla(FIELD_BG))
            .cursor_pointer()
            .flex()
            .items_center()
            .gap_2()
            .child(probe)
            .child(
                div()
                    .flex_1()
                    .text_size(px(13.0))
                    .text_color(hsla(TEXT))
                    .overflow_hidden()
                    .child(SharedString::from((c.get)(&self.opts))),
            )
            .child(chevrons)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _ev, _window, cx| {
                    this.toggle_choice(s.key, cx);
                    cx.stop_propagation();
                }),
            )
    }

    /// The open dropdown: a floating menu anchored below its button, over the
    /// content rather than pushing it, with a full-window dismiss layer.
    pub(crate) fn choice_overlay(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let key = self.open_choice?;
        let s = schema::find(key)?;
        let Control::Choice(c) = &s.control else {
            return None;
        };
        let bounds = self.choice_bounds.get(key).copied()?;

        let current = (c.get)(&self.opts);
        let mut items: Vec<(String, bool)> = Vec::new();
        if let Some(unset) = c.unset {
            items.push((unset.to_string(), true));
        }
        items.extend((c.variants)().into_iter().map(|v| (v, false)));

        let mut menu = div()
            .id("choice-menu")
            .min_w(bounds.size.width)
            .max_w(px(320.0))
            .max_h(px(320.0))
            .overflow_y_scroll()
            .p_1()
            .rounded(px(8.0))
            .border_1()
            .border_color(hsla(FIELD_BORDER))
            .bg(hsla(FIELD_BG))
            .shadow_lg()
            .flex()
            .flex_col();
        for (value, unset) in items {
            let selected = value == current;
            let mut item = div()
                .h(px(26.0))
                .px_2()
                .rounded(px(4.0))
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .text_size(px(13.0))
                .text_color(hsla(if selected { BLUE_TEXT } else { TEXT }))
                .hover(|d| d.bg(hsla(NAV_SELECTED)))
                .child(SharedString::from(value.clone()));
            if selected {
                item = item.child(SharedString::from("\u{2713}"));
            }
            menu = menu.child(item.on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _ev, _window, cx| {
                    this.choose(s, value.clone(), unset, cx);
                    cx.stop_propagation();
                }),
            ));
        }

        let dismiss = |this: &mut Self, _e: &MouseDownEvent, _w: &mut Window, cx: &mut Context<Self>| {
            this.open_choice = None;
            cx.stop_propagation();
            cx.notify();
        };
        Some(
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
                            .position(point(bounds.left(), bounds.bottom() + px(4.0)))
                            .snap_to_window_with_margin(px(8.0))
                            .child(menu),
                    ),
            )
            .into_any_element(),
        )
    }
}
