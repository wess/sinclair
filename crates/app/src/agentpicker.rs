//! "New Agent" picker — a small standalone window (like the New OS Tab picker)
//! to configure and launch an AI agent in a split of the main workspace. A real
//! window (rather than an in-window overlay) avoids clipping and does not depend
//! on guise's `deferred` draw pass, which made the in-window modal fragile.
//!
//! Creating queues a `relay launch` command that the main workspace turns into a
//! split, saves the agent definition, and closes the picker.

use gpui::prelude::*;
use gpui::{
    bounds, div, point, px, size, App, Context, Entity, FocusHandle, FontWeight, KeyDownEvent,
    MouseButton, Subscription, TitlebarOptions, Window, WindowBounds, WindowControlArea,
    WindowOptions,
};

use guise::{
    Button, SegmentedControl, SegmentedControlEvent, Select, TextInput, TextInputEvent, Variant,
};

use crate::root::WorkspaceView;

const WIDTH: f32 = 460.0;
const HEIGHT: f32 = 430.0;

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
                title: Some("New Agent".into()),
                appears_transparent: true,
                traffic_light_position: Some(point(px(12.0), px(12.0))),
            }),
            ..Default::default()
        },
        |window, cx| {
            window.set_window_title("New Agent");
            cx.new(|cx| AgentPickerView::new(window, cx))
        },
    );
    // Make the new window the key window so its fields receive input.
    if let Ok(handle) = handle {
        handle
            .update(cx, |view, window, cx| {
                window.activate_window();
                window.focus(&view.name.read(cx).focus_handle(), cx);
            })
            .ok();
    }
}

/// Run `cmd` in a new split on the main workspace window, then close `picker`.
fn create(app: &mut App, cmd: String, picker: &mut Window) {
    if let Some(handle) = app
        .windows()
        .into_iter()
        .find_map(|w| w.downcast::<WorkspaceView>())
    {
        handle
            .update(app, |ws, window, cx| ws.create_agent(&cmd, window, cx))
            .ok();
    }
    picker.remove_window();
}

pub struct AgentPickerView {
    opts: config::Options,
    providers: Vec<String>,
    roles: Vec<String>,
    provider: Entity<Select>,
    name: Entity<TextInput>,
    kind: Entity<SegmentedControl>,
    role: Entity<Select>,
    desc: Entity<TextInput>,
    /// True when the "Custom" tab is selected (free-form description).
    custom: bool,
    focus: FocusHandle,
    _subs: Vec<Subscription>,
}

impl AgentPickerView {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (opts, _) = config::load();
        let providers = crate::relay::enabled_agents(&opts);
        let roles = crate::relay::role_list();
        let custom = roles.is_empty();

        let provider = cx.new(|cx| {
            Select::new(cx)
                .label("Provider")
                .placeholder("none enabled")
                .data(providers.clone())
        });
        let name = cx.new(|cx| TextInput::new(cx).label("Name").placeholder("agent name"));
        let kind = cx.new(|cx| {
            SegmentedControl::new(cx)
                .data(["Preset", "Custom"])
                .selected(if custom { 1 } else { 0 })
        });
        let role = cx.new(|cx| Select::new(cx).label("Role").data(roles.clone()));
        let desc =
            cx.new(|cx| TextInput::new(cx).label("Describe").placeholder("what this agent does"));
        let focus = cx.focus_handle();

        // Focus the name field after the first paint. Focusing during
        // construction is dropped - the input element does not exist yet.
        let name_focus = name.read(cx).focus_handle();
        window.on_next_frame(move |window, cx| window.focus(&name_focus, cx));

        let me = cx.entity().downgrade();
        let mut subs = Vec::new();
        subs.push(cx.subscribe(&kind, |this, _src, event: &SegmentedControlEvent, cx| {
            this.custom = event.0 == 1;
            cx.notify();
        }));
        for field in [&name, &desc] {
            let me = me.clone();
            subs.push(window.subscribe(field, cx, move |_src, event, window, app| {
                if let TextInputEvent::Submit(_) = event {
                    me.update(app, |this, cx| this.commit(window, cx)).ok();
                }
            }));
        }

        Self {
            opts,
            providers,
            roles,
            provider,
            name,
            kind,
            role,
            desc,
            custom,
            focus,
            _subs: subs,
        }
    }

    fn commit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.name.read(cx).text().trim().to_string();
        if name.is_empty() || self.providers.is_empty() {
            window.remove_window();
            return;
        }
        let pi = self.provider.read(cx).selected_index().unwrap_or(0);
        let provider = self.providers[pi.min(self.providers.len() - 1)].clone();
        let (role, task) = if self.custom {
            (None, Some(self.desc.read(cx).text().trim().to_string()))
        } else {
            let ri = self.role.read(cx).selected_index().unwrap_or(0);
            (self.roles.get(ri).cloned(), None)
        };
        crate::relay::save_agent_def(crate::relay::AgentDef {
            name: name.clone(),
            provider: provider.clone(),
            role: role.clone(),
            task: task.clone(),
        });
        let cmd = crate::relay::launch_agent_command(
            &self.opts,
            &provider,
            &name,
            role.as_deref(),
            task.as_deref(),
        );
        create(cx, cmd, window);
    }

    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, _cx: &mut Context<Self>) {
        if event.keystroke.key == "escape" {
            window.remove_window();
        }
    }
}

impl Render for AgentPickerView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Pull colors from the same guise theme the embedded fields use.
        let t = guise::theme(cx);
        let bg = t.body().hsla();
        let text = t.text().hsla();
        let dim = t.dimmed().hsla();

        let role_row = if self.custom {
            self.desc.clone().into_any_element()
        } else {
            self.role.clone().into_any_element()
        };

        let type_row = div()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(div().text_size(px(12.0)).text_color(dim).child("Type"))
            .child(self.kind.clone());

        let me = cx.entity().downgrade();
        let footer = div()
            .flex()
            .justify_end()
            .gap(px(8.0))
            .child(
                Button::new("agent-cancel", "Cancel")
                    .variant(Variant::Default)
                    .on_click(move |_ev, window, _app| window.remove_window()),
            )
            .child(
                Button::new("agent-create", "Create")
                    .variant(Variant::Filled)
                    .on_click(move |_ev, window, app| {
                        me.update(app, |this, cx| this.commit(window, cx)).ok();
                    }),
            );

        div()
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
            .gap(px(12.0))
            .child(drag_strip())
            .child(
                div()
                    .text_size(px(15.0))
                    .font_weight(FontWeight::BOLD)
                    .child("New Agent"),
            )
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(dim)
                    .child("Run an AI agent in a new split of this workspace."),
            )
            .child(self.provider.clone())
            .child(self.name.clone())
            .child(type_row)
            .child(role_row)
            .child(footer)
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(dim)
                    .child("Press Return to create \u{2022} Esc to cancel"),
            )
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
