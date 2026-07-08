//! Window chrome. The `guise::PaneGroup`'s top-row tab bar doubles as the
//! titlebar (it reserves the traffic-light inset and is the window drag
//! handle), so this module is now just the macOS traffic-light clearance
//! constant, the Linux window controls (min/max/close), and the Linux resize
//! edges.

// Linux-only drawing below; macOS/Windows have native controls and only read
// the inset constant.
#[cfg(target_os = "linux")]
use gpui::prelude::*;
#[cfg(target_os = "linux")]
use gpui::{div, px, MouseButton, WindowControlArea};
#[cfg(target_os = "linux")]
use crate::colors::{self, Colors};

/// macOS traffic-light clearance reserved at the left of the group's top-row
/// tab bar (see `WorkspaceView::build_group`).
pub const TRAFFIC_LIGHT_INSET: f32 = 88.0;

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

/// The custom window controls as a top-right overlay, for when the pane group
/// doubles as the titlebar (the group reserves trailing space for them).
#[cfg(target_os = "linux")]
pub fn window_controls_overlay(colors: &Colors) -> impl IntoElement {
    div()
        .absolute()
        .top_0()
        .right_0()
        .h(px(28.0))
        .flex()
        .items_center()
        .pr(px(4.0))
        .child(window_controls(colors))
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
