//! Team Builder — a standalone window (like the New Agent picker) to assemble a
//! Relay team and save it via `relay team save`. Two modes: **Manual** (pick the
//! layout and add member rows by hand) and **Guided** (describe the team in
//! plain English; Claude drafts it, and the draft drops into the Manual form for
//! review before saving).

use std::path::PathBuf;

use gpui::prelude::*;
use gpui::{
    bounds, div, point, px, size, App, Context, Entity, FocusHandle, FontWeight, KeyDownEvent,
    MouseButton, SharedString, Subscription, TitlebarOptions, Window, WindowBounds,
    WindowControlArea, WindowOptions,
};
use guise::{Button, SegmentedControl, SegmentedControlEvent, Select, TextInput, TextInputEvent, Variant};

use crate::relay::{TeamMemberSpec, TeamSpec, TEAM_SHAPES};
use crate::root::WorkspaceView;

const WIDTH: f32 = 520.0;
const HEIGHT: f32 = 620.0;

/// Open the builder window, centered over `parent`. `project_dir` is the focused
/// pane's directory, used when saving a project-scoped team.
pub fn open(parent: &Window, project_dir: Option<PathBuf>, cx: &mut App) {
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
                title: Some("Team Builder".into()),
                appears_transparent: true,
                traffic_light_position: Some(point(px(12.0), px(12.0))),
            }),
            ..Default::default()
        },
        |window, cx| {
            window.set_window_title("Team Builder");
            cx.new(|cx| TeamBuilderView::new(project_dir, window, cx))
        },
    );
    if let Ok(handle) = handle {
        handle
            .update(cx, |view, window, cx| {
                window.activate_window();
                window.focus(&view.name.read(cx).focus_handle(), cx);
            })
            .ok();
    }
}

/// One editable member: a name plus role and agent-provider pickers.
struct MemberRow {
    name: Entity<TextInput>,
    role: Entity<Select>,
    provider: Entity<Select>,
}

pub struct TeamBuilderView {
    opts: config::Options,
    roles: Vec<String>,
    /// Provider choices; index 0 is "default" (no explicit agent override).
    providers: Vec<String>,
    project_dir: Option<PathBuf>,
    mode: Entity<SegmentedControl>,
    /// True while the Guided tab is active.
    guided: bool,
    /// True once a team exists to edit (built manually or generated), so the
    /// form shows even in Guided mode for review.
    have_form: bool,
    /// True while a Guided generation is running off-thread.
    busy: bool,
    /// True while a save is running off-thread (`relay team save` blocks).
    saving: bool,
    status: Option<String>,
    name: Entity<TextInput>,
    layout: Entity<Select>,
    scope: Entity<SegmentedControl>,
    members: Vec<MemberRow>,
    desc: Entity<TextInput>,
    focus: FocusHandle,
    _subs: Vec<Subscription>,
}

impl TeamBuilderView {
    fn new(project_dir: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (opts, _) = config::load();
        // Seed with the built-in roles so the window opens instantly; the real
        // list (a `relay role list` subprocess) loads off-thread and swaps in.
        let roles: Vec<String> = fallback_roles();
        let mut providers = vec!["default".to_string()];
        providers.extend(crate::relay::enabled_agents(&opts));
        Self::load_roles(cx);

        let mode = cx.new(|cx| SegmentedControl::new(cx).data(["Manual", "Guided"]).selected(0));
        let name =
            cx.new(|cx| TextInput::new(cx).label("Team name").placeholder("my-team"));
        let layout = cx.new(|cx| Select::new(cx).label("Layout").data(shape_list()).selected(0));
        let scope = cx.new(|cx| SegmentedControl::new(cx).data(["User", "Project"]).selected(0));
        let desc = cx.new(|cx| {
            TextInput::new(cx)
                .label("Describe the team")
                .placeholder("e.g. build and review a REST API with a frontend")
        });
        let focus = cx.focus_handle();

        // Seed one member so the manual form is usable immediately.
        let members = vec![member_row(cx, &roles, &providers, "lead", Some("supervisor"), None)];

        let me = cx.entity().downgrade();
        let mut subs = Vec::new();
        subs.push(cx.subscribe(&mode, |this, _src, ev: &SegmentedControlEvent, cx| {
            this.guided = ev.0 == 1;
            cx.notify();
        }));
        subs.push(window.subscribe(&desc, cx, move |_src, ev, window, app| {
            if let TextInputEvent::Submit(_) = ev {
                me.update(app, |this, cx| this.generate(window, cx)).ok();
            }
        }));

        Self {
            opts,
            roles,
            providers,
            project_dir,
            mode,
            guided: false,
            have_form: true,
            busy: false,
            saving: false,
            status: None,
            name,
            layout,
            scope,
            members,
            desc,
            focus,
            _subs: subs,
        }
    }

