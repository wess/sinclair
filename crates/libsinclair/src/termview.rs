//! A drop-in terminal pane for gpui apps.
//!
//! [`TermView`] owns a live [`Session`] and wires the full loop: pty events
//! repaint the pane, the element re-grids to its bounds and resizes the pty,
//! keystrokes and mouse gestures are encoded to pty bytes, and selection,
//! scrollback, links, and copy/paste behave like a terminal. Every session
//! [`Event`] is re-emitted so the host can react (close the pane on `Exit`,
//! retitle on `TitleChanged`, ...). Hosts that want more — overlays, search
//! UI, notifications — build their own view over
//! [`crate::element::TerminalElement`] the way the Sinclair app does.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use gpui::prelude::*;
use gpui::{
    div, px, App, ClipboardItem, Context, EventEmitter, FocusHandle, Focusable, Font,
    FontFeatures, FontStyle, FontWeight, KeyDownEvent, KeyUpEvent, Pixels, Subscription, Window,
};
use terminal::{Event, Session, SessionOptions};

use crate::bridge;
use crate::colors::{self, Colors};
use crate::element::{CursorShape, SnapCache, TerminalElement};
use crate::metrics::{self, CellSize, Padding};
use crate::mouse::MouseState;
use crate::pointer::{clipboard_copy, CopyHook};

/// Maximum time a frame is withheld for synchronized output before it is
/// painted anyway, so a stuck ?2026 cannot freeze the view.
const SYNC_TIMEOUT: Duration = Duration::from_millis(150);

/// Appearance and behavior for a [`TermView`]. The default is the built-in
/// dark theme with a platform monospace font.
pub struct TermOptions {
    pub colors: Colors,
    pub font: Font,
    /// Font size in pixels.
    pub font_size: f32,
    /// Inner padding around the grid, in pixels.
    pub pad: Padding,
    /// Cursor shape while the program leaves DECSCUSR at its default.
    pub cursor: CursorShape,
    /// Finishing a selection drag copies it to the clipboard.
    pub copy_on_select: bool,
    /// Double-click selects the semantic token under the pointer.
    pub smart_select: bool,
    /// Middle-click pastes the current selection (X style).
    pub middle_click_paste: bool,
    /// macOS: Option acts as Alt (ESC-prefixed bytes) instead of composing
    /// characters.
    pub option_as_alt: bool,
    /// Honor program clipboard writes (OSC 52). Off drops them silently.
    pub clipboard_write: bool,
    /// What copy-on-select and the copy shortcut do with captured text.
    /// Defaults to a plain clipboard write; hosts can layer redaction or
    /// history on top.
    pub copy: Rc<CopyHook>,
}

impl Default for TermOptions {
    fn default() -> Self {
        Self {
            colors: Colors::from_scheme(theme::default_scheme()),
            font: default_font(),
            font_size: 14.0,
            pad: Padding { x: 8.0, y: 8.0 },
            cursor: CursorShape::Block,
            copy_on_select: false,
            smart_select: true,
            middle_click_paste: true,
            option_as_alt: false,
            clipboard_write: true,
            copy: Rc::new(clipboard_copy),
        }
    }
}

/// A monospace family present on each platform, so the zero-config path
/// renders sensibly. Embedders set their own in [`TermOptions::font`].
fn default_font() -> Font {
    let family = if cfg!(target_os = "macos") {
        "Menlo"
    } else if cfg!(target_os = "windows") {
        "Consolas"
    } else {
        "DejaVu Sans Mono"
    };
    Font {
        family: family.into(),
        features: FontFeatures(Arc::new(Vec::new())),
        fallbacks: None,
        weight: FontWeight::NORMAL,
        style: FontStyle::Normal,
    }
}

/// The key name as text only when it is a single character (a literal key
/// such as `b` or `/`), never a named key like `left` or `enter`.
fn single_char(key: &str) -> Option<&str> {
    let mut chars = key.chars();
    match (chars.next(), chars.next()) {
        (Some(_), None) => Some(key),
        _ => None,
    }
}

/// Schedule a repaint from a focus in/out listener: gpui dispatches those
/// inside the draw's focus phase, where a plain `notify` cannot mark the
/// window dirty, so the cursor's filled/hollow flip would wait for the next
/// unrelated redraw.
fn repaint_after_focus_change(view: &gpui::WeakEntity<TermView>, cx: &mut App) {
    let view = view.clone();
    cx.defer(move |cx| {
        let _ = view.update(cx, |_, cx| cx.notify());
    });
}

