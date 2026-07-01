//! One terminal pane: owns its session and handles input/events.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::prelude::*;
use gpui::{
    anchored, deferred, div, px, AnyElement, App, ClipboardItem, Context, EventEmitter,
    FocusHandle, Focusable, KeyDownEvent, MouseButton, MouseDownEvent, Pixels, Point, SharedString,
    Subscription, Window,
};
use config::{Action, SplitDirection};
use terminal::{Event, Session};

use crate::colors::{self, Colors};
use crate::element::TerminalElement;
use crate::metrics::{CellSize, Padding};
use crate::mouse::MouseState;

mod assist;
mod keys;
mod mouse;
mod notify;
mod scroll;
mod search;

pub use notify::{notify_command, post_os_notification};

/// Maximum time a frame is withheld for synchronized output before it is
/// painted anyway, so a stuck ?2026 cannot freeze the view (xterm/contour
/// use a similar bound).
const SYNC_TIMEOUT: Duration = Duration::from_millis(150);

/// Pane events the workspace root reacts to.
#[derive(Debug, Clone, PartialEq)]
pub enum ViewEvent {
    /// The vt title changed: refresh tab labels / the window title.
    Title,
    /// The child exited: close this pane.
    Exited,
    /// Typed input to mirror to sibling panes (broadcast mode). Carries the
    /// already-encoded pty bytes from the focused pane.
    Input(Vec<u8>),
    /// A config action picked from this pane's right-click menu; the workspace
    /// focuses this pane and dispatches it.
    Action(Action),
    /// This pane's attention state changed (a notification arrived, or focus
    /// cleared it); the workspace repaints the tab/pane indicator.
    Attention,
    /// A terminal event a plugin `[[trigger]]` may react to; the workspace
    /// matches it against loaded triggers and runs their actions.
    Trigger(TriggerEvent),
}

/// A terminal event a plugin `[[trigger]]` can hook, carried to the workspace
/// on [`ViewEvent::Trigger`]. Names match `plugin::TRIGGER_EVENTS`.
#[derive(Debug, Clone, PartialEq)]
pub enum TriggerEvent {
    /// A BEL was received.
    Bell,
    /// The window title changed.
    TitleChanged(String),
    /// The child requested a desktop notification (OSC 9/777/99).
    Notify {
        title: Option<String>,
        body: String,
    },
    /// The child process exited (exit code, or `None` when signalled).
    Exit(Option<i32>),
    /// A shell-integration command finished (OSC 133 `D`) with its exit code.
    CommandFinished(Option<i32>),
    /// The working directory changed (OSC 7).
    DirChanged(String),
}

impl TriggerEvent {
    /// The manifest event name this matches.
    pub fn name(&self) -> &'static str {
        match self {
            TriggerEvent::Bell => "bell",
            TriggerEvent::TitleChanged(_) => "title_changed",
            TriggerEvent::Notify { .. } => "notify",
            TriggerEvent::Exit(_) => "exit",
            TriggerEvent::CommandFinished(_) => "command_finished",
            TriggerEvent::DirChanged(_) => "dir_changed",
        }
    }

    /// The exit code for exit / command-finished events, else `None`.
    pub fn exit_code(&self) -> Option<Option<i32>> {
        match self {
            TriggerEvent::Exit(c) | TriggerEvent::CommandFinished(c) => Some(*c),
            _ => None,
        }
    }

    /// The text a `when` substring filter matches against (title/notify/dir).
    pub fn match_text(&self) -> Option<&str> {
        match self {
            TriggerEvent::TitleChanged(s) | TriggerEvent::DirChanged(s) => Some(s),
            TriggerEvent::Notify { body, .. } => Some(body),
            _ => None,
        }
    }

    /// JSON payload handed to a plugin runtime when the action is `invoke`.
    pub fn payload(&self) -> serde_json::Value {
        match self {
            TriggerEvent::Bell => serde_json::json!({ "event": "bell" }),
            TriggerEvent::TitleChanged(t) => {
                serde_json::json!({ "event": "title_changed", "title": t })
            }
            TriggerEvent::Notify { title, body } => {
                serde_json::json!({ "event": "notify", "title": title, "body": body })
            }
            TriggerEvent::Exit(c) => serde_json::json!({ "event": "exit", "exit_code": c }),
            TriggerEvent::CommandFinished(c) => {
                serde_json::json!({ "event": "command_finished", "exit_code": c })
            }
            TriggerEvent::DirChanged(d) => serde_json::json!({ "event": "dir_changed", "dir": d }),
        }
    }
}