    /// Load the real role list off the UI thread (`relay role list` spawns a
    /// subprocess) and swap it into the form when it arrives.
    fn load_roles(cx: &mut Context<Self>) {
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let roles = executor.spawn(async { crate::relay::role_list() }).await;
            if roles.is_empty() {
                return;
            }
            let _ = this.update(cx, |view, cx| view.set_roles(roles, cx));
        })
        .detach();
    }

    /// Replace the role choices, rebuilding each member row's role picker while
    /// preserving its current selection by name.
    fn set_roles(&mut self, roles: Vec<String>, cx: &mut Context<Self>) {
        if roles == self.roles {
            return;
        }
        for row in &mut self.members {
            let current = row
                .role
                .read(cx)
                .selected_index()
                .and_then(|i| self.roles.get(i))
                .cloned();
            let ri = current
                .and_then(|name| roles.iter().position(|r| *r == name))
                .unwrap_or(0);
            row.role = cx.new({
                let data = roles.clone();
                move |cx| Select::new(cx).label("Role").data(data).selected(ri)
            });
        }
        self.roles = roles;
        cx.notify();
    }

    /// Replace the form fields with a generated (or otherwise supplied) team,
    /// then drop into review by showing the form.
    fn apply_spec(&mut self, spec: TeamSpec, cx: &mut Context<Self>) {
        self.name = cx.new({
            let v = spec.name.clone();
            move |cx| TextInput::new(cx).label("Team name").placeholder("my-team").value(&v)
        });
        let li = TEAM_SHAPES.iter().position(|s| *s == spec.layout).unwrap_or(0);
        self.layout = cx.new(|cx| Select::new(cx).label("Layout").data(shape_list()).selected(li));
        self.members = spec
            .members
            .iter()
            .map(|m| member_row(cx, &self.roles, &self.providers, &m.name, m.role.as_deref(), m.agent.as_deref()))
            .collect();
        if self.members.is_empty() {
            self.members.push(member_row(cx, &self.roles, &self.providers, "lead", Some("supervisor"), None));
        }
        self.have_form = true;
    }

    /// Kick off a Guided generation off the UI thread.
    fn generate(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let desc = self.desc.read(cx).text().trim().to_string();
        if desc.is_empty() {
            self.status = Some("Describe the team first.".into());
            cx.notify();
            return;
        }
        self.busy = true;
        self.status = Some("Generating with Claude…".into());
        cx.notify();
        let executor = cx.background_executor().clone();
        let opts = self.opts.clone();
        let roles = self.roles.clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move { crate::relay::generate_team(&opts, &roles, &desc) })
                .await;
            let _ = this.update(cx, |view, cx| {
                view.busy = false;
                match result {
                    Ok(spec) => {
                        view.apply_spec(spec, cx);
                        view.status = Some("Review the draft, then Save.".into());
                    }
                    Err(e) => view.status = Some(e),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn add_member(&mut self, cx: &mut Context<Self>) {
        let row = member_row(cx, &self.roles, &self.providers, "", None, None);
        self.members.push(row);
        cx.notify();
    }

    /// Collect the form into a [`TeamSpec`], save it via relay off the UI
    /// thread (the save is a subprocess), and on success refresh the AI menu
    /// and close. Errors surface in the status line.
    fn commit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.saving {
            return;
        }
        let name = self.name.read(cx).text().trim().to_string();
        if name.is_empty() {
            self.status = Some("Team name is required.".into());
            cx.notify();
            return;
        }
        let li = self.layout.read(cx).selected_index().unwrap_or(0);
        let layout = TEAM_SHAPES.get(li).copied().unwrap_or("columns").to_string();
        let members: Vec<TeamMemberSpec> = self
            .members
            .iter()
            .filter_map(|row| {
                let mname = row.name.read(cx).text().trim().to_string();
                if mname.is_empty() {
                    return None;
                }
                let ri = row.role.read(cx).selected_index().unwrap_or(0);
                let role = self.roles.get(ri).cloned();
                let pi = row.provider.read(cx).selected_index().unwrap_or(0);
                let agent = if pi == 0 { None } else { self.providers.get(pi).cloned() };
                Some(TeamMemberSpec { name: mname, role, agent })
            })
            .collect();
        if members.is_empty() {
            self.status = Some("Add at least one member.".into());
            cx.notify();
            return;
        }
        let user = self.scope.read(cx).selected_index() == 0;
        let spec = TeamSpec { name, layout, members };
        let Some(handle) = window.window_handle().downcast::<Self>() else {
            return;
        };
        self.saving = true;
        self.status = Some("Saving\u{2026}".into());
        cx.notify();
        let project_dir = self.project_dir.clone();
        let executor = cx.background_executor().clone();
        cx.spawn(async move |_this, cx| {
            let result = executor
                .spawn(async move { crate::relay::save_team(&spec, user, project_dir.as_deref()) })
                .await;
            let _ = handle.update(cx, |view, window, cx| {
                view.saving = false;
                match result {
                    Ok(_) => {
                        // Refresh menus on every workspace so the new team appears.
                        for handle in cx.windows() {
                            if let Some(ws) = handle.downcast::<WorkspaceView>() {
                                ws.update(cx, |view, _w, cx| view.after_team_saved(cx)).ok();
                            }
                        }
                        window.remove_window();
                    }
                    Err(e) => {
                        view.status = Some(e);
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, _cx: &mut Context<Self>) {
        if event.keystroke.key == "escape" {
            window.remove_window();
        }
    }
}

/// Build a member row with the given seed values.
fn member_row(
    cx: &mut Context<TeamBuilderView>,
    roles: &[String],
    providers: &[String],
    name: &str,
    role: Option<&str>,
    provider: Option<&str>,
) -> MemberRow {
    let seed = name.to_string();
    let name_in = cx.new(move |cx| TextInput::new(cx).placeholder("member name").value(&seed));
    let ri = role.and_then(|r| roles.iter().position(|x| x == r)).unwrap_or(0);
    let role_sel = cx.new({
        let data = roles.to_vec();
        move |cx| Select::new(cx).label("Role").data(data).selected(ri)
    });
    let pi = provider.and_then(|p| providers.iter().position(|x| x == p)).unwrap_or(0);
    let prov_sel = cx.new({
        let data = providers.to_vec();
        move |cx| Select::new(cx).label("Agent").data(data).selected(pi)
    });
    MemberRow { name: name_in, role: role_sel, provider: prov_sel }
}

fn shape_list() -> Vec<String> {
    TEAM_SHAPES.iter().map(|s| s.to_string()).collect()
}

/// The built-in roles, shown until (or in place of) the relay-provided list.
fn fallback_roles() -> Vec<String> {
    ["supervisor", "worker", "frontend", "backend", "reviewer", "devops", "qa"]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

impl Render for TeamBuilderView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = guise::theme(cx);
        let bg = t.body().hsla();
        let text = t.text().hsla();
        let dim = t.dimmed().hsla();

        let me = cx.entity().downgrade();

        // Guided input (only in the Guided tab).
        let guided_ui = if self.guided {
            let gen = me.clone();
            let label = if self.busy { "Generating…" } else { "Generate" };
            Some(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(self.desc.clone())
                    .child(
                        div().flex().justify_end().child(
                            Button::new("team-generate", label)
                                .variant(Variant::Filled)
                                .on_click(move |_ev, window, app| {
                                    gen.update(app, |this, cx| this.generate(window, cx)).ok();
                                }),
                        ),
                    ),
            )
        } else {
            None
        };

        // The editable form, shown in Manual mode or after a Guided draft.
        let form = if self.have_form {
            let add = me.clone();
            let members = self
                .members
                .iter()
                .enumerate()
                .map(|(i, row)| {
                    let rm = me.clone();
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(6.0))
                        .p(px(8.0))
                        .rounded(px(8.0))
                        .bg(t.surface().hsla())
                        .child(row.name.clone())
                        .child(div().flex().gap(px(8.0)).child(row.role.clone()).child(row.provider.clone()))
                        .child(
                            div().flex().justify_end().child(
                                Button::new(SharedString::from(format!("team-rm-{i}")), "Remove")
                                    .variant(Variant::Subtle)
                                    .on_click(move |_ev, _window, app| {
                                        rm.update(app, |this, cx| {
                                            if i < this.members.len() {
                                                this.members.remove(i);
                                                cx.notify();
                                            }
                                        })
                                        .ok();
                                    }),
                            ),
                        )
                })
                .collect::<Vec<_>>();

            Some(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.0))
                    .child(self.name.clone())
                    .child(self.layout.clone())
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .child(div().text_size(px(12.0)).text_color(dim).child("Scope"))
                            .child(self.scope.clone()),
                    )
                    .child(div().text_size(px(12.0)).text_color(dim).child("Members"))
                    .child(div().flex().flex_col().gap(px(8.0)).children(members))
                    .child(
                        div().flex().child(
                            Button::new("team-add", "+ Add member")
                                .variant(Variant::Outline)
                                .on_click(move |_ev, _window, app| {
                                    add.update(app, |this, cx| this.add_member(cx)).ok();
                                }),
                        ),
                    ),
            )
        } else {
            None
        };

        let status = self.status.clone().map(|s| {
            div().text_size(px(12.0)).text_color(dim).child(SharedString::from(s))
        });

        let save = me.clone();
        let show_save = self.have_form;
        let save_label = if self.saving { "Saving…" } else { "Save Team" };
        let footer = div()
            .flex()
            .justify_end()
            .gap(px(8.0))
            .child(
                Button::new("team-cancel", "Cancel")
                    .variant(Variant::Default)
                    .on_click(move |_ev, window, _app| window.remove_window()),
            )
            .when(show_save, |d| {
                d.child(
                    Button::new("team-save", save_label)
                        .variant(Variant::Filled)
                        .on_click(move |_ev, window, app| {
                            save.update(app, |this, cx| this.commit(window, cx)).ok();
                        }),
                )
            });

        div()
            .size_full()
            .flex()
            .flex_col()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key_down))
            .bg(bg)
            .text_color(text)
            .pt(px(34.0))
            .px(px(16.0))
            .pb(px(16.0))
            .gap(px(12.0))
            .child(drag_strip())
            .child(div().text_size(px(15.0)).font_weight(FontWeight::BOLD).child("Team Builder"))
            .child(self.mode.clone())
            .children(guided_ui)
            .children(status)
            .child(
                div()
                    .id("team-form-scroll")
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .flex_1()
                    .overflow_y_scroll()
                    .children(form),
            )
            .child(footer)
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
