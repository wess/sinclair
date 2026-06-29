//! Modal to add an agent to the current workspace: pick a provider, name it,
//! and either choose a role preset or describe a custom one. On create it
//! queues a `relay launch` command that the workspace turns into a split.

use gpui::prelude::*;
use gpui::{
    bounds, div, point, px, size, App, Context, FocusHandle, FontWeight, Hsla, KeyDownEvent,
    MouseButton, SharedString, TitlebarOptions, Window, WindowBounds, WindowControlArea,
    WindowHandle, WindowOptions,
};

use crate::colors;
use crate::root::WorkspaceView;
use crate::textedit::TextEdit;

const WIDTH: f32 = 460.0;
const HEIGHT: f32 = 372.0;

#[derive(PartialEq)]
enum Active {
    Name,
    Desc,
}

pub struct NewAgentView {
    workspace: WindowHandle<WorkspaceView>,
    opts: config::Options,
    providers: Vec<String>,
    provider: usize,
    name: TextEdit,
    custom: bool,
    roles: Vec<String>,
    role: usize,
    desc: TextEdit,
    active: Active,
    focus: FocusHandle,
}

pub fn open(
    parent: &Window,
    workspace: WindowHandle<WorkspaceView>,
    opts: config::Options,
    providers: Vec<String>,
    roles: Vec<String>,
    cx: &mut App,
) {
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
                title: Some("New Agent".into()),
                appears_transparent: true,
                traffic_light_position: Some(point(px(12.0), px(12.0))),
            }),
            ..Default::default()
        },
        move |window, cx| {
            window.set_window_title("New Agent");
            let view = cx.new(|cx| NewAgentView::new(workspace, opts, providers, roles, cx));
            let handle = view.read(cx).focus.clone();
            window.focus(&handle, cx);
            view
        },
    );
}

impl NewAgentView {
    fn new(
        workspace: WindowHandle<WorkspaceView>,
        opts: config::Options,
        providers: Vec<String>,
        roles: Vec<String>,
        cx: &mut Context<Self>,
    ) -> Self {
        let custom = roles.is_empty();
        Self {
            workspace,
            opts,
            providers,
            provider: 0,
            name: TextEdit::new(""),
            custom,
            roles,
            role: 0,
            desc: TextEdit::new(""),
            active: Active::Name,
            focus: cx.focus_handle(),
        }
    }

    fn commit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.name.text().trim().to_string();
        if name.is_empty() || self.providers.is_empty() {
            window.remove_window();
            return;
        }
        let provider = self.providers[self.provider.min(self.providers.len() - 1)].clone();
        let (role, task) = if self.custom {
            (None, Some(self.desc.text().trim().to_string()))
        } else {
            (self.roles.get(self.role).cloned(), None)
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
        self.workspace
            .update(cx, |ws, window, cx| ws.create_agent(&cmd, window, cx))
            .ok();
        window.remove_window();
    }

