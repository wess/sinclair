//! Update UI: the "update available" prompt window, the launch/hourly notifier,
//! and the manual "Check for Updates…" entry point. The mechanics live in the
//! `updater` crate; this decides when to ask and drives the chosen action.
//!
//! The prompt is a state machine with exactly one action in flight. Once an
//! install starts it reports every stage it moves through and the window stays
//! put until it either restarts the app or fails with a reason on screen —
//! never a button whose only feedback is a notification you might not see.

use gpui::prelude::*;
use gpui::{
    div, point, px, size, App, Bounds, Context, FocusHandle, FontWeight, KeyDownEvent, MouseButton,
    SharedString, TitlebarOptions, Window, WindowBounds, WindowControlArea, WindowOptions,
};
use guise::{Alert, Button, Progress, Size, Variant};

use updater::{Check, Install, Release, Relaunch, Stage};

/// This build's version.
pub(crate) fn current() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

const WIDTH: f32 = 460.0;
const HEIGHT: f32 = 300.0;

/// Height reserved for the status area, so the window doesn't jump as the
/// prompt moves between its states.
const STATUS_HEIGHT: f32 = 62.0;

/// Marker so the background poller is started at most once.
struct Started;
impl gpui::Global for Started {}

/// The version we last opened the prompt for, so hourly re-checks don't reopen
/// a window the user already dismissed.
#[derive(Default)]
struct Notified(String);
impl gpui::Global for Notified {}

/// Set while an install is in flight, anywhere in the process.
///
/// `Phase::Working` only serializes installs within a single prompt window, and
/// there can be more than one: the manual "Check for Updates…" opens a prompt
/// unconditionally. Two installs would race two curls over the same staging
/// path, two `hdiutil attach` calls on the same mountpoint, and two rsyncs into
/// the live bundle — each window's unmount tearing down the other's mount
/// mid-copy.
#[derive(Default)]
struct Installing(bool);
impl gpui::Global for Installing {}

fn is_installing(cx: &App) -> bool {
    cx.try_global::<Installing>().is_some_and(|i| i.0)
}

fn installing(active: bool, cx: &mut App) {
    cx.set_global(Installing(active));
}

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
        let found = executor.spawn(async { updater::check(current(), &updater::detect()) }).await;
        cx.update(|cx| apply(found, false, cx));
        executor.timer(updater::POLL).await;
    })
    .detach();
}

/// Run a check now (the "Check for Updates…" menu item), opening the prompt if
/// there's an update and — when `manual` — a short status note otherwise.
pub fn check_now(cx: &mut App) {
    let executor = cx.background_executor().clone();
    cx.spawn(async move |cx| {
        let found = executor.spawn(async { updater::check(current(), &updater::detect()) }).await;
        cx.update(|cx| apply(found, true, cx));
    })
    .detach();
}

