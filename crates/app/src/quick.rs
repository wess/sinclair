//! Quick Terminal: a Quake-style dropdown terminal.
//!
//! - Summoned by a global hotkey (default cmd+alt+t) even when Prompt is not
//!   the focused application, registered via Carbon `RegisterEventHotKey`,
//!   which needs no Accessibility permission.
//! - Floats above every other app and Space (see [`crate::appkit`]).
//! - A single instance whose shell persists across toggles: toggling hides
//!   and re-shows the same window rather than respawning it.

use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::channel::mpsc::UnboundedReceiver;
use futures::StreamExt;
use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use gpui::prelude::*;
use gpui::{
    div, point, px, size, App, Bounds, Context, Entity, FocusHandle, Focusable as _, Global,
    KeyDownEvent, Subscription, TitlebarOptions, WindowBounds, WindowHandle, WindowKind,
    WindowOptions,
};
use terminal::{Event, Session};

#[cfg(target_os = "macos")]
use crate::appkit;
use crate::bridge;
use crate::colors::{self, Colors};
use crate::metrics::Padding;
use crate::session;
use crate::view::{TerminalView, ViewEvent};

/// Initial grid for the quick terminal until its first layout pass.
const COLS: usize = 80;
const ROWS: usize = 20;
/// Fraction of the display height the dropdown occupies.
const HEIGHT_FRACTION: f32 = 0.42;
const MIN_HEIGHT: f32 = 260.0;
/// How often the global-hotkey event queue is drained (cheap, idle-friendly).
const POLL: Duration = Duration::from_millis(60);
/// Two Escapes within this window dismiss the quick terminal.
const DOUBLE_ESC: Duration = Duration::from_millis(500);

/// Tracks the single quick-terminal window so the hotkey can toggle it.
#[derive(Default)]
struct QuickWindow {
    handle: Option<WindowHandle<QuickTerminalView>>,
    /// Whether the window has been observed active since it was last shown.
    /// Gates autohide so a freshly summoned window isn't hidden before macOS
    /// finishes activating it.
    seen_active: bool,
}
impl Global for QuickWindow {}

impl QuickWindow {
    fn set(cx: &mut App, handle: Option<WindowHandle<QuickTerminalView>>, seen_active: bool) {
        cx.set_global(QuickWindow {
            handle,
            seen_active,
        });
    }

    fn current(cx: &App) -> (Option<WindowHandle<QuickTerminalView>>, bool) {
        match cx.try_global::<QuickWindow>() {
            Some(g) => (g.handle, g.seen_active),
            None => (None, false),
        }
    }
}

