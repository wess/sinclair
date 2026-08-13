//! Minimal embedding: one window hosting a [`TermView`] running the login
//! shell, quitting when the shell exits.
//!
//! ```sh
//! cargo run -p libsinclair --example embed
//! ```

use gpui::AppContext as _;
use gpui::{px, size, App, Bounds, Focusable, WindowBounds, WindowOptions};
use libsinclair::terminal::{Event, SessionOptions};
use libsinclair::termview::{TermOptions, TermView};

fn main() {
    gpui_platform::application().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(800.0), px(520.0)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            ..Default::default()
        };
        let window = cx
            .open_window(options, |window, cx| {
                cx.new(|cx| {
                    TermView::spawn(
                        SessionOptions::default(),
                        TermOptions::default(),
                        window,
                        cx,
                    )
                    .expect("spawn shell")
                })
            })
            .expect("open window");
        window
            .update(cx, |view, window, cx| {
                window.focus(&view.focus_handle(cx), cx);
                cx.subscribe(&cx.entity(), |_, _, event: &Event, cx| {
                    if let Event::Exit(_) = event {
                        cx.quit();
                    }
                })
                .detach();
            })
            .ok();
        cx.activate(true);
    });
}
