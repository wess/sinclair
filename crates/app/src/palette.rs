//! Command palette: a fuzzy launcher over the workspace's actions. Type to
//! filter, arrow keys to move, Return to run. Mirrors the other modal windows.

use gpui::prelude::*;
use gpui::{
    bounds, div, point, px, size, App, Context, FocusHandle, KeyDownEvent, SharedString,
    TitlebarOptions, WindowBounds, WindowHandle, WindowOptions,
};

use config::Action;

use crate::colors;
use crate::root::WorkspaceView;
use crate::textedit::TextEdit;

const WIDTH: f32 = 520.0;
const HEIGHT: f32 = 420.0;
const ROWS: usize = 9;

/// One launchable entry: a human label, the keybind hint if any, and the
/// action to dispatch.
pub struct Item {
    pub label: String,
    pub shortcut: Option<String>,
    pub action: Action,
}

/// Open the palette centered over `parent`.
pub fn open(parent: &gpui::Window, workspace: WindowHandle<WorkspaceView>, items: Vec<Item>, cx: &mut App) {
    let center = parent.bounds().center();
    let window_bounds = bounds(
        center - point(px(WIDTH / 2.0), px(HEIGHT / 2.0)),
        size(px(WIDTH), px(HEIGHT)),
    );
    let _ = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(window_bounds)),
            is_resizable: false,
            titlebar: Some(TitlebarOptions {
                title: Some("Command Palette".into()),
                appears_transparent: true,
                traffic_light_position: Some(point(px(12.0), px(12.0))),
            }),
            ..Default::default()
        },
        move |window, cx| {
            window.set_window_title("Command Palette");
            let view = cx.new(|cx| PaletteView::new(workspace, items, cx));
            let handle = view.read(cx).focus.clone();
            window.focus(&handle, cx);
            view
        },
    );
}

pub struct PaletteView {
    workspace: WindowHandle<WorkspaceView>,
    items: Vec<Item>,
    query: TextEdit,
    selected: usize,
    focus: FocusHandle,
}

impl PaletteView {
    fn new(workspace: WindowHandle<WorkspaceView>, items: Vec<Item>, cx: &mut Context<Self>) -> Self {
        Self {
            workspace,
            items,
            query: TextEdit::default(),
            selected: 0,
            focus: cx.focus_handle(),
        }
    }

    /// Indices of items matching the query, in catalog order. An empty query
    /// matches everything; otherwise the query must be a (case-insensitive)
    /// subsequence of the label.
    fn matches(&self) -> Vec<usize> {
        let q = self.query.text().to_lowercase();
        self.items
            .iter()
            .enumerate()
            .filter(|(_, it)| subsequence(&q, &it.label.to_lowercase()))
            .map(|(i, _)| i)
            .collect()
    }

    fn move_selection(&mut self, delta: isize, len: usize) {
        if len == 0 {
            self.selected = 0;
            return;
        }
        let n = len as isize;
        self.selected = (((self.selected as isize + delta) % n + n) % n) as usize;
    }

    fn run(&mut self, window: &mut gpui::Window, cx: &mut Context<Self>) {
        let matches = self.matches();
        let Some(&idx) = matches.get(self.selected) else {
            return;
        };
        let action = self.items[idx].action.clone();
        self.workspace
            .update(cx, |ws, window, cx| ws.run_action(action, window, cx))
            .ok();
        window.remove_window();
    }

    fn key_down(&mut self, event: &KeyDownEvent, window: &mut gpui::Window, cx: &mut Context<Self>) {
        let ks = &event.keystroke;
        if ks.modifiers.platform && ks.key == "w" {
            window.remove_window();
            cx.stop_propagation();
            return;
        }
        let len = self.matches().len();
        match ks.key.as_str() {
            "escape" => window.remove_window(),
            "enter" => self.run(window, cx),
            "up" => {
                self.move_selection(-1, len);
                cx.notify();
                cx.stop_propagation();
            }
            "down" => {
                self.move_selection(1, len);
                cx.notify();
                cx.stop_propagation();
            }
            _ => match crate::textkeys::apply(&mut self.query, ks) {
                crate::textkeys::Outcome::Submit => self.run(window, cx),
                crate::textkeys::Outcome::Cancel => window.remove_window(),
                crate::textkeys::Outcome::Edited => {
                    self.selected = 0;
                    cx.notify();
                    cx.stop_propagation();
                }
                crate::textkeys::Outcome::Pass => {}
            },
        }
    }
}

impl Render for PaletteView {
    fn render(&mut self, _window: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (before, after) = self.query.split();
        let matches = self.matches();
        let selected = self.selected.min(matches.len().saturating_sub(1));

        let mut list = div().flex().flex_col().gap(px(1.0)).mt_2();
        if matches.is_empty() {
            list = list.child(
                div()
                    .px_2()
                    .py_2()
                    .text_color(hsla(MUTED))
                    .child(SharedString::from("No matching commands")),
            );
        }
        let start = selected.saturating_sub(ROWS - 1);
        for (vis, &idx) in matches.iter().enumerate().skip(start).take(ROWS) {
            let it = &self.items[idx];
            let on = vis == selected;
            let mut bg = hsla(BLUE);
            bg.a = if on { 1.0 } else { 0.0 };
            let mut row = div()
                .h(px(34.0))
                .px_3()
                .rounded(px(7.0))
                .bg(bg)
                .flex()
                .items_center()
                .justify_between()
                .child(div().text_color(hsla(TEXT)).child(SharedString::from(it.label.clone())));
            if let Some(sc) = &it.shortcut {
                row = row.child(
                    div()
                        .px_2()
                        .py(px(2.0))
                        .rounded(px(5.0))
                        .border_1()
                        .border_color(hsla(LINE))
                        .text_size(px(12.0))
                        .text_color(hsla(MUTED))
                        .child(SharedString::from(sc.clone())),
                );
            }
            list = list.child(row);
        }

        div()
            .size_full()
            .flex()
            .flex_col()
            .px_3()
            .pt(px(40.0))
            .pb_3()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key_down))
            .bg(hsla(CONTENT_BG))
            .text_color(hsla(TEXT))
            .child(
                div()
                    .h(px(32.0))
                    .px_3()
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(hsla(BLUE))
                    .bg(hsla(FIELD_BG))
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(SharedString::from("\u{2315}"))
                    .child(SharedString::from(before))
                    .child(div().w(px(1.0)).h(px(16.0)).bg(hsla(TEXT)))
                    .child(SharedString::from(after)),
            )
            .child(list)
    }
}

/// Case-insensitive subsequence test: are all chars of `needle` found in
/// `haystack` in order? Both are expected already lowercased.
fn subsequence(needle: &str, haystack: &str) -> bool {
    let mut hay = haystack.chars();
    needle.chars().all(|c| hay.any(|h| h == c))
}

fn hsla(rgb: theme::Rgb) -> gpui::Hsla {
    colors::hsla(rgb)
}

const CONTENT_BG: theme::Rgb = theme::Rgb::new(35, 42, 44);
const FIELD_BG: theme::Rgb = theme::Rgb::new(49, 56, 58);
const LINE: theme::Rgb = theme::Rgb::new(61, 70, 73);
const TEXT: theme::Rgb = theme::Rgb::new(242, 244, 246);
const MUTED: theme::Rgb = theme::Rgb::new(170, 177, 181);
const BLUE: theme::Rgb = theme::Rgb::new(10, 102, 220);