/// Pane title: the vt title when set and non-blank, else the fallback.
pub fn label<'a>(title: Option<&'a str>, fallback: &'a str) -> &'a str {
    match title {
        Some(t) if !t.trim().is_empty() => t,
        _ => fallback,
    }
}

/// Scrollback search overlay state.
struct Search {
    edit: guise::TextEdit,
    /// Index of the focused match among current results.
    current: usize,
    /// The query `results` were computed for; `None` until first search.
    cached_query: Option<String>,
    /// Cached match list, reused across frames until the query changes or new
    /// output arrives, so the whole buffer isn't re-scanned every repaint.
    results: Vec<vt::Match>,
    /// Set when new output may have changed the matches.
    dirty: bool,
}

enum Assist {
    Semantic {
        edit: guise::TextEdit,
        current: usize,
        /// The query `hits` were computed for; `None` until the first scan.
        cached_query: Option<String>,
        /// Cached ranked hits, reused across frames until the query changes or
        /// new output arrives, so the whole buffer isn't re-scanned and
        /// re-allocated as owned Strings every repaint.
        hits: Vec<::assist::context::Hit>,
        /// Set when new output may have changed the hits.
        dirty: bool,
    },
    Compose {
        edit: guise::TextEdit,
    },
    Message {
        title: String,
        body: String,
    },
    Paste {
        text: String,
        risk: ::assist::PasteRisk,
    },
    ClipboardWrite {
        text: String,
    },
}

/// Config-derived appearance pushed to every pane on a live reload.
pub struct Appearance {
    pub colors: Rc<Colors>,
    pub font: gpui::Font,
    pub font_size: gpui::Pixels,
    pub cell: CellSize,
    pub pad: Padding,
    pub cursor_default: config::CursorStyle,
    pub copy_on_select: bool,
    pub option_as_alt: config::OptionAsAlt,
    pub paste_protection: bool,
    pub clipboard_write: config::ClipboardAccess,
}

pub struct TerminalView {
    session: Arc<Session>,
    colors: Rc<Colors>,
    font: gpui::Font,
    font_size: gpui::Pixels,
    cell: CellSize,
    pad: Padding,
    cursor_default: config::CursorStyle,
    copy_on_select: bool,
    /// How the macOS Option key is treated for pty input (`macos-option-as-alt`).
    option_as_alt: config::OptionAsAlt,
    /// When set, a risky paste prompts for confirmation before reaching the
    /// shell (`clipboard-paste-protection`).
    paste_protection: bool,
    /// Policy for program-initiated clipboard writes via OSC 52
    /// (`clipboard-write`): allow silently, ask first, or deny outright.
    clipboard_write: config::ClipboardAccess,
    /// Open right-click menu, at its window-coordinate anchor.
    context_menu: Option<Point<Pixels>>,
    /// Pointer state shared with the element's per-frame event closures.
    mouse: Rc<RefCell<MouseState>>,
    /// Decoded sixel textures, keyed by placement id; persists across frames.
    image_cache: Rc<RefCell<std::collections::HashMap<u64, Arc<gpui::RenderImage>>>>,
    focus: FocusHandle,
    /// Last vt title (OSC 0/2); `None` until the child sets one.
    title: Option<String>,
    /// User-set title; overrides the vt title and fallback when present.
    override_title: Option<String>,
    /// Title fallback: the shell name.
    fallback: String,
    /// When set, keystrokes and pastes are not forwarded to the pty.
    read_only: bool,
    /// Set when BEL arrives. TODO: visual bell.
    pub bell: bool,
    /// Set when this pane posts a desktop notification (OSC 9/777/99) while
    /// unfocused; drives the tab/pane attention indicator. Cleared on focus.
    attention: bool,
    /// Tracks pane focus (kept in sync by the focus-in/out subscriptions), so
    /// a notification only raises the attention indicator on a background pane.
    focused: bool,
    /// True while a repaint is being withheld for synchronized output
    /// (?2026), with a safety timer armed to release it.
    sync_pending: bool,
    /// Active scrollback search, if the overlay is open.
    search: Option<Search>,
    /// Active local-assist overlay, if any.
    assist: Option<Assist>,
    /// Focus in/out listeners that drive focus reporting (?1004).
    _focus_subs: [Subscription; 2],
}

