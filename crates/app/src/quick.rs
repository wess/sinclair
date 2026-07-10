//! Quick Terminal: a Quake-style dropdown terminal.
//!
//! - Summoned by a global hotkey (the `toggle_quick_terminal` binding, default
//!   cmd+alt+t) even when Sinclair is not the focused application, registered
//!   via Carbon `RegisterEventHotKey`, which needs no Accessibility permission.
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
/// Two Escapes within this window dismiss the quick terminal.
const DOUBLE_ESC: Duration = Duration::from_millis(500);

/// Tracks the single quick-terminal window so the hotkey can toggle it.
#[derive(Default)]
struct QuickWindow {
    handle: Option<WindowHandle<QuickTerminalView>>,
}
impl Global for QuickWindow {}

impl QuickWindow {
    fn set(cx: &mut App, handle: Option<WindowHandle<QuickTerminalView>>) {
        cx.set_global(QuickWindow { handle });
    }

    fn current(cx: &App) -> Option<WindowHandle<QuickTerminalView>> {
        cx.try_global::<QuickWindow>().and_then(|g| g.handle)
    }
}

/// Keeps the hotkey registration alive for the lifetime of the app.
struct Hotkeys(#[allow(dead_code)] GlobalHotKeyManager);
impl Global for Hotkeys {}

/// The user's `toggle_quick_terminal` binding, if one is resolvable from the
/// config (defaults included). Drives both the global summon hotkey and the
/// in-window dismiss chord, so a rebind applies to both.
fn toggle_binding(opts: &config::Options) -> Option<(config::Mods, String)> {
    let (binds, _) = config::resolve(&opts.keybind);
    binds
        .into_iter()
        .find(|kb| kb.action == config::Action::ToggleQuickTerminal && kb.tail.is_empty())
        .map(|kb| (kb.mods, kb.key))
}

/// Map a config binding onto a `global_hotkey` registration. `None` when the
/// key has no OS-level code mapping (the in-app binding still works).
fn hotkey_for(mods: &config::Mods, key: &str) -> Option<HotKey> {
    let mut m = Modifiers::empty();
    if mods.cmd {
        m |= Modifiers::SUPER;
    }
    if mods.ctrl {
        m |= Modifiers::CONTROL;
    }
    if mods.alt {
        m |= Modifiers::ALT;
    }
    if mods.shift {
        m |= Modifiers::SHIFT;
    }
    let code = key_code(key)?;
    Some(HotKey::new((!m.is_empty()).then_some(m), code))
}

/// OS key code for a config key name (letters, digits, F-keys, and the few
/// symbols that make sense as a global summon key).
fn key_code(key: &str) -> Option<Code> {
    let k = key.to_ascii_lowercase();
    let code = match k.as_str() {
        "a" => Code::KeyA, "b" => Code::KeyB, "c" => Code::KeyC, "d" => Code::KeyD,
        "e" => Code::KeyE, "f" => Code::KeyF, "g" => Code::KeyG, "h" => Code::KeyH,
        "i" => Code::KeyI, "j" => Code::KeyJ, "k" => Code::KeyK, "l" => Code::KeyL,
        "m" => Code::KeyM, "n" => Code::KeyN, "o" => Code::KeyO, "p" => Code::KeyP,
        "q" => Code::KeyQ, "r" => Code::KeyR, "s" => Code::KeyS, "t" => Code::KeyT,
        "u" => Code::KeyU, "v" => Code::KeyV, "w" => Code::KeyW, "x" => Code::KeyX,
        "y" => Code::KeyY, "z" => Code::KeyZ,
        "0" => Code::Digit0, "1" => Code::Digit1, "2" => Code::Digit2, "3" => Code::Digit3,
        "4" => Code::Digit4, "5" => Code::Digit5, "6" => Code::Digit6, "7" => Code::Digit7,
        "8" => Code::Digit8, "9" => Code::Digit9,
        "f1" => Code::F1, "f2" => Code::F2, "f3" => Code::F3, "f4" => Code::F4,
        "f5" => Code::F5, "f6" => Code::F6, "f7" => Code::F7, "f8" => Code::F8,
        "f9" => Code::F9, "f10" => Code::F10, "f11" => Code::F11, "f12" => Code::F12,
        "space" => Code::Space,
        "`" | "backquote" | "grave" => Code::Backquote,
        _ => return None,
    };
    Some(code)
}

/// Register the global summon hotkey (the user's `toggle_quick_terminal`
/// binding, default cmd+alt+t) and drain its events on a blocking background
/// thread — no foreground polling. Call once at startup. A failure (e.g. the
/// combo is already owned by another app) is logged and leaves the in-app
/// menu/keybind working.
pub fn install_global_hotkey(cx: &mut App) {
    let manager = match GlobalHotKeyManager::new() {
        Ok(manager) => manager,
        Err(error) => {
            eprintln!("sinclair: quick terminal: hotkey manager unavailable: {error}");
            return;
        }
    };
    let (opts, _) = config::load();
    let hotkey = toggle_binding(&opts)
        .and_then(|(mods, key)| hotkey_for(&mods, &key))
        .unwrap_or_else(|| HotKey::new(Some(Modifiers::SUPER | Modifiers::ALT), Code::KeyT));
    if let Err(error) = manager.register(hotkey) {
        eprintln!("sinclair: quick terminal: could not register the summon hotkey: {error}");
    }
    cx.set_global(Hotkeys(manager));

    let id = hotkey.id();
    let receiver = GlobalHotKeyEvent::receiver().clone();
    let (tx, mut rx) = futures::channel::mpsc::unbounded::<()>();
    std::thread::Builder::new()
        .name("quickhotkey".to_string())
        .spawn(move || {
            // Blocking drain: the thread parks between presses.
            while let Ok(event) = receiver.recv() {
                if event.id == id
                    && event.state == HotKeyState::Pressed
                    && tx.unbounded_send(()).is_err()
                {
                    return;
                }
            }
        })
        .ok();
    cx.spawn(async move |cx| {
        while rx.next().await.is_some() {
            cx.update(toggle);
        }
    })
    .detach();
}

/// Toggle the quick terminal: hide it if shown, reveal it if hidden, or open
/// it the first time. Self-contained so the global hotkey can drive it with
/// no focused workspace.
pub fn toggle(cx: &mut App) {
    if let Some(handle) = QuickWindow::current(cx) {
        if handle.is_active(cx).is_some() {
            toggle_alive(handle, cx);
            return;
        }
        QuickWindow::set(cx, None);
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
    QuickWindow::set(cx, Some(handle));
}

#[cfg(not(target_os = "macos"))]
fn toggle_alive(handle: WindowHandle<QuickTerminalView>, cx: &mut App) {
    handle
        .update(cx, |_, window, _| window.remove_window())
        .ok();
    QuickWindow::set(cx, None);
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
    let toggle_chord = toggle_binding(&opts);
    let palette = Rc::new(colors::from_config(
        &opts,
        crate::root::is_dark(cx.window_appearance()),
    ));
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
            eprintln!("sinclair: quick terminal: failed to spawn shell: {error}");
            return;
        }
    };
    let sh = Arc::new(sh);
    let fallback = session::shellname(opts.shell.as_deref());
    let cursor_style = opts.cursor_style;
    let copy_on_select = opts.copy_on_select;
    let smart_select = opts.smart_select;
    let middle_click_paste = opts.middle_click_paste;
    let option_as_alt = opts.macos_option_as_alt;
    let paste_protection = opts.clipboard_paste_protection;
    let clipboard_write = opts.clipboard_write;
    let unfocused_split_opacity = opts.unfocused_split_opacity;
    let suggest_cfg = crate::suggest::SuggestConfig::from_opts(&opts);

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
                    smart_select,
                    middle_click_paste,
                    option_as_alt,
                    paste_protection,
                    clipboard_write,
                    unfocused_split_opacity,
                    suggest_cfg,
                    fallback,
                    toggle_chord,
                    window,
                    cx,
                )
            })
        },
    );

    if let Ok(handle) = handle {
        handle.update(cx, |_, window, _| apply_overlay(window)).ok();
        QuickWindow::set(cx, Some(handle));
        cx.activate(true);
    }
}

