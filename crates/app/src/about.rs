//! "About Prompt" panel, opened from the application menu — the macOS
//! convention of a small centered card showing the app icon, version, and
//! build date. Static content; no interaction beyond closing the window.

use std::sync::Arc;

use gpui::prelude::*;
use gpui::{
    bounds, div, img, point, px, size, App, FontWeight, Image, ImageFormat, SharedString,
    TitlebarOptions, Window, WindowBounds, WindowOptions,
};

use crate::colors;

const WIDTH: f32 = 380.0;
const HEIGHT: f32 = 420.0;

/// The app icon, embedded so the panel works without the bundle's resources
/// (e.g. when run straight from `cargo run`).
const ICON: &[u8] = include_bytes!("../../../assets/icon.png");

/// Compiled-in release metadata.
const VERSION: &str = env!("CARGO_PKG_VERSION");
const RELEASE_DATE: &str = env!("PROMPT_RELEASE_DATE");

/// Open the About panel centered over `parent`.
pub fn open(parent: &Window, cx: &mut App) {
    // Reuse an already-open About window instead of stacking duplicates.
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
                title: Some("About Prompt".into()),
                appears_transparent: true,
                traffic_light_position: Some(point(px(12.0), px(12.0))),
            }),
            ..Default::default()
        },
        |window, cx| {
            window.set_window_title("About Prompt");
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
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let line = |text: String, color: theme::Rgb, sz: f32, weight: FontWeight| {
            div()
                .text_size(px(sz))
                .font_weight(weight)
                .text_color(hsla(color))
                .child(SharedString::from(text))
        };

        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .bg(hsla(BG))
            .pt(px(64.0))
            .pb(px(28.0))
            .px(px(28.0))
            .child(
                img(self.icon.clone())
                    .w(px(128.0))
                    .h(px(128.0))
                    .mb_5(),
            )
            .child(line("Prompt".into(), TEXT, 26.0, FontWeight::BOLD))
            .child(
                line(
                    format!("Version {VERSION}"),
                    MUTED,
                    13.0,
                    FontWeight::NORMAL,
                )
                .mt_1(),
            )
            .child(
                line(
                    "A fast, modern terminal that gets out of your way.".into(),
                    BODY,
                    13.0,
                    FontWeight::NORMAL,
                )
                .mt_4()
                .max_w(px(280.0))
                .text_center(),
            )
            // Release date and copyright sink to the bottom of the card.
            .child(div().flex_1())
            .child(line(
                format!("Released {RELEASE_DATE}"),
                FAINT,
                12.0,
                FontWeight::NORMAL,
            ))
            .child(
                line(
                    "Apache-2.0 licensed".into(),
                    FAINT,
                    12.0,
                    FontWeight::NORMAL,
                )
                .mt_1(),
            )
    }
}

fn hsla(rgb: theme::Rgb) -> gpui::Hsla {
    colors::hsla(rgb)
}

const BG: theme::Rgb = theme::Rgb::new(35, 42, 44);
const TEXT: theme::Rgb = theme::Rgb::new(242, 244, 246);
const BODY: theme::Rgb = theme::Rgb::new(206, 212, 217);
const MUTED: theme::Rgb = theme::Rgb::new(170, 177, 181);
const FAINT: theme::Rgb = theme::Rgb::new(132, 139, 143);
