//! Documentation window opened from the Help menu. A searchable sidebar of
//! articles beside a reading pane. Content is static, embedded prose so the
//! help works offline; the sidebar search filters articles by title and body.

use gpui::prelude::*;
use gpui::{
    bounds, div, point, px, size, App, Context, FocusHandle, KeyDownEvent, SharedString,
    TitlebarOptions, Window, WindowBounds, WindowOptions,
};

use crate::colors;
use crate::textedit::TextEdit;

mod content;
mod render;

use content::ARTICLES;

const WIDTH: f32 = 760.0;
const HEIGHT: f32 = 820.0;
const SIDEBAR: f32 = 248.0;

/// Open the documentation window centered over `parent`.
pub fn open(parent: &Window, cx: &mut App) {
    let center = parent.bounds().center();
    let bounds = bounds(
        center - point(px(WIDTH / 2.0), px(HEIGHT / 2.0)),
        size(px(WIDTH), px(HEIGHT)),
    );
    let _ = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            is_resizable: false,
            titlebar: Some(TitlebarOptions {
                title: Some("Documentation".into()),
                appears_transparent: true,
                traffic_light_position: Some(point(px(20.0), px(20.0))),
            }),
            ..Default::default()
        },
        |window, cx| {
            window.set_window_title("Documentation");
            cx.new(HelpView::new)
        },
    );
}

pub struct HelpView {
    query: TextEdit,
    /// Selected article, an index into `ARTICLES`.
    selected: usize,
    focus: FocusHandle,
}

impl HelpView {
    fn new(cx: &mut Context<Self>) -> Self {
        Self {
            query: TextEdit::default(),
            selected: 0,
            focus: cx.focus_handle(),
        }
    }

    /// Article indices matching the current search query, in order.
    fn visible(&self) -> Vec<usize> {
        let q = self.query.text().to_lowercase();
        ARTICLES
            .iter()
            .enumerate()
            .filter(|(_, a)| a.matches(&q))
            .map(|(i, _)| i)
            .collect()
    }

    /// The article to display: the selection if it still matches, else the
    /// first visible one.
    fn current(&self, visible: &[usize]) -> Option<usize> {
        if visible.contains(&self.selected) {
            Some(self.selected)
        } else {
            visible.first().copied()
        }
    }

    fn key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let ks = &event.keystroke;
        if ks.modifiers.platform || ks.modifiers.control {
            return;
        }
        match ks.key.as_str() {
            "down" => self.move_selection(1),
            "up" => self.move_selection(-1),
            "backspace" => {
                self.query.backspace();
            }
            "delete" => {
                self.query.delete();
            }
            "left" => self.query.left(),
            "right" => self.query.right(),
            "home" => self.query.home(),
            "end" => self.query.end(),
            "escape" => {
                self.query = TextEdit::default();
            }
            _ => {
                if let Some(text) = ks
                    .key_char
                    .as_deref()
                    .filter(|t| !t.is_empty() && !ks.modifiers.alt)
                {
                    self.query.insert(text);
                }
            }
        }
        cx.notify();
        cx.stop_propagation();
    }

    /// Step the selection through the currently visible articles.
    fn move_selection(&mut self, delta: i32) {
        let visible = self.visible();
        if visible.is_empty() {
            return;
        }
        let pos = visible
            .iter()
            .position(|&i| i == self.selected)
            .unwrap_or(0) as i32;
        let n = visible.len() as i32;
        let next = ((pos + delta) % n + n) % n;
        self.selected = visible[next as usize];
    }
}

impl Render for HelpView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let visible = self.visible();
        let current = self.current(&visible);
        let mut root = div()
            .size_full()
            .flex()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key_down))
            .text_color(hsla(TEXT))
            .bg(hsla(CONTENT_BG))
            .child(self.sidebar(&visible, current, cx));
        if let Some(index) = current {
            root = root.child(self.content(&ARTICLES[index]));
        } else {
            root = root.child(
                div()
                    .flex_1()
                    .h_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(hsla(CONTENT_BG))
                    .text_color(hsla(MUTED))
                    .child(SharedString::from("No matching topics")),
            );
        }
        root
    }
}

fn hsla(rgb: theme::Rgb) -> gpui::Hsla {
    colors::hsla(rgb)
}

const SIDEBAR_BG: theme::Rgb = theme::Rgb::new(30, 35, 38);
const CONTENT_BG: theme::Rgb = theme::Rgb::new(35, 42, 44);
const PANEL: theme::Rgb = theme::Rgb::new(43, 52, 54);
const SEARCH_BG: theme::Rgb = theme::Rgb::new(57, 62, 65);
const FIELD_BG: theme::Rgb = theme::Rgb::new(49, 56, 58);
const FIELD_BORDER: theme::Rgb = theme::Rgb::new(76, 84, 88);
const LINE: theme::Rgb = theme::Rgb::new(61, 70, 73);
const TEXT: theme::Rgb = theme::Rgb::new(242, 244, 246);
const BODY: theme::Rgb = theme::Rgb::new(206, 212, 217);
const MUTED: theme::Rgb = theme::Rgb::new(170, 177, 181);
const BLUE: theme::Rgb = theme::Rgb::new(10, 102, 220);