/// A full-width band across the top of the display hosting the active (key)
/// window, falling back to the primary display when no window is active.
fn dropdown_bounds(cx: &mut App) -> Bounds<gpui::Pixels> {
    let active = cx
        .active_window()
        .and_then(|handle| handle.update(cx, |_, window, cx| window.display(cx)).ok())
        .flatten();
    match active.or_else(|| cx.primary_display()) {
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
    /// The user's `toggle_quick_terminal` chord; pressing it inside the window
    /// dismisses (hides) it, mirroring the summon key.
    toggle_chord: Option<(config::Mods, String)>,
    _subscription: Subscription,
    /// Quake-style autohide: hide the moment the window resigns key (macOS
    /// only — it relies on the overlay hide/show other platforms stub out).
    /// Event-driven via the window-activation observer; no polling.
    #[cfg(target_os = "macos")]
    _activation: Subscription,
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
        smart_select: bool,
        middle_click_paste: bool,
        option_as_alt: config::OptionAsAlt,
        paste_protection: bool,
        clipboard_write: config::ClipboardAccess,
        unfocused_split_opacity: f32,
        suggest_cfg: crate::suggest::SuggestConfig,
        fallback: String,
        toggle_chord: Option<(config::Mods, String)>,
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
                smart_select,
                middle_click_paste,
                option_as_alt,
                paste_protection,
                clipboard_write,
                unfocused_split_opacity,
                suggest_cfg,
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

        // Hide the moment the window resigns key (the user clicked another app
        // or window). The observer only fires on changes, so a freshly summoned
        // window that hasn't become key yet is never hidden prematurely.
        #[cfg(target_os = "macos")]
        let activation = cx.observe_window_activation(window, |_this, window, _cx| {
            if !window.is_window_active() {
                appkit::hide(window);
            }
        });

        window.focus(&view.focus_handle(cx), cx);
        Self {
            view,
            focus: cx.focus_handle(),
            last_esc: None,
            toggle_chord,
            _subscription: subscription,
            #[cfg(target_os = "macos")]
            _activation: activation,
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

    /// The toggle hotkey (the user's `toggle_quick_terminal` binding) also
    /// dismisses the window from inside, hiding it so the session is kept.
    /// Modifier chords are never encoded by the terminal, so the keystroke
    /// bubbles up here instead of hitting the shell.
    fn key_down(&mut self, event: &KeyDownEvent, window: &mut gpui::Window, cx: &mut Context<Self>) {
        let Some((mods, key)) = &self.toggle_chord else {
            return;
        };
        let ks = &event.keystroke;
        let m = &ks.modifiers;
        if m.platform == mods.cmd
            && m.alt == mods.alt
            && m.control == mods.ctrl
            && m.shift == mods.shift
            && ks.key.eq_ignore_ascii_case(key)
        {
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
