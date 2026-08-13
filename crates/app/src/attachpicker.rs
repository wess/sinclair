//! "Attach to Container" picker — the sibling of the "OS Tabs" picker
//! (`ospicker`). Instead of running a fresh image, it lists the containers the
//! engine already has running and attaches a tab to the one you pick (an
//! interactive shell via `exec`). A free-text field also takes a container name
//! or id directly, for one not shown.
//!
//! Like the OS picker this is a real window (not an in-window overlay) so it
//! never clips and does not depend on guise's `deferred` draw pass. The running
//! list is fetched once at construction (a blocking `docker ps`); reopen the
//! picker to re-list.

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
                title: Some("Attach to Container".into()),
                appears_transparent: true,
                traffic_light_position: Some(point(px(12.0), px(12.0))),
            }),
            ..Default::default()
        },
        |window, cx| {
            window.set_window_title("Attach to Container");
            cx.new(|cx| AttachPickerView::new(window, cx))
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

/// Attach a tab to `running` on the active workspace window (not an arbitrary
/// first one — with several windows the tab must land where the user is), then
/// close `picker`.
fn attach(app: &mut App, running: container::Running, picker: &mut Window) {
    if let Some(handle) = crate::mcpbridge::active_workspace(app) {
        handle
            .update(app, |ws, window, cx| {
                ws.attach_container(&running, window, cx)
            })
            .ok();
    }
    picker.remove_window();
}

/// Resolve typed text to a container: empty → first running; a matching
/// name/id (or id prefix) → that container; otherwise attach by the raw string
/// (the engine resolves names and short ids itself).
fn resolve(text: &str, running: &[container::Running]) -> Option<container::Running> {
    let text = text.trim();
    if text.is_empty() {
        return running.first().cloned();
    }
    Some(
        running
            .iter()
            .find(|r| {
                r.name.eq_ignore_ascii_case(text)
                    || r.id.eq_ignore_ascii_case(text)
                    || r.id.starts_with(text)
            })
            .cloned()
            .unwrap_or_else(|| container::Running {
                id: text.to_string(),
                name: text.to_string(),
                image: String::new(),
                status: String::new(),
            }),
    )
}

/// List the engine's running containers, blocking on `docker ps`. Returns
/// whether an engine is installed at all (to distinguish "none running" from
/// "no engine") plus the rows.
fn running_containers() -> (bool, Vec<container::Running>) {
    let (opts, _) = config::load();
    let Some(engine) = container::Engine::resolve(opts.container_engine.as_deref()) else {
        return (false, Vec::new());
    };
    let argv = container::ps_argv(engine);
    let rows = match std::process::Command::new(&argv[0])
        .args(&argv[1..])
        .output()
    {
        Ok(out) if out.status.success() => {
            container::parse_ps(&String::from_utf8_lossy(&out.stdout))
        }
        _ => Vec::new(),
    };
    (true, rows)
}

pub struct AttachPickerView {
    available: bool,
    running: Vec<container::Running>,
    input: Entity<TextInput>,
    focus: FocusHandle,
    _submit: Subscription,
}

impl AttachPickerView {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (available, running) = running_containers();

        let input = cx.new(|cx| TextInput::new(cx).placeholder("or type a container name or id"));
        let focus = cx.focus_handle();

        // Focus the field after the first paint. Focusing here during
        // construction is dropped - the input element does not exist yet - which
        // is why the field opened looking inert and swallowing keystrokes.
        let input_focus = input.read(cx).focus_handle();
        window.on_next_frame(move |window, cx| window.focus(&input_focus, cx));

        let submit = {
            let running = running.clone();
            window.subscribe(&input, cx, move |_input, event, window, app| {
                if let TextInputEvent::Submit(text) = event {
                    if let Some(r) = resolve(text, &running) {
                        attach(app, r, window);
                    }
                }
            })
        };

        Self {
            available,
            running,
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

impl Render for AttachPickerView {
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
                    .child("Attach to Container"),
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
            .id("attach-list")
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(5.0));
        if self.running.is_empty() {
            list = list.child(
                div()
                    .text_size(px(13.0))
                    .text_color(dim)
                    .child("No running containers. Start one, or type a name below."),
            );
        }
        for (i, r) in self.running.iter().enumerate() {
            let running = r.clone();
            let name = if r.name.is_empty() {
                r.id.clone()
            } else {
                r.name.clone()
            };
            let mut row = div()
                .id(("attach-row", i))
                .flex()
                .items_center()
                .gap(px(8.0))
                .px(px(12.0))
                .py(px(9.0))
                .rounded(px(7.0))
                .bg(surface)
                .border_1()
                .border_color(border)
                .hover(move |s| s.border_color(text))
                .text_size(px(13.0))
                .on_click(move |_ev: &ClickEvent, window, app| {
                    attach(app, running.clone(), window);
                })
                .child(
                    div()
                        .flex_1()
                        .child(format!("{name}  \u{00b7}  {}", r.image)),
                );
            if !r.status.is_empty() {
                row = row.child(
                    div()
                        .text_size(px(11.0))
                        .text_color(dim)
                        .child(r.status.clone()),
                );
            }
            list = list.child(row);
        }

        root.child(div().text_size(px(11.0)).text_color(dim).child("RUNNING"))
            .child(list)
            .child(self.input.clone())
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(dim)
                    .child("Click a container or press Return \u{2022} Esc to cancel"),
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
