//! Standalone-window placement for a plugin `[webview]`. Opens a normal titled,
//! resizable window whose whole content is the plugin's [`PluginWebView`]. Used
//! when the manifest sets `placement = "window"` (and, for now, `placement =
//! "tab"` until tab hosting lands).

use gpui::prelude::*;
use gpui::{
    bounds, point, px, size, App, TitlebarOptions, Window, WindowBounds, WindowOptions,
};

use crate::pluginwebview::{PluginWebView, WebviewSurface};

const WIDTH: f32 = 760.0;
const HEIGHT: f32 = 560.0;

/// Open a window hosting `plugin`'s webview, centered over `parent`.
pub fn open(parent: &Window, plugin: plugin::Plugin, cx: &mut App) {
    let title = plugin
        .webview
        .as_ref()
        .map(|w| w.title.clone())
        .unwrap_or_else(|| plugin.name.clone());
    let center = parent.bounds().center();
    let where_ = bounds(
        center - point(px(WIDTH / 2.0), px(HEIGHT / 2.0)),
        size(px(WIDTH), px(HEIGHT)),
    );
    let handle = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(where_)),
            is_resizable: true,
            titlebar: Some(TitlebarOptions {
                title: Some(title.clone().into()),
                appears_transparent: false,
                traffic_light_position: None,
            }),
            ..Default::default()
        },
        |window, cx| {
            window.set_window_title(&title);
            cx.new(|cx| PluginWebView::new(WebviewSurface::from_plugin(plugin), cx))
        },
    );
    if let Ok(handle) = handle {
        handle
            .update(cx, |_view, window, _cx| window.activate_window())
            .ok();
    }
}