/// Keeps the hotkey registration alive for the lifetime of the app.
struct Hotkeys(#[allow(dead_code)] GlobalHotKeyManager);
impl Global for Hotkeys {}

/// Register the global summon hotkey (cmd+alt+t) and start draining its
/// events. Call once at startup. A failure (e.g. the combo is already owned
/// by another app) is logged and leaves the in-app menu/keybind working.
pub fn install_global_hotkey(cx: &mut App) {
    let manager = match GlobalHotKeyManager::new() {
        Ok(manager) => manager,
        Err(error) => {
            eprintln!("prompt: quick terminal: hotkey manager unavailable: {error}");
            return;
        }
    };
    let hotkey = HotKey::new(Some(Modifiers::SUPER | Modifiers::ALT), Code::KeyT);
    if let Err(error) = manager.register(hotkey) {
        eprintln!("prompt: quick terminal: could not register cmd+alt+t: {error}");
    }
    cx.set_global(Hotkeys(manager));

    let id = hotkey.id();
    let receiver = GlobalHotKeyEvent::receiver().clone();
    let executor = cx.background_executor().clone();
    cx.spawn(async move |cx| loop {
        executor.timer(POLL).await;
        let mut fire = false;
        while let Ok(event) = receiver.try_recv() {
            if event.id == id && event.state == HotKeyState::Pressed {
                fire = true;
            }
        }
        if fire {
            cx.update(toggle);
        } else {
            cx.update(autohide);
        }
    })
    .detach();
}

/// Quake-style autohide: once the window has been seen active, hide it the
/// moment it loses focus (the user clicked another app or window). macOS only;
/// it relies on the overlay hide/show that other platforms stub out.
#[cfg(not(target_os = "macos"))]
fn autohide(_cx: &mut App) {}

#[cfg(target_os = "macos")]
fn autohide(cx: &mut App) {
    let (Some(handle), seen_active) = QuickWindow::current(cx) else {
        return;
    };
    let Ok(true) = handle.update(cx, |_, window, _| appkit::is_visible(window)) else {
        if seen_active {
            QuickWindow::set(cx, Some(handle), false);
        }
        return;
    };
    let active = handle.is_active(cx).unwrap_or(false);
    if active {
        if !seen_active {
            QuickWindow::set(cx, Some(handle), true);
        }
    } else if seen_active {
        handle.update(cx, |_, window, _| appkit::hide(window)).ok();
        QuickWindow::set(cx, Some(handle), false);
    }
}

/// Toggle the quick terminal: hide it if shown, reveal it if hidden, or open
/// it the first time. Self-contained so the global hotkey can drive it with
/// no focused workspace.
pub fn toggle(cx: &mut App) {
    if let (Some(handle), _) = QuickWindow::current(cx) {
        if handle.is_active(cx).is_some() {
            toggle_alive(handle, cx);
            return;
        }
        QuickWindow::set(cx, None, false);
    }
    open(cx);
}

/// macOS keeps the session by hiding/showing the overlay; other platforms
/// lack a portable hide-and-keep, so the window is closed and respawned.
#[cfg(target_os = "macos")]
fn toggle_alive(handle: WindowHandle<QuickTerminalView>, cx: &mut App) {
    let visible = handle
        .update(cx, |_, window, _| appkit::is_visible(window))
        .unwrap_or(false);
    if visible {
        handle.update(cx, |_, window, _| appkit::hide(window)).ok();
    } else {
        cx.activate(true);
        handle
            .update(cx, |this, window, cx| {
                appkit::make_overlay(window);
                appkit::show(window);
                window.focus(&this.view.focus_handle(cx), cx);
            })
            .ok();
    }
    QuickWindow::set(cx, Some(handle), false);
}

#[cfg(not(target_os = "macos"))]
fn toggle_alive(handle: WindowHandle<QuickTerminalView>, cx: &mut App) {
    handle
        .update(cx, |_, window, _| window.remove_window())
        .ok();
    QuickWindow::set(cx, None, false);
}

/// Window kind for the quick terminal: a Wayland layer-shell overlay where
/// available, otherwise a normal window decorated by [`apply_overlay`].
fn quick_window_kind() -> WindowKind {
    #[cfg(target_os = "linux")]
    {
        crate::linux::window_kind()
    }
    #[cfg(not(target_os = "linux"))]
    {
        WindowKind::Normal
    }
}

/// Make the open window float above everything, per platform.
fn apply_overlay(window: &gpui::Window) {
    #[cfg(target_os = "macos")]
    appkit::make_overlay(window);
    #[cfg(target_os = "linux")]
    crate::linux::make_overlay(window);
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let _ = window;
}

/// Open a fresh quick-terminal window dropped over the active display.
fn open(cx: &mut App) {
    let (opts, _diagnostics) = config::load();
    let palette = Rc::new(colors::from_config(&opts));
    let font = crate::font::build(&opts);
    let font_size = px(opts.font_size.max(1.0));
    let cell = crate::metrics::measure(cx.text_system(), &font, font_size);
    let pad = Padding {
        x: opts.window_padding_x as f32,
        y: opts.window_padding_y as f32,
    };

    let options = session::options(&opts, COLS, ROWS, None);
    let (sh, events) = match Session::spawn(options) {
        Ok(pair) => pair,
        Err(error) => {
            eprintln!("prompt: quick terminal: failed to spawn shell: {error}");
            return;
        }
    };
    let sh = Arc::new(sh);
    let fallback = session::shellname(opts.shell.as_deref());
    let cursor_style = opts.cursor_style;
    let copy_on_select = opts.copy_on_select;
    let option_as_alt = opts.macos_option_as_alt;
    let paste_protection = opts.clipboard_paste_protection;
    let clipboard_write = opts.clipboard_write;

    let bounds = dropdown_bounds(cx);
    let handle = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            kind: quick_window_kind(),
            is_resizable: false,
            is_minimizable: false,
            titlebar: Some(TitlebarOptions {
                title: Some("Quick Terminal".into()),
                appears_transparent: true,
                traffic_light_position: Some(point(px(12.0), px(12.0))),
            }),
            ..Default::default()
        },
        move |window, cx| {
            window.set_window_title("Quick Terminal");
            cx.new(move |cx| {
                QuickTerminalView::new(
                    sh,
                    events,
                    palette,
                    font,
                    font_size,
                    cell,
                    pad,
                    cursor_style,
                    copy_on_select,
                    option_as_alt,
                    paste_protection,
                    clipboard_write,
                    fallback,
                    window,
                    cx,
                )
            })
        },
    );

    if let Ok(handle) = handle {
        handle.update(cx, |_, window, _| apply_overlay(window)).ok();
        QuickWindow::set(cx, Some(handle), false);
        cx.activate(true);
    }
}

