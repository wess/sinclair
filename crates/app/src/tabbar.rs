//! The tab strip that lives inside the window titlebar: one button per tab
//! plus a new-tab control. Click activates a tab, the × closes it, + opens one.

use gpui::prelude::*;
use gpui::{div, px, ClickEvent, Context, SharedString};
use theme::Rgb;

use crate::colors::{self, Colors};
use crate::root::WorkspaceView;

/// Linear mix of two colors: `t` 0 is `a`, 1 is `b`. Clamped.
pub fn blend(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    let mix = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Rgb::new(mix(a.r, b.r), mix(a.g, b.g), mix(a.b, b.b))
}

/// The inline row of tabs. The active tab keeps the terminal background and a
/// foreground accent underline; inactive tabs are dimmed and brighten on hover.
/// A trailing + opens a new tab.
pub fn tabs(
    titles: &[String],
    active: usize,
    colors: &Colors,
    font: &gpui::Font,
    font_size: gpui::Pixels,
    cx: &mut Context<WorkspaceView>,
) -> impl IntoElement {
    let activebg = colors::rgba(colors.bg);
    let fg = colors::hsla(colors.fg);
    let mut dim = fg;
    dim.a = 0.55;
    let mut hover = fg;
    hover.a = 0.10;
    let accent = fg;

    div()
        .flex()
        .flex_row()
        .items_center()
        .h_full()
        .font_family(font.family.clone())
        .text_size(font_size * 0.85)
        .children(titles.iter().enumerate().map(|(index, title)| {
            let isactive = index == active;
            div()
                .id(("tab", index))
                .flex()
                .flex_row()
                .items_center()
                .justify_center()
                .gap(px(6.0))
                .h_full()
                .min_w(px(90.0))
                .max_w(px(220.0))
                .px(px(10.0))
                // A bottom accent on the active tab; a matching transparent
                // border on inactive tabs keeps the text baseline steady.
                .border_b_2()
                .border_color(if isactive { accent } else { gpui::transparent_black() })
                .when(isactive, |d| d.bg(activebg))
                .text_color(if isactive { fg } else { dim })
                .hover(|s| s.bg(hover))
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.activatetab(index, window, cx);
                }))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(SharedString::from(title.clone())),
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
        }))
        .child(
            div()
                .id("newtab")
                .flex()
                .items_center()
                .justify_center()
                .w(px(34.0))
                .h_full()
                .text_color(dim)
                .hover(|s| s.bg(hover).text_color(fg))
                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                    this.newtab(window, cx);
                }))
                .child(SharedString::from("+")),
        )
}

#[cfg(test)]
#[path = "../tests/tabbar.rs"]
mod tests;
