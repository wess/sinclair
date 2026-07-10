//! "New OS Tab" picker — a small standalone window (like Settings) listing OS
//! images to run fresh as a container-backed tab, plus a field for an arbitrary
//! image. A real window (rather than an in-window overlay) avoids clipping and
//! does not depend on guise's `deferred` draw pass.
//!
//! Selecting an image launches the container on the main workspace window and
//! closes the picker.

use gpui::prelude::*;
use gpui::{
    bounds, div, point, px, size, App, ClickEvent, Context, Entity, FocusHandle, FontWeight,
    KeyDownEvent, MouseButton, Subscription, TitlebarOptions, Window, WindowBounds,
    WindowControlArea, WindowOptions,
};

use guise::{TextInput, TextInputEvent};

const WIDTH: f32 = 380.0;
const HEIGHT: f32 = 440.0;

/// Open the picker window, centered over `parent`.
pub fn open(parent: &Window, cx: &mut App) {
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
                title: Some("New OS Tab".into()),
                appears_transparent: true,
                traffic_light_position: Some(point(px(12.0), px(12.0))),
            }),
            ..Default::default()
        },
        |window, cx| {
            window.set_window_title("New OS Tab");
            cx.new(|cx| OsPickerView::new(window, cx))
        },
    );
    // Make the new window the key window so its text field receives input.
    if let Ok(handle) = handle {
        handle
            .update(cx, |view, window, cx| {
                window.activate_window();
                window.focus(&view.input.read(cx).focus_handle(), cx);
            })
            .ok();
    }
}

/// Run `profile` in a new tab on the active workspace window (not an arbitrary
/// first one — with several windows the container must land where the user
/// is), then close `picker`.
fn launch(app: &mut App, profile: container::Profile, picker: &mut Window) {
    if let Some(handle) = crate::mcpbridge::active_workspace(app) {
        handle
            .update(app, |ws, window, cx| {
                ws.launch_container(&profile, window, cx)
            })
            .ok();
    }
    picker.remove_window();
}

/// Resolve typed text to a profile: empty → first profile; a matching
/// label/image → that profile; otherwise a one-off profile for the typed image.
fn resolve(text: &str, profiles: &[container::Profile]) -> Option<container::Profile> {
    if text.is_empty() {
        return profiles.first().cloned();
    }
    Some(
        profiles
            .iter()
            .find(|p| p.label.eq_ignore_ascii_case(text) || p.image.eq_ignore_ascii_case(text))
            .cloned()
            .unwrap_or_else(|| container::Profile {
                label: text.to_string(),
                image: text.to_string(),
                command: "bash".to_string(),
                persist: None,
            }),
    )
}

pub struct OsPickerView {
    available: bool,
    profiles: Vec<container::Profile>,
    input: Entity<TextInput>,
    focus: FocusHandle,
    _submit: Subscription,
}

impl OsPickerView {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (opts, _) = config::load();
        let available = container::Engine::resolve(opts.container_engine.as_deref()).is_some();
        let (profiles, _) = container::profiles(&opts.container);

        let input =
            cx.new(|cx| TextInput::new(cx).placeholder("or type an image, e.g. debian:bookworm"));
        let focus = cx.focus_handle();

        // Focus the field after the first paint. Focusing here during
        // construction is dropped - the input element does not exist yet - which
        // is why the field opened looking inert and swallowing keystrokes.
        let input_focus = input.read(cx).focus_handle();
        window.on_next_frame(move |window, cx| window.focus(&input_focus, cx));

        let submit = {
            let profiles = profiles.clone();
            window.subscribe(&input, cx, move |_input, event, window, app| {
                if let TextInputEvent::Submit(text) = event {
                    if let Some(p) = resolve(text.trim(), &profiles) {
                        launch(app, p, window);
                    }
                }
            })
        };

        Self {
            available,
            profiles,
            input,
            focus,
            _submit: submit,
        }
    }

    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, _cx: &mut Context<Self>) {
        if event.keystroke.key == "escape" {
            window.remove_window();
        }
    }
}

impl Render for OsPickerView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Pull colors from the same guise theme the embedded TextInput uses, so
        // the field and the rest of the dialog track one palette.
        let t = guise::theme(cx);
        let bg = t.body().hsla();
        let surface = t.surface().hsla();
        let border = t.border().hsla();
        let text = t.text().hsla();
        let dim = t.dimmed().hsla();

        let root = div()
            .size_full()
            .flex()
            .flex_col()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key_down))
            .bg(bg)
            .text_color(text)
            .pt(px(34.0)) // clear the transparent titlebar
            .px(px(16.0))
            .pb(px(16.0))
            .gap(px(10.0))
            .child(drag_strip())
            .child(
                div()
                    .text_size(px(15.0))
                    .font_weight(FontWeight::BOLD)
                    .child("New OS Tab"),
            );

        if !self.available {
            return root
                .child(
                    div()
                        .text_size(px(13.0))
                        .child("No container engine found. Install Docker or Podman."),
                )
                .into_any_element();
        }

        let mut list = div()
            .id("os-list")
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(5.0));
        for (i, p) in self.profiles.iter().enumerate() {
            let profile = p.clone();
            list = list.child(
                div()
                    .id(("os-row", i))
                    .flex()
                    .items_center()
                    .px(px(12.0))
                    .py(px(9.0))
                    .rounded(px(7.0))
                    .bg(surface)
                    .border_1()
                    .border_color(border)
                    .hover(move |s| s.border_color(text))
                    .text_size(px(13.0))
                    .on_click(move |_ev: &ClickEvent, window, app| {
                        launch(app, profile.clone(), window);
                    })
                    .child(format!("{}  \u{00b7}  {}", p.label, p.image)),
            );
        }

        root.child(div().text_size(px(11.0)).text_color(dim).child("RUN FRESH"))
            .child(list)
            .child(self.input.clone())
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(dim)
                    .child("Click an image or press Return \u{2022} Esc to cancel"),
            )
            .into_any_element()
    }
}

/// Drag handle across the transparent titlebar so the window can be moved.
fn drag_strip() -> impl IntoElement {
    let lead = if cfg!(target_os = "macos") { 70.0 } else { 0.0 };
    div()
        .absolute()
        .top_0()
        .left(px(lead))
        .w(px(WIDTH - lead))
        .h(px(28.0))
        .window_control_area(WindowControlArea::Drag)
        .on_mouse_down(MouseButton::Left, |_, window, _| window.start_window_move())
}