    fn field(&mut self) -> &mut TextEdit {
        if self.custom && self.active == Active::Desc {
            &mut self.desc
        } else {
            &mut self.name
        }
    }

    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let ks = &event.keystroke;
        if ks.modifiers.platform && ks.key == "w" {
            window.remove_window();
            cx.stop_propagation();
            return;
        }
        if ks.key == "tab" {
            self.active = if self.active == Active::Name {
                Active::Desc
            } else {
                Active::Name
            };
            cx.notify();
            cx.stop_propagation();
            return;
        }
        match crate::textkeys::apply(self.field(), ks) {
            crate::textkeys::Outcome::Submit => self.commit(window, cx),
            crate::textkeys::Outcome::Cancel => window.remove_window(),
            crate::textkeys::Outcome::Edited => {
                cx.notify();
                cx.stop_propagation();
            }
            crate::textkeys::Outcome::Pass => {}
        }
    }

    /// A macOS pull-down-style popup: the current value in a rounded field
    /// with a small attached ‹ › stepper to walk through the choices.
    fn cycle(&self, value: String, which: Cycle, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .min_w(px(150.0))
                    .h(px(28.0))
                    .px_3()
                    .rounded(px(7.0))
                    .border_1()
                    .border_color(hsla(FIELD_BORDER))
                    .bg(hsla(FIELD_BG))
                    .flex()
                    .items_center()
                    .text_color(hsla(TEXT))
                    .child(SharedString::from(value)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .rounded(px(7.0))
                    .border_1()
                    .border_color(hsla(FIELD_BORDER))
                    .bg(hsla(FIELD_BG))
                    .overflow_hidden()
                    .child(step_button("\u{2039}", which, -1, cx))
                    .child(div().w(px(1.0)).h(px(18.0)).bg(hsla(FIELD_BORDER)))
                    .child(step_button("\u{203a}", which, 1, cx)),
            )
    }

    /// A macOS-style segmented control choosing between a role preset and a
    /// free-form custom description.
    fn segmented(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .p(px(2.0))
            .rounded(px(8.0))
            .bg(hsla(TRACK))
            .child(segment("Preset", !self.custom, !self.roles.is_empty(), cx))
            .child(segment("Custom", self.custom, true, cx))
    }

    fn text_box(&self, which: Active, value: String, placeholder: &str, cx: &mut Context<Self>) -> impl IntoElement {
        let active = self.active == which && (which == Active::Name || self.custom);
        let edit = if which == Active::Name { &self.name } else { &self.desc };
        let border = hsla(if active { BLUE } else { FIELD_BORDER });
        let mut field = div()
            .w(px(238.0))
            .h(px(28.0))
            .px_3()
            .rounded(px(7.0))
            .border_1()
            .border_color(border)
            .bg(hsla(FIELD_BG))
            .flex()
            .items_center();
        if active {
            let (before, after) = edit.split();
            field = field
                .text_color(hsla(TEXT))
                .child(SharedString::from(before))
                .child(div().w(px(1.0)).h(px(16.0)).bg(hsla(BLUE)))
                .child(SharedString::from(after));
        } else {
            let empty = value.is_empty();
            field = field
                .text_color(hsla(if empty { MUTED } else { TEXT }))
                .child(SharedString::from(if empty {
                    placeholder.to_string()
                } else {
                    value
                }))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _ev, window, cx| {
                        this.active = if which == Active::Name { Active::Name } else { Active::Desc };
                        window.focus(&this.focus, cx);
                        cx.notify();
                    }),
                );
        }
        field
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Cycle {
    Provider,
    Role,
}

/// One arrow of a [`NewAgentView::cycle`] stepper.
fn step_button(glyph: &str, which: Cycle, dir: i32, cx: &mut Context<NewAgentView>) -> gpui::Stateful<gpui::Div> {
    let id = match (which, dir < 0) {
        (Cycle::Provider, true) => "step-provider-prev",
        (Cycle::Provider, false) => "step-provider-next",
        (Cycle::Role, true) => "step-role-prev",
        (Cycle::Role, false) => "step-role-next",
    };
    div()
        .id(id)
        .w(px(26.0))
        .h(px(26.0))
        .flex()
        .items_center()
        .justify_center()
        .text_color(hsla(MUTED))
        .hover(|s| s.bg(hsla(HOVER)).text_color(hsla(TEXT)))
        .child(SharedString::from(glyph.to_string()))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _ev, _window, cx| {
                match which {
                    Cycle::Provider => {
                        let n = this.providers.len();
                        if n > 0 {
                            this.provider = (this.provider as i32 + dir).rem_euclid(n as i32) as usize;
                        }
                    }
                    Cycle::Role => {
                        let n = this.roles.len();
                        if n > 0 {
                            this.role = (this.role as i32 + dir).rem_euclid(n as i32) as usize;
                        }
                    }
                }
                cx.notify();
                cx.stop_propagation();
            }),
        )
}

/// One side of the Preset/Custom segmented control.
fn segment(label: &str, on: bool, enabled: bool, cx: &mut Context<NewAgentView>) -> gpui::Div {
    let custom = label == "Custom";
    let mut s = div()
        .h(px(24.0))
        .px_3()
        .rounded(px(6.0))
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(12.0))
        .child(SharedString::from(label.to_string()));
    if on {
        s = s.bg(hsla(SEG_ON)).text_color(hsla(TEXT)).font_weight(FontWeight::MEDIUM);
    } else {
        s = s.text_color(hsla(MUTED));
    }
    if enabled {
        s = s.on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _ev, _window, cx| {
                this.custom = custom;
                cx.notify();
                cx.stop_propagation();
            }),
        );
    }
    s
}