/// Bind a view's focus listeners to `window`. Split out of `new` because these
/// are per-*window* subscriptions: a `TermView` moved between windows keeps
/// listeners pointed at the old one, which then report that window's focus
/// instead of its own (see [`TermView::rehome`]).
fn focus_subs(
    focus: &FocusHandle,
    window: &mut Window,
    cx: &mut Context<TermView>,
) -> [Subscription; 3] {
    let on_in = cx.weak_entity();
    let sub_in = window.on_focus_in(focus, cx, move |_window, cx| {
        let _ = on_in.update(cx, |this, _| {
            this.focused = true;
            this.report_focus(true);
        });
        repaint_after_focus_change(&on_in, cx);
    });
    let on_out = cx.weak_entity();
    let sub_out = window.on_focus_out(focus, cx, move |_event, _window, cx| {
        let _ = on_out.update(cx, |this, _| {
            this.focused = false;
            this.report_focus(false);
        });
        repaint_after_focus_change(&on_out, cx);
    });
    // gpui derives focus in/out from the drawn frame and folds window
    // activation into it, so a frame drawn before the platform reports the
    // window active carries an empty focus path. A window that gains focus and
    // activation together can therefore settle with `focused` never set,
    // painting a hollow cursor in the pane the user is typing into. Activation
    // is the one signal that outranks the frame — re-derive `focused` from it.
    let sub_act = cx.observe_window_activation(window, |this, window, cx| {
        let focused = window.is_window_active() && this.focus.contains_focused(window, cx);
        if this.focused != focused {
            this.focused = focused;
            this.report_focus(focused);
            cx.notify();
        }
    });
    [sub_in, sub_out, sub_act]
}

pub struct TermView {
    session: Arc<Session>,
    colors: Rc<Colors>,
    font: Font,
    font_size: Pixels,
    cell: CellSize,
    pad: Padding,
    cursor: CursorShape,
    copy_on_select: bool,
    smart_select: bool,
    middle_click_paste: bool,
    option_as_alt: bool,
    clipboard_write: bool,
    copy: Rc<CopyHook>,
    /// Pointer state shared with the element's per-frame event closures.
    mouse: Rc<RefCell<MouseState>>,
    /// Decoded sixel textures, keyed by placement id; persists across frames.
    image_cache: Rc<RefCell<HashMap<u64, Arc<gpui::RenderImage>>>>,
    /// Previous frame's render snapshot, reused while vt reports no damage.
    snap_cache: Rc<RefCell<SnapCache>>,
    focus: FocusHandle,
    focused: bool,
    /// True while a repaint is withheld for synchronized output (?2026),
    /// with a safety timer armed to release it.
    sync_pending: bool,
    /// Last vt title (OSC 0/2); `None` until the child sets one.
    title: Option<String>,
    exited: bool,
    /// Focus in/out listeners plus the window-activation resync; together they
    /// drive focus reporting (?1004) and the focused/unfocused cursor.
    _focus_subs: [Subscription; 3],
}

