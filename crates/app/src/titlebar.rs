//! Custom window titlebar. The window opens with a transparent native title
//! bar (see `open_window`), so this themed strip is the whole chrome: it folds
//! the tab strip in, is the window's drag handle, double-click zooms, and on
//! Linux it draws its own minimize / maximize / close controls and resize
//! edges. On macOS the native traffic lights stay, so the strip is inset to
//! clear them. Zed-style.

use gpui::prelude::*;
use gpui::{div, px, Context, Window, WindowControlArea};

use crate::colors::{self, Colors};
use crate::root::{TabBarMenu, WorkspaceView};
use crate::tabbar;

/// macOS traffic-light clearance, with a little breathing room before the
/// first tab (the lights themselves end around 67px).
pub const TRAFFIC_LIGHT_INSET: f32 = 88.0;

/// Title bar height, matching Zed: 1.75rem, floored at 34px.
pub fn height(window: &Window) -> gpui::Pixels {
    (window.rem_size() * 1.75).max(px(34.0))
}

/// The titlebar strip rendered as the first child of the workspace root.
#[allow(clippy::too_many_arguments)]
pub fn bar(
    tabs: &[tabbar::TabInfo],
    active: usize,
    max_visible: usize,
    open_menu: Option<TabBarMenu>,
    colors: &Colors,
    font: &gpui::Font,
    font_size: gpui::Pixels,
    window: &mut Window,
    cx: &mut Context<WorkspaceView>,
) -> impl IntoElement {
    let barbg = colors::rgba(tabbar::blend(colors.bg, colors.fg, 0.12));
    let on_mac = cfg!(target_os = "macos");
    let lead = if on_mac && !window.is_fullscreen() {
        px(TRAFFIC_LIGHT_INSET)
    } else {
        px(8.0)
    };

    let bar = div()
        .w_full()
        .h(height(window))
        .flex()
        .flex_row()
        .items_center()
        .bg(barbg)
        .window_control_area(WindowControlArea::Drag)
        .pl(lead)
        .child(tabbar::tabs(tabs, active, max_visible, open_menu, colors, font, font_size, cx));

    #[cfg(target_os = "linux")]
    let bar = bar.child(window_controls(colors));

    bar
}

/// Minimize / maximize / close buttons for platforms without native controls.
#[cfg(target_os = "linux")]
fn window_controls(colors: &Colors) -> impl IntoElement {
    let fg = colors::hsla(colors.fg);
    let mut dim = fg;
    dim.a = 0.6;
    let mut hover = fg;
    hover.a = 0.12;

    let button = move |id: &'static str, glyph: &'static str| {
        div()
            .id(id)
            .w(px(46.0))
            .h_full()
            .flex()
            .items_center()
            .justify_center()
            .text_color(dim)
            .hover(move |s| s.bg(hover).text_color(fg))
            .child(gpui::SharedString::from(glyph))
    };

    div()
        .flex()
        .flex_row()
        .items_center()
        .h_full()
        .child(
            button("win-min", "\u{2013}")
                .window_control_area(WindowControlArea::Min)
                .on_click(|_, window, _| window.minimize_window()),
        )
        .child(
            button("win-max", "\u{25a1}")
                .window_control_area(WindowControlArea::Max)
                .on_click(|_, window, _| window.zoom_window()),
        )
        .child(
            button("win-close", "\u{2715}")
                .window_control_area(WindowControlArea::Close)
                .on_click(|_, window, _| window.remove_window()),
        )
}

/// A transparent overlay of edge/corner hit-zones that start a window resize.
/// Only meaningful with client-side decorations (Linux), where the app draws
/// its own border. The overlay itself is inert in the center, so it never
/// intercepts clicks meant for the terminal.
#[cfg(target_os = "linux")]
pub fn resize_handles() -> impl IntoElement {
    use gpui::{MouseButton, ResizeEdge};

    let edge = |id: &'static str, e: ResizeEdge| {
        div()
            .id(id)
            .absolute()
            .on_mouse_down(MouseButton::Left, move |_, window, _| {
                window.start_window_resize(e);
            })
    };
    let t = px(6.0);
    let c = px(12.0);

    div()
        .absolute()
        .inset_0()
        .child(edge("rs-top", ResizeEdge::Top).top_0().left_0().right_0().h(t))
        .child(edge("rs-bottom", ResizeEdge::Bottom).bottom_0().left_0().right_0().h(t))
        .child(edge("rs-left", ResizeEdge::Left).top_0().bottom_0().left_0().w(t))
        .child(edge("rs-right", ResizeEdge::Right).top_0().bottom_0().right_0().w(t))
        .child(edge("rs-tl", ResizeEdge::TopLeft).top_0().left_0().w(c).h(c))
        .child(edge("rs-tr", ResizeEdge::TopRight).top_0().right_0().w(c).h(c))
        .child(edge("rs-bl", ResizeEdge::BottomLeft).bottom_0().left_0().w(c).h(c))
        .child(edge("rs-br", ResizeEdge::BottomRight).bottom_0().right_0().w(c).h(c))
}
