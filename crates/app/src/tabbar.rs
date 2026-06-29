//! The tab strip that lives inside the window titlebar: one button per tab
//! plus a new-tab control. Click activates a tab, the × closes it, + opens one.

use gpui::prelude::*;
use gpui::{div, px, ClickEvent, Context, MouseButton, SharedString, WindowControlArea};
use theme::Rgb;

use crate::colors::{self, Colors};
use crate::root::{TabBarMenu, WorkspaceView};

/// Linear mix of two colors: `t` 0 is `a`, 1 is `b`. Clamped.
pub fn blend(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    let mix = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Rgb::new(mix(a.r, b.r), mix(a.g, b.g), mix(a.b, b.b))
}

/// Per-tab data rendered in the strip: the label and whether the tab is
/// waiting on the user (a notification).
pub struct TabInfo {
    pub title: String,
    pub attention: bool,
}

/// Attention accent for a tab with a pending notification.
const ATTENTION: Rgb = Rgb::new(255, 196, 0);

/// Nominal per-tab width used to decide how many tabs fit before the strip
/// overflows into the `…` dropdown.
pub const TAB_BUDGET_PX: f32 = 130.0;

/// How many tab slots fit in `avail` pixels (always at least one).
pub fn fit_count(avail: f32) -> usize {
    ((avail / TAB_BUDGET_PX).floor() as usize).max(1)
}

/// Split tab indices into `(visible, overflow)` given the max inline slots.
/// The active tab is always kept visible; when everything fits, overflow is
/// empty. One inline slot is reserved for the `…` button when overflowing.
pub fn visible_split(n: usize, active: usize, max_visible: usize) -> (Vec<usize>, Vec<usize>) {
    let max_visible = max_visible.max(1);
    if n <= max_visible {
        return ((0..n).collect(), Vec::new());
    }
    let show = max_visible.saturating_sub(1).max(1);
    let mut visible: Vec<usize> = (0..show).collect();
    if !visible.contains(&active) {
        if let Some(last) = visible.last_mut() {
            *last = active;
        }
        visible.sort_unstable();
    }
    let overflow: Vec<usize> = (0..n).filter(|i| !visible.contains(i)).collect();
    (visible, overflow)
}

/// The inline row of tabs. The active tab keeps the terminal background;
/// inactive tabs are dimmed and brighten on hover. When more tabs exist than
/// `max_visible` slots, the overflow folds into a trailing `…` button (which
/// opens a dropdown); a final + opens a new tab.
#[allow(clippy::too_many_arguments)]
pub fn tabs(
    tabs: &[TabInfo],
    active: usize,
    max_visible: usize,
    open_menu: Option<TabBarMenu>,
    colors: &Colors,
    font: &gpui::Font,
    font_size: gpui::Pixels,
    cx: &mut Context<WorkspaceView>,
) -> impl IntoElement {
    let activebg = colors::rgba(colors.bg);
    let fg = colors::hsla(colors.fg);
    let attention = colors::hsla(ATTENTION);
    let mut dim = fg;
    dim.a = 0.55;
    let mut hover = fg;
    hover.a = 0.10;

    let (visible, overflow) = visible_split(tabs.len(), active, max_visible);

    let mut row = div()
        .flex_1()
        .flex()
        .flex_row()
        .items_center()
        .h_full()
        .font_family(font.family.clone())
        .text_size(font_size * 0.85)
        .children(visible.into_iter().map(|index| {
            let info = &tabs[index];
            let isactive = index == active;
            div()
                .id(("tab", index))
                .flex()
                .flex_row()
                .items_center()
                .gap(px(6.0))
                .h_full()
                .flex_grow(100.0)
                .min_w(px(100.0))
                .max_w(px(240.0))
                .px(px(10.0))
                .border_b_2()
                .border_color(if info.attention {
                    attention
                } else {
                    gpui::transparent_black()
                })
                .when(isactive, |d| d.bg(activebg))
                .text_color(if isactive { fg } else { dim })
                .hover(|s| s.bg(hover))
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.activatetab(index, window, cx);
                }))
                .when(info.attention, |d| {
                    d.child(
                        div()
                            .flex_none()
                            .w(px(6.0))
                            .h(px(6.0))
                            .rounded_full()
                            .bg(attention),
                    )
                })
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .flex()
                        .flex_col()
                        .justify_center()
                        .child(
                            div()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .child(SharedString::from(info.title.clone())),
                        ),
                )
                .child(
                    div()
                        .id(("tabclose", index))
                        .px(px(4.0))
                        .rounded(px(4.0))
                        .text_color(dim)
                        .hover(|s| s.bg(hover).text_color(fg))
                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                            cx.stop_propagation();
                            this.closetab(index, window, cx);
                        }))
                        .child("\u{00d7}"),
                )
        }));

    if !overflow.is_empty() {
        let n = overflow.len();
        row = row.child(
            div()
                .id("taboverflow")
                .flex()
                .items_center()
                .justify_center()
                .gap(px(3.0))
                .px(px(10.0))
                .h_full()
                .text_color(dim)
                .hover(|s| s.bg(hover).text_color(fg))
                .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                    this.toggle_tab_overflow(cx);
                }))
                .child(SharedString::from("\u{2026}"))
                .child(SharedString::from(n.to_string())),
        );
    }

    let mut filler = div()
        .id("titlebar-drag")
        .flex_1()
        .h_full()
        .window_control_area(WindowControlArea::Drag)
        .on_mouse_down(MouseButton::Left, |_, window, _| window.start_window_move());
    #[cfg(target_os = "macos")]
    {
        filler = filler.on_click(|ev, window, _| {
            if ev.click_count() == 2 {
                window.titlebar_double_click();
            }
        });
    }
    #[cfg(target_os = "linux")]
    {
        filler = filler.on_click(|ev, window, _| {
            if ev.click_count() == 2 {
                window.zoom_window();
            }
        });
    }
    row = row.child(filler);

    // Trailing button section: new (+) and split-into-column, each opening a
    // dropdown of choices. The active button (open menu) stays highlighted.
    let mut sep = fg;
    sep.a = 0.18;

    let button = move |id: &'static str, glyph: &'static str, menu: TabBarMenu| {
        let open = open_menu == Some(menu);
        div()
            .id(id)
            .flex()
            .items_center()
            .justify_center()
            .w(px(34.0))
            .h_full()
            .text_color(if open { fg } else { dim })
            .when(open, |s| s.bg(hover))
            .hover(|s| s.bg(hover).text_color(fg))
            .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                this.toggle_trailing_menu(menu, cx);
            }))
            .child(SharedString::from(glyph))
    };

    row.child(
        div()
            .flex()
            .flex_row()
            .items_center()
            .h_full()
            .border_l_1()
            .border_color(sep)
            .child(button("newtab", "+", TabBarMenu::New))
            .child(
                button("splitcolumn", "\u{25eb}", TabBarMenu::Split)
                    .border_l_1()
                    .border_color(sep),
            ),
    )
}

#[cfg(test)]
#[path = "../tests/tabbar.rs"]
mod tests;
