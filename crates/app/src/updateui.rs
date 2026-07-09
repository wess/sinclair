//! Update UI: the "update available" prompt window, the launch/hourly notifier,
//! and the manual "Check for Updates…" entry point. The mechanics live in the
//! `updater` crate; this decides when to ask and drives the chosen action.

use gpui::prelude::*;
use gpui::{
    div, point, px, size, App, Bounds, Context, FocusHandle, FontWeight, KeyDownEvent, MouseButton,
    SharedString, TitlebarOptions, Window, WindowBounds, WindowControlArea, WindowOptions,
};
use guise::{Button, Variant};

use updater::{Install, Release, Relaunch};

/// This build's version.
pub(crate) fn current() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

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
        let found = executor.spawn(async { updater::check(current()) }).await;
        cx.update(|cx| apply(found, false, cx));
        executor.timer(updater::POLL).await;
    })
    .detach();
}

/// Run a check now (the "Check for Updates…" menu item), opening the prompt if
/// there's an update and — when `manual` — a brief "up to date" note otherwise.
pub fn check_now(cx: &mut App) {
    let executor = cx.background_executor().clone();
    cx.spawn(async move |cx| {
        let found = executor.spawn(async { updater::check(current()) }).await;
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
                crate::view::post_os_notification("Sinclair", "You're on the latest version.");
            }
        }
        Err(e) => {
            if manual {
                crate::view::post_os_notification("Update check failed", &e);
            }
        }
    }
}

/// Persist a workspace window's session before an update restart, so a
/// session-restore user comes back to their tabs. The restart never goes
/// through `request_quit`, which is where an ordinary quit saves. The active
/// window here is usually the update prompt itself, so fall back to the first
/// window whose root actually downcasts to a workspace.
fn save_sessions(cx: &mut App) {
    let workspace = cx
        .active_window()
        .and_then(|w| w.downcast::<crate::root::WorkspaceView>())
        .or_else(|| {
            cx.windows().into_iter().find_map(|w| w.downcast::<crate::root::WorkspaceView>())
        });
    if let Some(handle) = workspace {
        let _ = handle.update(cx, |view, _window, cx| view.save_state(cx));
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
        Self { release, install: updater::detect(), focus: cx.focus_handle() }
    }

    /// The Update button's label, tailored to how the app was installed.
    fn action_label(&self) -> &'static str {
        if self.install.is_in_place() {
            "Update & Restart"
        } else {
            "Open Download"
        }
    }

    fn do_update(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.install.is_in_place() {
            // Download the release and swap it in place, then relaunch.
            self.stage_and_restart(cx);
        } else {
            // Can't rewrite this install ourselves — open the release page.
            cx.open_url(&self.release.url);
            window.remove_window();
        }
    }

    /// Download + install in place off the UI thread, then relaunch into it.
    fn stage_and_restart(&mut self, cx: &mut Context<Self>) {
        crate::view::post_os_notification("Sinclair", "Downloading update…");
        let release = self.release.clone();
        let install = self.install.clone();
        let executor = cx.background_executor().clone();
        cx.spawn(async move |_this, cx| {
            let staged =
                executor.spawn(async move { updater::install(&release, &install) }).await;
            cx.update(|cx| match staged {
                Ok(relaunch) => {
                    save_sessions(cx);
                    // Relaunch::Current restarts with no explicit path on
                    // purpose: gpui reopens the running bundle via NSBundle.
                    // Handing `open` an explicit path right after the install
                    // is what used to relaunch the bare Mach-O in Terminal.app.
                    if let Relaunch::Binary(path) = relaunch {
                        cx.set_restart_path(path);
                    }
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

        let body = if self.install.is_in_place() {
            "A new version is available. Update now?"
        } else {
            "Open the download page to update."
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
                    .child(SharedString::from(format!("Sinclair {} is available", self.release.version))),
            )
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(dim)
                    .child(SharedString::from(format!("You have {}.", current()))),
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