impl TerminalView {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        session: Arc<Session>,
        colors: Rc<Colors>,
        font: gpui::Font,
        font_size: gpui::Pixels,
        cell: CellSize,
        pad: Padding,
        cursor_default: config::CursorStyle,
        copy_on_select: bool,
        option_as_alt: config::OptionAsAlt,
        paste_protection: bool,
        clipboard_write: config::ClipboardAccess,
        fallback: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        session.with_term(|term| term.set_report_colors(colors::report_colors(&colors)));
        let focus = cx.focus_handle();
        let on_in = cx.weak_entity();
        let sub_in = window.on_focus_in(&focus, cx, move |_window, cx| {
            let _ = on_in.update(cx, |this, cx| {
                this.focused = true;
                this.report_focus(true);
                this.clear_attention(cx);
            });
        });
        let on_out = cx.weak_entity();
        let sub_out = window.on_focus_out(&focus, cx, move |_event, _window, cx| {
            let _ = on_out.update(cx, |this, _| {
                this.focused = false;
                this.report_focus(false);
            });
        });
        Self {
            session,
            colors,
            font,
            font_size,
            cell,
            pad,
            cursor_default,
            copy_on_select,
            option_as_alt,
            paste_protection,
            clipboard_write,
            context_menu: None,
            mouse: Rc::new(RefCell::new(MouseState::default())),
            image_cache: Rc::new(RefCell::new(std::collections::HashMap::new())),
            focus,
            title: None,
            override_title: None,
            fallback,
            read_only: false,
            bell: false,
            attention: false,
            focused: false,
            sync_pending: false,
            search: None,
            assist: None,
            _focus_subs: [sub_in, sub_out],
        }
    }

    /// Emit a focus-in/out report to the child if it enabled ?1004, then
    /// flush the bytes to the pty.
    fn report_focus(&self, focused: bool) {
        let out = self.session.with_term(|term| {
            term.report_focus(focused);
            term.take_output()
        });
        if !out.is_empty() {
            let _ = self.session.write(&out);
        }
    }

    /// Current pane title for tab labels and the window title.
    pub fn title(&self) -> &str {
        match self.override_title.as_deref() {
            Some(t) => t,
            None => label(self.title.as_deref(), &self.fallback),
        }
    }

    /// Override the pane title (empty string clears it back to the vt title).
    pub fn set_title_override(&mut self, title: &str, cx: &mut Context<Self>) {
        self.override_title = (!title.trim().is_empty()).then(|| title.trim().to_string());
        cx.emit(ViewEvent::Title);
        cx.notify();
    }

    pub fn is_read_only(&self) -> bool {
        self.read_only
    }

    /// Toggle input gating; returns the new state.
    pub fn toggle_read_only(&mut self, cx: &mut Context<Self>) -> bool {
        self.read_only = !self.read_only;
        cx.notify();
        self.read_only
    }

    /// Current grid size as `(cols, rows)`.
    pub fn grid_size(&self) -> (usize, usize) {
        self.session.with_term(|term| (term.cols(), term.rows()))
    }

    /// Apply a reloaded appearance. A font/size change shifts the cell box,
    /// so the next layout pass re-grids and resizes the session; here we
    /// just swap the fields and repaint.
    pub fn set_appearance(&mut self, a: &Appearance, cx: &mut Context<Self>) {
        self.colors = a.colors.clone();
        self.font = a.font.clone();
        self.font_size = a.font_size;
        self.cell = a.cell;
        self.pad = a.pad;
        self.cursor_default = a.cursor_default;
        self.copy_on_select = a.copy_on_select;
        self.option_as_alt = a.option_as_alt;
        self.paste_protection = a.paste_protection;
        self.clipboard_write = a.clipboard_write;
        self.session
            .with_term(|term| term.set_report_colors(colors::report_colors(&self.colors)));
        cx.notify();
    }

    /// The pane's OSC 7 working directory report, if any.
    pub fn cwd(&self) -> Option<String> {
        self.session
            .with_term(|term| term.cwd().map(str::to_string))
    }

    /// The pane's whole buffer (scrollback + screen) as plain text.
    pub fn buffer_text(&self) -> String {
        self.session.with_term(|term| term.buffer_text())
    }

    /// Apply one session event; called from the bridge task.
    pub fn apply(&mut self, event: Event, cx: &mut Context<Self>) {
        match event {
            Event::Wakeup => self.wakeup(cx),
            Event::TitleChanged(title) => {
                self.title = Some(title.clone());
                cx.emit(ViewEvent::Title);
                cx.emit(ViewEvent::Trigger(TriggerEvent::TitleChanged(title)));
            }
            Event::Bell => {
                self.bell = true;
                cx.emit(ViewEvent::Trigger(TriggerEvent::Bell));
            }
            Event::Notify { title, body } => {
                let heading = title.clone().unwrap_or_else(|| self.title().to_string());
                post_os_notification(&heading, &body);
                if !self.focused {
                    self.attention = true;
                    cx.emit(ViewEvent::Attention);
                    cx.notify();
                }
                cx.emit(ViewEvent::Trigger(TriggerEvent::Notify { title, body }));
            }
            Event::Clipboard { data, .. } => {
                let text = String::from_utf8_lossy(&data).into_owned();
                match self.clipboard_write {
                    config::ClipboardAccess::Deny => {}
                    config::ClipboardAccess::Ask => {
                        self.search = None;
                        self.assist = Some(Assist::ClipboardWrite { text });
                        cx.notify();
                    }
                    config::ClipboardAccess::Allow => self.write_clipboard(text, cx),
                }
            }
            Event::CommandFinished(code) => {
                cx.emit(ViewEvent::Trigger(TriggerEvent::CommandFinished(code)))
            }
            Event::DirChanged(dir) => cx.emit(ViewEvent::Trigger(TriggerEvent::DirChanged(dir))),
            Event::Exit(code) => {
                cx.emit(ViewEvent::Trigger(TriggerEvent::Exit(code)));
                cx.emit(ViewEvent::Exited);
            }
        }
    }

    /// Handle new child output. While the program holds synchronized output
    /// (?2026), withhold the repaint so the frame lands atomically, but arm
    /// a short safety timer so a program that never clears ?2026 can't
    /// freeze the view.
    fn wakeup(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = &mut self.search {
            s.dirty = true;
        }
        if let Some(Assist::Semantic { dirty, .. }) = &mut self.assist {
            *dirty = true;
        }
        if self.session.with_term(|t| t.synchronized_output()) {
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

    /// Whether a foreground process (beyond the shell) is running in this pane.
    pub fn has_running_process(&self) -> bool {
        self.session.foreground_running()
    }

    /// Whether this pane has a pending notification awaiting the user's eyes.
    pub fn needs_attention(&self) -> bool {
        self.attention
    }

    /// This pane's working directory as a path (from OSC 7), if reported.
    pub fn cwd_path(&self) -> Option<std::path::PathBuf> {
        self.cwd().and_then(|osc| crate::session::cwdpath(&osc))
    }

    /// Clear the attention indicator (the user is now looking at this pane).
    fn clear_attention(&mut self, cx: &mut Context<Self>) {
        if self.attention {
            self.attention = false;
            cx.emit(ViewEvent::Attention);
            cx.notify();
        }
    }
}

impl EventEmitter<ViewEvent> for TerminalView {}

impl Focusable for TerminalView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for TerminalView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let matches = self.search_matches();
        let total = matches.len();
        let query = self.search.as_ref().map(move |s| crate::element::SearchQuery {
            query: s.edit.text(),
            current: s.current,
            matches,
        });
        let bar = self.search.as_ref().map(|s| {
            let pos = if total == 0 { 0 } else { s.current + 1 };
            let (before, after) = s.edit.split();
            (pos, before, after)
        });
        let bar = bar.map(|(pos, before, after)| self.search_bar(&before, &after, pos, total, cx));
        let hits = self.semantic_hits();
        let assist = self.assist_panel(&hits);
        let menu = self
            .context_menu
            .map(|pos| self.context_menu_overlay(pos, cx));
        div()
            .relative()
            .size_full()
            .bg(colors::rgba(self.colors.bg))
            .key_context("Terminal")
            .track_focus(&self.focus)
            .capture_key_down(cx.listener(Self::capture_key))
            .on_key_down(cx.listener(Self::key_down))
            .on_mouse_down(MouseButton::Right, cx.listener(Self::right_down))
            .child(TerminalElement::new(
                self.session.clone(),
                self.colors.clone(),
                self.font.clone(),
                self.font_size,
                self.cell,
                self.pad,
                self.cursor_default,
                self.mouse.clone(),
                self.copy_on_select,
                query,
                self.image_cache.clone(),
            ))
            .children(bar)
            .children(assist)
            .children(menu)
    }
}

#[cfg(test)]
use keys::{layout_wants_alt, option_policy};

#[cfg(test)]
#[path = "../../tests/view.rs"]
mod tests;