impl TermView {
    /// Spawn a child on a fresh pty and build the view around it.
    pub fn spawn(
        session: SessionOptions,
        opts: TermOptions,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> std::io::Result<Self> {
        let (session, events) = Session::spawn(session)?;
        Ok(Self::new(Arc::new(session), events, opts, window, cx))
    }

    /// Build the view around an already-running session (its event receiver
    /// still in hand).
    pub fn new(
        session: Arc<Session>,
        events: Receiver<Event>,
        opts: TermOptions,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let colors = Rc::new(opts.colors);
        session.with_term(|term| term.set_report_colors(colors::report_colors(&colors)));
        let font_size = px(opts.font_size.max(1.0));
        let cell = metrics::measure(window.text_system(), &opts.font, font_size);

        let focus = cx.focus_handle();
        let subs = focus_subs(&focus, window, cx);

        let mut events = bridge::forward(events);
        cx.spawn(async move |this, cx| {
            while let Some(event) = events.next().await {
                if this.update(cx, |view, cx| view.apply(event, cx)).is_err() {
                    break;
                }
            }
        })
        .detach();

        Self {
            session,
            colors,
            font: opts.font,
            font_size,
            cell,
            pad: opts.pad,
            cursor: opts.cursor,
            copy_on_select: opts.copy_on_select,
            smart_select: opts.smart_select,
            middle_click_paste: opts.middle_click_paste,
            option_as_alt: opts.option_as_alt,
            clipboard_write: opts.clipboard_write,
            copy: opts.copy,
            mouse: Rc::new(RefCell::new(MouseState::default())),
            image_cache: Rc::new(RefCell::new(HashMap::new())),
            snap_cache: Rc::new(RefCell::new(SnapCache::default())),
            focus,
            focused: false,
            sync_pending: false,
            title: None,
            exited: false,
            _focus_subs: subs,
        }
    }

    /// Move this view's focus listeners onto `window`. Call it after moving a
    /// `TermView` into a different window (tearing a pane off into its own
    /// window, say): the listeners are per-window subscriptions, so until they
    /// are rebound the view reports the *old* window's focus and can arrive
    /// unfocused with no listener left that can mark it focused again.
    pub fn rehome(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self._focus_subs = focus_subs(&self.focus, window, cx);
    }

    /// The live session, for direct writes and terminal reads.
    pub fn session(&self) -> &Arc<Session> {
        &self.session
    }

    /// The child's window title (OSC 0/2), once it has set one.
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    /// Whether the child has exited.
    pub fn exited(&self) -> bool {
        self.exited
    }

    /// Swap the color set (theme change); updates OSC 4/10/11/12 reporting.
    pub fn set_colors(&mut self, colors: Colors, cx: &mut Context<Self>) {
        self.colors = Rc::new(colors);
        self.session
            .with_term(|term| term.set_report_colors(colors::report_colors(&self.colors)));
        cx.notify();
    }

    /// Swap font and size; the next layout re-grids and resizes the pty.
    pub fn set_font(&mut self, font: Font, size: f32, window: &mut Window, cx: &mut Context<Self>) {
        self.font_size = px(size.max(1.0));
        self.cell = metrics::measure(window.text_system(), &font, self.font_size);
        self.font = font;
        cx.notify();
    }

    /// Paste the system clipboard into the pty (bracketed when enabled).
    pub fn paste(&mut self, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        if text.is_empty() {
            return;
        }
        let bracketed = self.session.with_term(|term| term.bracketed_paste());
        self.scroll_to_bottom(cx);
        let _ = self.session.write(&input::encode_paste(&text, bracketed));
    }

    /// Copy the current selection through the copy hook; `false` when there
    /// is no selection.
    pub fn copy_selection(&mut self, cx: &mut Context<Self>) -> bool {
        let text = self.session.with_term(|term| term.selection_text());
        match text.filter(|text| !text.is_empty()) {
            Some(text) => {
                (self.copy)(text, cx);
                true
            }
            None => false,
        }
    }

    /// Emit a focus-in/out report to the child if it enabled ?1004.
    fn report_focus(&self, focused: bool) {
        let out = self.session.with_term(|term| {
            term.report_focus(focused);
            term.take_output()
        });
        if !out.is_empty() {
            let _ = self.session.write(&out);
        }
    }

    /// Any write to the pty snaps the view back to the live bottom.
    fn scroll_to_bottom(&self, cx: &mut Context<Self>) {
        let was_back = self.session.with_term(|term| {
            let back = term.display_offset() != 0;
            term.set_display_offset(0);
            back
        });
        if was_back {
            cx.notify();
        }
    }

    /// Apply one session event; called from the bridge task.
    fn apply(&mut self, event: Event, cx: &mut Context<Self>) {
        match &event {
            Event::Wakeup => self.wakeup(cx),
            Event::TitleChanged(title) => {
                self.title = Some(title.clone());
                cx.notify();
            }
            Event::Clipboard { data, .. } => {
                // gpui has no primary-selection API, so OSC 52's `p` target
                // lands on the clipboard too.
                let text = String::from_utf8_lossy(data).into_owned();
                if self.clipboard_write && !text.is_empty() {
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                }
            }
            Event::Exit(_) => {
                self.exited = true;
                cx.notify();
            }
            Event::Bell
            | Event::Notify { .. }
            | Event::CommandFinished(_)
            | Event::DirChanged(_) => {}
        }
        cx.emit(event);
    }

    /// Handle new child output. While the program holds synchronized output
    /// (?2026), withhold the repaint so the frame lands atomically, but arm
    /// a short safety timer so a program that never clears ?2026 can't
    /// freeze the view.
    fn wakeup(&mut self, cx: &mut Context<Self>) {
        if self.session.with_term(|term| term.synchronized_output()) {
            if !self.sync_pending {
                self.sync_pending = true;
                let timer = cx.background_executor().timer(SYNC_TIMEOUT);
                cx.spawn(async move |this, cx| {
                    timer.await;
                    let _ = this.update(cx, |this, cx| {
                        if this.sync_pending {
                            this.sync_pending = false;
                            cx.notify();
                        }
                    });
                })
                .detach();
            }
            return;
        }
        self.sync_pending = false;
        cx.notify();
    }

    /// Capture phase: intercept Tab before gpui's focus traversal consumes
    /// it, so it reaches the shell (completion, TUIs).
    fn capture_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if event.keystroke.key == "tab" {
            self.key_down(event, window, cx);
        }
    }

