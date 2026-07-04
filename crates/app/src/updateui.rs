//! Update UI: the "update available" prompt window, the launch/hourly notifier,
//! and the manual "Check for Updates…" entry point. The mechanics live in
//! [`crate::update`]; this decides when to ask and drives the chosen action.

use gpui::prelude::*;
use gpui::{
    div, point, px, size, App, Bounds, Context, FocusHandle, FontWeight, KeyDownEvent, MouseButton,
    SharedString, TitlebarOptions, Window, WindowBounds, WindowControlArea, WindowOptions,
};
use guise::{Button, Variant};

use crate::root::WorkspaceView;
use crate::update::{self, Install, Release};

const WIDTH: f32 = 460.0;
const HEIGHT: f32 = 260.0;

/// Marker so the background poller is started at most once.
struct Started;
impl gpui::Global for Started {}

/// The version we last opened the prompt for, so hourly re-checks don't reopen
/// a window the user already dismissed.
#[derive(Default)]
struct Notified(String);
impl gpui::Global for Notified {}

/// Start the launch + hourly update checker (once). The caller has already
/// checked `auto-update`. On finding a newer release it opens the prompt once
/// per version.
pub fn start_from_config(cx: &mut App) {
    if cx.try_global::<Started>().is_some() {
        return;
    }
    cx.set_global(Started);
    poll(cx);
}

fn poll(cx: &mut App) {
    let executor = cx.background_executor().clone();
    cx.spawn(async move |cx| loop {
        let found = executor.spawn(async { update::check() }).await;
        cx.update(|cx| apply(found, false, cx));
        executor.timer(update::POLL).await;
    })
    .detach();
}

/// Run a check now (the "Check for Updates…" menu item), opening the prompt if
/// there's an update and — when `manual` — a brief "up to date" note otherwise.
pub fn check_now(cx: &mut App) {
    let executor = cx.background_executor().clone();
    cx.spawn(async move |cx| {
        let found = executor.spawn(async { update::check() }).await;
        cx.update(|cx| apply(found, true, cx));
    })
    .detach();
}

/// Apply a check result: publish status, and open the prompt for a new release
/// (once per version, or always when the user asked manually).
fn apply(found: Result<Option<Release>, String>, manual: bool, cx: &mut App) {
    match found {
        Ok(Some(rel)) => {
            let seen = cx.try_global::<Notified>().map(|n| n.0 == rel.version).unwrap_or(false);
            if manual || !seen {
                cx.set_global(Notified(rel.version.clone()));
                open(rel, cx);
            }
        }
        Ok(None) => {
            if manual {
                crate::view::post_os_notification("Prompt", "You're on the latest version.");
            }
        }
        Err(e) => {
            if manual {
                crate::view::post_os_notification("Update check failed", &e);
            }
        }
    }
}

/// Open the update prompt window centered on the primary display.
pub fn open(release: Release, cx: &mut App) {
    let bounds = Bounds::centered(None, size(px(WIDTH), px(HEIGHT)), cx);
    let handle = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            is_resizable: false,
            titlebar: Some(TitlebarOptions {
                title: Some("Software Update".into()),
                appears_transparent: true,
                traffic_light_position: Some(point(px(12.0), px(12.0))),
            }),
            ..Default::default()
        },
        |window, cx| {
            window.set_window_title("Software Update");
            cx.new(|cx| UpdatePromptView::new(release, window, cx))
        },
    );
    if let Ok(handle) = handle {
        handle.update(cx, |_v, window, _cx| window.activate_window()).ok();
    }
}

struct UpdatePromptView {
    release: Release,
    install: Install,
    focus: FocusHandle,
}

impl UpdatePromptView {
    fn new(release: Release, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self { release, install: update::detect_install(), focus: cx.focus_handle() }
    }

    /// The Update button's label, tailored to how the app was installed.
    fn action_label(&self) -> &'static str {
        match self.install {
            Install::BrewCask => "Update via Homebrew",
            Install::LinuxPackage => "Show Upgrade Command",
            Install::MacApp(_) | Install::AppImage(_) => "Update & Restart",
            Install::Unknown => "Open Download",
        }
    }

    fn do_update(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match update::managed_command(&self.install) {
            // Package-managed: run the upgrade command in a pane on the workspace.
            Some(cmd) => {
                run_in_workspace(&cmd, cx);
                window.remove_window();
            }
            None => match &self.install {
                Install::MacApp(_) | Install::AppImage(_) => self.stage_and_restart(cx),
                // Unknown install: just open the release page.
                _ => {
                    cx.open_url(&self.release.url);
                    window.remove_window();
                }
            },
        }
    }

    /// Download + swap in place off the UI thread, then relaunch into it.
    fn stage_and_restart(&mut self, cx: &mut Context<Self>) {
        crate::view::post_os_notification("Prompt", "Downloading update…");
        let release = self.release.clone();
        let install = self.install.clone();
        let executor = cx.background_executor().clone();
        cx.spawn(async move |_this, cx| {
            let staged = executor.spawn(async move { update::stage(&release, &install) }).await;
            cx.update(|cx| match staged {
                Ok(bin) => {
                    cx.set_restart_path(bin);
                    cx.restart();
                }
                Err(e) => crate::view::post_os_notification("Update failed", &e),
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

impl Render for UpdatePromptView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = guise::theme(cx);
        let bg = t.body().hsla();
        let text = t.text().hsla();
        let dim = t.dimmed().hsla();
        let me = cx.entity().downgrade();
        let manual = matches!(self.install, Install::LinuxPackage | Install::Unknown);

        let body = if manual {
            match update::managed_command(&self.install) {
                Some(cmd) => format!("Run this to update:\n\n{cmd}"),
                None => "Open the download page to update.".to_string(),
            }
        } else {
            "A new version is available. Update now?".to_string()
        };

        div()
            .size_full()
            .flex()
            .flex_col()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key_down))
            .bg(bg)
            .text_color(text)
            .pt(px(34.0))
            .px(px(20.0))
            .pb(px(18.0))
            .gap(px(10.0))
            .child(drag_strip())
            .child(
                div()
                    .text_size(px(16.0))
                    .font_weight(FontWeight::BOLD)
                    .child(SharedString::from(format!("Prompt {} is available", self.release.version))),
            )
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(dim)
                    .child(SharedString::from(format!("You have {}.", update::current()))),
            )
            .child(div().text_size(px(13.0)).child(SharedString::from(body)))
            .child(div().flex_1())
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.0))
                    .child(
                        Button::new("upd-notes", "Release Notes")
                            .variant(Variant::Subtle)
                            .on_click({
                                let url = self.release.url.clone();
                                move |_e, _w, app| app.open_url(&url)
                            }),
                    )
                    .child(
                        Button::new("upd-later", "Later")
                            .variant(Variant::Default)
                            .on_click(|_e, window, _app| window.remove_window()),
                    )
                    .child(
                        Button::new("upd-go", self.action_label())
                            .variant(Variant::Filled)
                            .on_click({
                                let me = me.clone();
                                move |_e, window, app| {
                                    me.update(app, |this, cx| this.do_update(window, cx)).ok();
                                }
                            }),
                    ),
            )
    }
}

/// Run `cmd` in a new split on the frontmost workspace window.
fn run_in_workspace(cmd: &str, cx: &mut App) {
    for handle in cx.windows() {
        if let Some(ws) = handle.downcast::<WorkspaceView>() {
            let cmd = cmd.to_string();
            ws.update(cx, |view, window, cx| view.run_update_command(&cmd, window, cx)).ok();
            return;
        }
    }
}

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