/// Apply a check result: open the prompt for an installable release (once per
/// version, or always when the user asked manually), and otherwise say why not.
fn apply(found: Result<Check, String>, manual: bool, cx: &mut App) {
    match found {
        // Never stack a second prompt on top of a running install: the new
        // window's Update button would start a concurrent one.
        Ok(Check::Ready(_)) if is_installing(cx) => {}
        Ok(Check::Ready(rel)) => {
            let seen = cx.try_global::<Notified>().map(|n| n.0 == rel.version).unwrap_or(false);
            if manual || !seen {
                cx.set_global(Notified(rel.version.clone()));
                open(rel, cx);
            }
        }
        // A newer version exists but its build hasn't finished uploading. Say so
        // rather than claiming you're up to date; the next poll picks it up.
        Ok(Check::Pending(version)) => {
            if manual {
                crate::view::post_os_notification(
                    "Update on the way",
                    &format!("Sinclair {version} is still building. Check back shortly."),
                );
            }
        }
        Ok(Check::UpToDate) => {
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

/// Where the prompt is in its one-shot lifecycle. Only [`Phase::Idle`] and
/// [`Phase::Failed`] accept the action, which is what keeps Update & Restart
/// from starting a second install over the first.
enum Phase {
    Idle,
    Working(Stage),
    Failed(String),
}

/// How far along the bar each stage sits, as the percentage `Progress` wants
/// (0..=100, *not* a 0..1 fraction). The download dominates the wall clock, so
/// it owns most of the bar and the later stages are checkpoints past it.
fn percent(stage: &Stage) -> f32 {
    match stage {
        Stage::Downloading { done, total } if *total > 0 => {
            85.0 * (*done as f32 / *total as f32).clamp(0.0, 1.0)
        }
        // Total unknown: hold at the start rather than pretending to advance.
        Stage::Downloading { .. } => 0.0,
        Stage::Preparing => 88.0,
        Stage::Installing => 94.0,
        Stage::Verifying => 98.0,
    }
}

/// Whether an install is in flight in this window, and so whether the action
/// button and Escape are inert. Free of the view so it can be tested directly.
fn busy(phase: &Phase) -> bool {
    matches!(phase, Phase::Working(_))
}

/// The action button's label for a given phase. `installable` is false when we
/// can't rewrite this install, or when the release hasn't published the asset
/// to do it with — either way the button must not promise an install.
fn action_label(phase: &Phase, installable: bool) -> &'static str {
    match phase {
        Phase::Working(_) => "Updating…",
        Phase::Failed(_) => "Try Again",
        Phase::Idle if installable => "Update & Restart",
        Phase::Idle => "Open Download",
    }
}

/// `Downloading` renders its byte counts; the rest are just their label.
fn detail(stage: &Stage) -> String {
    match stage {
        Stage::Downloading { done, total } if *total > 0 => {
            format!("{} of {}", megabytes(*done), megabytes(*total))
        }
        _ => String::new(),
    }
}

/// Decimal megabytes, matching what GitHub's release page and Finder report for
/// the same file. Dividing by 1 MiB instead would render an 87.4 MB download as
/// "83.3 MB" and read as a stalled or wrong transfer.
fn megabytes(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / 1_000_000.0)
}

struct UpdatePromptView {
    release: Release,
    install: Install,
    focus: FocusHandle,
    phase: Phase,
    /// Whether we can rewrite this install *and* the release actually published
    /// the asset to do it with. Both halves matter: an in-place install with no
    /// matching asset would offer an Update button that could only fail, which
    /// is precisely the state a release still uploading its artifacts leaves us
    /// in. Falling back to the download page keeps the button honest.
    installable: bool,
}

impl UpdatePromptView {
    fn new(release: Release, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        let install = updater::detect();
        let installable = install.is_in_place() && release.asset_for(&install).is_some();
        Self { release, install, focus: cx.focus_handle(), phase: Phase::Idle, installable }
    }

    fn action_label(&self) -> &'static str {
        action_label(&self.phase, self.installable)
    }

    fn busy(&self) -> bool {
        busy(&self.phase)
    }

    fn do_update(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // `busy` covers this window; the global covers a second prompt window
        // whose install is already running.
        if self.busy() || is_installing(cx) {
            return;
        }
        if self.installable {
            self.phase = Phase::Working(Stage::Downloading { done: 0, total: 0 });
            installing(true, cx);
            cx.notify();
            self.stage_and_restart(cx);
        } else {
            // Can't rewrite this install ourselves — open the release page.
            cx.open_url(&self.release.url);
            window.remove_window();
        }
    }

    /// Download + install in place off the UI thread, then relaunch into it.
    ///
    /// The installer reports its stages from a background thread, so they cross
    /// back over a channel that a foreground task drains into the view. Without
    /// that the window shows nothing at all until the whole install resolves.
    fn stage_and_restart(&mut self, cx: &mut Context<Self>) {
        crate::view::post_os_notification(
            "Sinclair",
            &format!("Downloading Sinclair {}…", self.release.version),
        );
        let release = self.release.clone();
        let install = self.install.clone();
        let executor = cx.background_executor().clone();
        let (tx, mut rx) = futures::channel::mpsc::unbounded::<Stage>();

        // Drain progress into the view for as long as the installer reports it.
        cx.spawn(async move |this, cx| {
            use futures::StreamExt;
            while let Some(stage) = rx.next().await {
                let updated = this.update(cx, |view, cx| {
                    if view.busy() {
                        view.phase = Phase::Working(stage);
                        cx.notify();
                    }
                });
                if updated.is_err() {
                    break;
                }
            }
        })
        .detach();

        cx.spawn(async move |this, cx| {
            let staged = executor
                .spawn(async move {
                    updater::install(&release, &install, &|stage| {
                        let _ = tx.unbounded_send(stage);
                    })
                })
                .await;
            match staged {
                Ok(relaunch) => {
                    // The prompt disables its own buttons while installing, but
                    // the titlebar's close control stays live. Closing the
                    // window withdraws consent to be restarted, so leave the
                    // new version on disk for the next launch instead. A dead
                    // entity is how that close reaches us here.
                    let dismissed = this.update(cx, |_, _| ()).is_err();
                    cx.update(|cx| installing(false, cx));
                    if dismissed {
                        crate::view::post_os_notification(
                            "Update installed",
                            "Sinclair will finish updating the next time you open it.",
                        );
                        return;
                    }
                    cx.update(|cx| {
                        save_sessions(cx);
                        // Relaunch::Current restarts with no explicit path on
                        // purpose: gpui reopens the running bundle via NSBundle.
                        // Handing `open` an explicit path right after the install
                        // is what used to relaunch the bare Mach-O in Terminal.app.
                        if let Relaunch::Binary(path) = relaunch {
                            cx.set_restart_path(path);
                        }
                        cx.restart();
                    });
                }
                Err(e) => {
                    cx.update(|cx| installing(false, cx));
                    crate::view::post_os_notification("Update failed", &e);
                    // Show the reason in the window and let the user retry it.
                    // The failure used to land only in a notification, which
                    // left the prompt looking like the click did nothing.
                    this.update(cx, |view, cx| {
                        view.phase = Phase::Failed(e);
                        cx.notify();
                    })
                    .ok();
                }
            }
        })
        .detach();
    }

    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, _cx: &mut Context<Self>) {
        // While an update is staging, keep the window (and its progress) around.
        if event.keystroke.key == "escape" && !self.busy() {
            window.remove_window();
        }
    }

    /// The status area: a live progress bar while installing, the reason when
    /// it failed, and what the button will do when idle.
    fn status(&self, dim: gpui::Hsla) -> gpui::AnyElement {
        match &self.phase {
            Phase::Working(stage) => div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(Progress::new(percent(stage)).size(Size::Sm))
                .child(
                    div()
                        .flex()
                        .justify_between()
                        .text_size(px(12.0))
                        .text_color(dim)
                        .child(SharedString::from(stage.label()))
                        .child(SharedString::from(detail(stage))),
                )
                .into_any_element(),
            Phase::Failed(reason) => Alert::new(SharedString::from(reason.clone()))
                .title("Update failed")
                .variant(Variant::Light)
                .color(guise::ColorName::Red)
                .into_any_element(),
            Phase::Idle => div()
                .text_size(px(13.0))
                .child(SharedString::from(if self.installable {
                    "Sinclair will download the update, install it, and restart."
                } else {
                    "Open the download page to update."
                }))
                .into_any_element(),
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
        let busy = self.busy();

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
                div().text_size(px(16.0)).font_weight(FontWeight::BOLD).child(SharedString::from(
                    format!("Sinclair {} is available", self.release.version),
                )),
            )
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(dim)
                    .child(SharedString::from(format!("You have {}.", current()))),
            )
            .child(div().h(px(STATUS_HEIGHT)).child(self.status(dim)))
            .child(div().flex_1())
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(8.0))
                    .child(
                        Button::new("upd-notes", "Release Notes")
                            .variant(Variant::Subtle)
                            .disabled(busy)
                            .on_click({
                                let url = self.release.url.clone();
                                move |_e, _w, app| app.open_url(&url)
                            }),
                    )
                    .child(
                        Button::new("upd-later", "Later")
                            .variant(Variant::Default)
                            .disabled(busy)
                            .on_click(|_e, window, _app| window.remove_window()),
                    )
                    .child(
                        Button::new("upd-go", self.action_label())
                            .variant(Variant::Filled)
                            .disabled(busy)
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

#[cfg(test)]
#[path = "../tests/updateui.rs"]
mod tests;
