//! "About Sinclair" panel, opened from the application menu, the macOS
//! convention of a small centered card showing the app icon, version, and
//! build date. Static content; no interaction beyond closing the window.
//!
//! Rendered with guise components (Title/Text/Anchor) over the shared guise
//! theme, so it tracks the active terminal palette like the rest of the chrome.

use std::sync::Arc;

use gpui::prelude::*;
use gpui::{
    bounds, div, img, point, px, size, App, ClickEvent, Image, ImageFormat, TitlebarOptions,
    Window, WindowBounds, WindowOptions,
};
use guise::{Anchor, Size, Text, Title};

const WIDTH: f32 = 380.0;
const HEIGHT: f32 = 420.0;

/// The app icon, embedded so the panel works without the bundle's resources
/// (e.g. when run straight from `cargo run`).
const ICON: &[u8] = include_bytes!("../../../assets/icon.png");

/// Compiled-in build metadata (see `build.rs`).
const VERSION: &str = env!("CARGO_PKG_VERSION");
const BUILD_DATE: &str = env!("SINCLAIR_BUILD_DATE");
/// Whether this build is the tagged release of [`VERSION`], rather than some
/// later commit that merely carries the same version number.
const RELEASED: bool = matches!(env!("SINCLAIR_RELEASED").as_bytes(), b"1");

/// The dated line at the foot of the panel, for this build.
fn build_line() -> String {
    dateline(RELEASED, BUILD_DATE)
}

/// Only a build made from the version's own tag can honestly call its date a
/// release date; anything else says what it actually is, so a development build
/// cannot pass itself off as the release. Split out from the compiled-in
/// constants so every case can be tested rather than only the one this build is.
fn dateline(released: bool, date: &str) -> String {
    match (released, date) {
        (true, date) => format!("Released {date}"),
        // Outside a git checkout there is no date to qualify, so the line is
        // just what the build is.
        (false, "unknown") => "Development build".to_string(),
        (false, date) => format!("Development build \u{b7} {date}"),
    }
}

/// Project home page, opened when the link is clicked.
const REPO: &str = "https://github.com/wess/sinclair";

/// Open the About panel centered over `parent`.
pub fn open(parent: &Window, cx: &mut App) {
    for handle in cx.windows() {
        if handle
            .downcast::<AboutView>()
            .and_then(|h| h.update(cx, |_, window, _| window.activate_window()).ok())
            .is_some()
        {
            return;
        }
    }

    let center = parent.bounds().center();
    let bounds = bounds(
        center - point(px(WIDTH / 2.0), px(HEIGHT / 2.0)),
        size(px(WIDTH), px(HEIGHT)),
    );
    let _ = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            is_resizable: false,
            is_minimizable: false,
            titlebar: Some(TitlebarOptions {
                title: Some("About Sinclair".into()),
                appears_transparent: true,
                traffic_light_position: Some(point(px(12.0), px(12.0))),
            }),
            ..Default::default()
        },
        |window, cx| {
            window.set_window_title("About Sinclair");
            cx.new(|_| AboutView::new())
        },
    );
}

pub struct AboutView {
    icon: Arc<Image>,
}

impl AboutView {
    fn new() -> Self {
        Self {
            icon: Arc::new(Image::from_bytes(ImageFormat::Png, ICON.to_vec())),
        }
    }
}

impl Render for AboutView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = guise::theme(cx);
        let bg = t.body().hsla();

        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .bg(bg)
            .pt(px(64.0))
            .pb(px(28.0))
            .px(px(28.0))
            .child(img(self.icon.clone()).w(px(128.0)).h(px(128.0)).mb_5())
            .child(Title::new("Sinclair").order(2))
            .child(
                div().mt_1().child(
                    Text::new(format!("Version {VERSION}"))
                        .size(Size::Sm)
                        .dimmed(),
                ),
            )
            .child(div().mt_4().max_w(px(280.0)).text_center().child(
                Text::new("A fast, modern terminal that gets out of your way.").size(Size::Sm),
            ))
            .child(
                div().mt_3().child(
                    Anchor::new("about-repo-link", "github.com/wess/sinclair")
                        .size(Size::Sm)
                        .on_click(|_: &ClickEvent, _, cx| cx.open_url(REPO)),
                ),
            )
            .child(div().flex_1())
            .child(Text::new(build_line()).size(Size::Xs).dimmed())
            .child(
                div()
                    .mt_1()
                    .child(Text::new("Apache-2.0 licensed").size(Size::Xs).dimmed()),
            )
    }
}

#[cfg(test)]
#[path = "../tests/about.rs"]
mod tests;