impl Render for NewAgentView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let provider = self
            .providers
            .get(self.provider)
            .cloned()
            .unwrap_or_else(|| "none enabled".to_string());
        let role = self
            .roles
            .get(self.role)
            .cloned()
            .unwrap_or_else(|| "(none)".to_string());

        let role_row = if self.custom {
            card_row(
                "Describe",
                self.text_box(Active::Desc, self.desc.text(), "what this agent does", cx)
                    .into_any_element(),
                true,
            )
        } else {
            card_row("Role", self.cycle(role, Cycle::Role, cx).into_any_element(), true)
        };

        let card = div()
            .flex()
            .flex_col()
            .rounded(px(10.0))
            .border_1()
            .border_color(hsla(HAIRLINE))
            .bg(hsla(PANEL))
            .overflow_hidden()
            .child(card_row(
                "Provider",
                self.cycle(provider, Cycle::Provider, cx).into_any_element(),
                false,
            ))
            .child(card_row(
                "Name",
                self.text_box(Active::Name, self.name.text(), "agent name", cx).into_any_element(),
                false,
            ))
            .child(card_row("Type", self.segmented(cx).into_any_element(), false))
            .child(role_row);

        let header = div()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .child(
                div()
                    .text_size(px(16.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(hsla(TEXT))
                    .child(SharedString::from("Define Agent")),
            )
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(hsla(MUTED))
                    .child(SharedString::from("Run an AI agent in a new split of this workspace.")),
            );

        let footer = div()
            .flex()
            .justify_end()
            .gap_2()
            .child(
                push_button("Cancel", false).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|_this, _ev, window, _cx| window.remove_window()),
                ),
            )
            .child(
                push_button("Create", true).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _ev, window, cx| this.commit(window, cx)),
                ),
            );

        div()
            .size_full()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key_down))
            .bg(hsla(CONTENT_BG))
            .text_color(hsla(TEXT))
            .flex()
            .flex_col()
            .child(drag_strip())
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .px_5()
                    .pt(px(38.0))
                    .pb_5()
                    .child(header)
                    .child(card)
                    .child(div().flex_1())
                    .child(footer),
            )
    }
}

/// Drag handle across the transparent titlebar so the modal can be moved.
fn drag_strip() -> impl IntoElement {
    let lead = if cfg!(target_os = "macos") { 78.0 } else { 0.0 };
    div()
        .absolute()
        .top_0()
        .left(px(lead))
        .w(px(WIDTH - lead))
        .h(px(30.0))
        .window_control_area(WindowControlArea::Drag)
        .on_mouse_down(MouseButton::Left, |_, window, _| window.start_window_move())
}

/// A grouped-list row: muted label on the left, control on the right, with a
/// hairline separator unless it is the last row of the card.
fn card_row(label: &str, control: gpui::AnyElement, last: bool) -> gpui::Div {
    let mut r = div()
        .h(px(46.0))
        .px_4()
        .flex()
        .items_center()
        .justify_between();
    if !last {
        r = r.border_b_1().border_color(hsla(HAIRLINE));
    }
    r.child(
        div()
            .text_color(hsla(TEXT))
            .child(SharedString::from(label.to_string())),
    )
    .child(control)
}

/// A macOS push button. `primary` renders the accent-filled default button.
fn push_button(label: &str, primary: bool) -> gpui::Div {
    let mut b = div()
        .h(px(28.0))
        .px_4()
        .rounded(px(7.0))
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(13.0))
        .child(SharedString::from(label.to_string()));
    if primary {
        b = b
            .bg(hsla(BLUE))
            .text_color(hsla(WHITE))
            .font_weight(FontWeight::MEDIUM);
    } else {
        b = b
            .bg(hsla(FIELD_BG))
            .border_1()
            .border_color(hsla(FIELD_BORDER))
            .text_color(hsla(TEXT));
    }
    b
}

fn hsla(rgb: theme::Rgb) -> Hsla {
    colors::hsla(rgb)
}

const CONTENT_BG: theme::Rgb = theme::Rgb::new(35, 42, 44);
const PANEL: theme::Rgb = theme::Rgb::new(44, 51, 54);
const HAIRLINE: theme::Rgb = theme::Rgb::new(62, 70, 74);
const TRACK: theme::Rgb = theme::Rgb::new(28, 34, 36);
const SEG_ON: theme::Rgb = theme::Rgb::new(64, 72, 76);
const HOVER: theme::Rgb = theme::Rgb::new(60, 68, 72);
const FIELD_BG: theme::Rgb = theme::Rgb::new(49, 56, 58);
const FIELD_BORDER: theme::Rgb = theme::Rgb::new(76, 84, 88);
const TEXT: theme::Rgb = theme::Rgb::new(242, 244, 246);
const MUTED: theme::Rgb = theme::Rgb::new(170, 177, 181);
const BLUE: theme::Rgb = theme::Rgb::new(10, 102, 220);
const WHITE: theme::Rgb = theme::Rgb::new(255, 255, 255);