/// A full-width band across the top of the active display.
fn dropdown_bounds(cx: &App) -> Bounds<gpui::Pixels> {
    match cx.primary_display() {
        Some(display) => {
            let screen = display.bounds();
            let height = (f32::from(screen.size.height) * HEIGHT_FRACTION).max(MIN_HEIGHT);
            Bounds {
                origin: screen.origin,
                size: size(screen.size.width, px(height)),
            }
        }
        None => Bounds {
            origin: point(px(0.0), px(0.0)),
            size: size(px(1200.0), px(MIN_HEIGHT)),
        },
    }
}

pub struct QuickTerminalView {
    view: Entity<TerminalView>,
    focus: FocusHandle,
    /// Timestamp of the last lone Escape, for double-Escape dismissal.
    last_esc: Option<Instant>,
    _subscription: Subscription,
}

impl QuickTerminalView {
    #[allow(clippy::too_many_arguments)]
    fn new(
        session: Arc<Session>,
        events: std::sync::mpsc::Receiver<Event>,
        colors: Rc<Colors>,
        font: gpui::Font,
        font_size: gpui::Pixels,
        cell: crate::metrics::CellSize,
        pad: Padding,
        cursor_style: config::CursorStyle,
        copy_on_select: bool,
        option_as_alt: config::OptionAsAlt,
        paste_protection: bool,
        clipboard_write: config::ClipboardAccess,
        fallback: String,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let view = cx.new(|cx| {
            TerminalView::new(
                session,
                colors,
                font,
                font_size,
                cell,
                pad,
                cursor_style,
                copy_on_select,
                option_as_alt,
                paste_protection,
                clipboard_write,
                fallback,
                window,
                cx,
            )
        });

        let weak = view.downgrade();
        let mut events: UnboundedReceiver<Event> = bridge::forward(events);
        window
            .spawn(cx, async move |cx| {
                while let Some(event) = events.next().await {
                    if weak.update(cx, |view, cx| view.apply(event, cx)).is_err() {
                        break;
                    }
                }
            })
            .detach();

        let subscription = cx.subscribe_in(
            &view,
            window,
            |_this, _view, event: &ViewEvent, window, _cx| {
                if matches!(event, ViewEvent::Exited) {
                    window.remove_window();
                }
            },
        );

        window.focus(&view.focus_handle(cx), cx);
        Self {
            view,
            focus: cx.focus_handle(),
            last_esc: None,
            _subscription: subscription,
        }
    }

    /// Capture phase: detect a double-Escape before the terminal swallows the
    /// key. A lone Escape still falls through to the shell (so it works in
    /// vim, etc.); a second Escape within [`DOUBLE_ESC`] dismisses the window.
    fn capture_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        let ks = &event.keystroke;
        let bare = !ks.modifiers.platform
            && !ks.modifiers.alt
            && !ks.modifiers.control
            && !ks.modifiers.shift;
        if ks.key != "escape" || !bare {
            self.last_esc = None;
            return;
        }
        let now = Instant::now();
        match self.last_esc {
            Some(prev) if now.duration_since(prev) <= DOUBLE_ESC => {
                self.last_esc = None;
                self.dismiss(window);
                cx.stop_propagation();
            }
            _ => self.last_esc = Some(now),
        }
    }

    /// The toggle hotkey (cmd+alt+t) also dismisses the window from inside,
    /// hiding it so the session is kept. Cmd chords are never encoded by the
    /// terminal, so the keystroke bubbles up here instead of hitting the shell.
    fn key_down(&mut self, event: &KeyDownEvent, window: &mut gpui::Window, cx: &mut Context<Self>) {
        let ks = &event.keystroke;
        let m = &ks.modifiers;
        if m.platform && m.alt && !m.control && !m.shift && ks.key == "t" {
            self.dismiss(window);
            cx.stop_propagation();
        }
    }

    /// Hide the window from inside. macOS keeps the session (overlay hide);
    /// other platforms close it, since there is no portable hide-and-keep.
    fn dismiss(&self, window: &mut gpui::Window) {
        #[cfg(target_os = "macos")]
        appkit::hide(window);
        #[cfg(not(target_os = "macos"))]
        window.remove_window();
    }
}

impl Render for QuickTerminalView {
    fn render(&mut self, _window: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .track_focus(&self.focus)
            .capture_key_down(cx.listener(Self::capture_key))
            .on_key_down(cx.listener(Self::key_down))
            .child(self.view.clone())
    }
}