    fn key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        let mut mods = input::Mods {
            shift: keystroke.modifiers.shift,
            alt: keystroke.modifiers.alt,
            ctrl: keystroke.modifiers.control,
            cmd: keystroke.modifiers.platform,
        };
        if self.clipboard_key(&keystroke.key, mods, cx) {
            cx.stop_propagation();
            return;
        }
        // macOS Option: either a real Alt for the pty or the platform's
        // character composer, per `option_as_alt`.
        let mut text = keystroke.key_char.as_deref();
        if cfg!(target_os = "macos") && keystroke.modifiers.alt && !keystroke.modifiers.platform {
            if self.option_as_alt {
                text = single_char(&keystroke.key);
            } else {
                mods.alt = false;
            }
        }
        let state = self.session.with_term(|term| input::TermState {
            cursor_keys_app: term.cursor_keys_app(),
            keypad_app: term.keypad_app(),
            bracketed_paste: term.bracketed_paste(),
            kitty_flags: term.kitty_keyboard_flags(),
        });
        // gpui re-fires key_down for auto-repeat with `is_held`; report those as
        // kitty repeat events, everything else as a press.
        let phase = if event.is_held {
            input::KeyEvent::Repeat
        } else {
            input::KeyEvent::Press
        };
        if let Some(bytes) = input::encode_key(&keystroke.key, text, mods, state, phase) {
            self.scroll_to_bottom(cx);
            let _ = self.session.write(&bytes);
            cx.stop_propagation();
        }
    }

    /// Key-release. Only the kitty keyboard protocol with event reporting turns
    /// a key-up into bytes (`encode_key` returns `None` otherwise), so this is a
    /// no-op in normal use.
    fn key_up(&mut self, event: &KeyUpEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        let mut mods = input::Mods {
            shift: keystroke.modifiers.shift,
            alt: keystroke.modifiers.alt,
            ctrl: keystroke.modifiers.control,
            cmd: keystroke.modifiers.platform,
        };
        let mut text = keystroke.key_char.as_deref();
        if cfg!(target_os = "macos") && keystroke.modifiers.alt && !keystroke.modifiers.platform {
            if self.option_as_alt {
                text = single_char(&keystroke.key);
            } else {
                mods.alt = false;
            }
        }
        let state = self.session.with_term(|term| input::TermState {
            cursor_keys_app: term.cursor_keys_app(),
            keypad_app: term.keypad_app(),
            bracketed_paste: term.bracketed_paste(),
            kitty_flags: term.kitty_keyboard_flags(),
        });
        if let Some(bytes) =
            input::encode_key(&keystroke.key, text, mods, state, input::KeyEvent::Release)
        {
            let _ = self.session.write(&bytes);
            cx.stop_propagation();
        }
    }

    /// Built-in clipboard shortcuts: Cmd+C/Cmd+V on macOS, Ctrl+Shift+C/V
    /// elsewhere. Copy falls through when nothing is selected, so the bare
    /// keystroke still reaches the pty where that matters.
    fn clipboard_key(&mut self, key: &str, mods: input::Mods, cx: &mut Context<Self>) -> bool {
        let held = if cfg!(target_os = "macos") {
            mods.cmd && !mods.ctrl && !mods.alt && !mods.shift
        } else {
            mods.ctrl && mods.shift && !mods.alt && !mods.cmd
        };
        if !held {
            return false;
        }
        match key {
            "c" => self.copy_selection(cx),
            "v" => {
                self.paste(cx);
                true
            }
            _ => false,
        }
    }
}

impl Focusable for TermView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl EventEmitter<Event> for TermView {}

impl Render for TermView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Pointing-hand cursor while the open-modifier hovers a link.
        let link_hover = self.mouse.borrow().hover_link.is_some();
        div()
            .size_full()
            .key_context("Terminal")
            .track_focus(&self.focus)
            .when(link_hover, |d| d.cursor_pointer())
            .capture_key_down(cx.listener(Self::capture_key))
            .on_key_down(cx.listener(Self::key_down))
            .on_key_up(cx.listener(Self::key_up))
            .child(TerminalElement::new(
                self.session.clone(),
                self.colors.clone(),
                self.font.clone(),
                self.font_size,
                self.cell,
                self.pad,
                self.cursor,
                self.mouse.clone(),
                self.copy_on_select,
                self.copy.clone(),
                self.smart_select,
                self.middle_click_paste,
                self.focused,
                None,
                None,
                self.image_cache.clone(),
                self.snap_cache.clone(),
            ))
    }
}
