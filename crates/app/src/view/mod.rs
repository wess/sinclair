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
use libsinclair::element::{CursorShape, TerminalElement};
use libsinclair::metrics::{CellSize, Padding};
use libsinclair::mouse::MouseState;
use libsinclair::pointer::CopyHook;

mod annotate;
mod assist;
mod copymode;
mod hints;
mod keys;
mod mouse;
mod notify;
mod paneopts;
mod scroll;
mod search;
mod suggest;
mod timestamps;

pub use notify::{notify_command, post_os_notification};
pub use timestamps::install as install_timestamps;

/// Maximum time a frame is withheld for synchronized output before it is
/// painted anyway, so a stuck ?2026 cannot freeze the view.
const SYNC_TIMEOUT: Duration = Duration::from_millis(150);

/// How long the visual bell flash stays on screen.
const BELL_FLASH: Duration = Duration::from_millis(120);

/// How long a pane must be quiet before idle scrollback compaction starts.
const COMPACT_IDLE: Duration = Duration::from_millis(500);

/// Gap between compaction steps (one compressed block per step), so the
/// terminal lock is only ever held for a single block encode at a time.
const COMPACT_TICK: Duration = Duration::from_millis(80);

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
    /// Real gpui focus entered this pane (e.g. its body was clicked); the
    /// workspace re-activates the item so guise's focused pane tracks it.
    Focused,
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
    /// A git worktree was created at this path (fired by the worktree verbs, not
    /// a pane's own output).
    WorktreeCreated(String),
    /// A git worktree was removed from this path.
    WorktreeRemoved(String),
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
            TriggerEvent::WorktreeCreated(_) => "worktree_created",
            TriggerEvent::WorktreeRemoved(_) => "worktree_removed",
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
            TriggerEvent::TitleChanged(s)
            | TriggerEvent::DirChanged(s)
            | TriggerEvent::WorktreeCreated(s)
            | TriggerEvent::WorktreeRemoved(s) => Some(s),
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
            TriggerEvent::WorktreeCreated(p) => {
                serde_json::json!({ "event": "worktree_created", "path": p })
            }
            TriggerEvent::WorktreeRemoved(p) => {
                serde_json::json!({ "event": "worktree_removed", "path": p })
            }
        }
    }
}

/// Map the configured cursor style onto the renderer's shape vocabulary.
fn cursor_shape(style: config::CursorStyle) -> CursorShape {
    match style {
        config::CursorStyle::Block => CursorShape::Block,
        config::CursorStyle::Bar => CursorShape::Bar,
        config::CursorStyle::Underline => CursorShape::Underline,
    }
}

/// Pane title: the vt title when set and non-blank, else the fallback.
pub fn label<'a>(title: Option<&'a str>, fallback: &'a str) -> &'a str {
    match title {
        Some(t) if !t.trim().is_empty() => t,
        _ => fallback,
    }
}

/// Schedule a repaint from a focus in/out listener. gpui dispatches these at
/// the end of a draw, inside the draw's focus phase, where `notify` cannot
/// mark the window dirty — the `focused`/`pane_active` flip would sit
/// unpainted until the next unrelated redraw, leaving an idle pane's cursor
/// stuck hollow (or filled) after a focus switch. Deferring the notify lands
/// it after the draw, where it schedules a real frame.
fn repaint_after_focus_change(view: &gpui::WeakEntity<TerminalView>, cx: &mut App) {
    let view = view.clone();
    cx.defer(move |cx| {
        let _ = view.update(cx, |_, cx| cx.notify());
    });
}

/// Bind a view's focus listeners to `window`. Split out of `new` because these
/// are per-*window* subscriptions: a torn-off pane keeps its `TerminalView` but
/// lands in a different window, and listeners still pointed at the old one see
/// that window's activation instead of their own (see `rehome`).
fn focus_subs(
    focus: &gpui::FocusHandle,
    window: &mut Window,
    cx: &mut Context<TerminalView>,
) -> [Subscription; 3] {
    let on_in = cx.weak_entity();
    let sub_in = window.on_focus_in(focus, cx, move |_window, cx| {
        let _ = on_in.update(cx, |this, cx| {
            this.focused = true;
            this.pane_active = true;
            this.report_focus(true);
            this.clear_attention(cx);
            cx.emit(ViewEvent::Focused);
        });
        repaint_after_focus_change(&on_in, cx);
    });
    let on_out = cx.weak_entity();
    let sub_out = window.on_focus_out(focus, cx, move |_event, window, cx| {
        // `focused` tracks true focus (window + pane) — it drives ?1004
        // reporting and the background-notification attention dot, both of
        // which should react to the window losing focus. `pane_active`
        // tracks the active *pane* only and drives the split dimming +
        // hidden cursor, so it stays set when the whole window merely
        // deactivates (which also fires focus-out); only a real pane switch,
        // where the window is still active, clears it.
        let pane_switch = window.is_window_active();
        let _ = on_out.update(cx, |this, _| {
            this.focused = false;
            this.report_focus(false);
            if pane_switch {
                this.pane_active = false;
            }
        });
        repaint_after_focus_change(&on_out, cx);
    });
    // gpui derives focus in/out from the drawn frame and folds window
    // activation into it, so a frame drawn before the platform reports the
    // window active carries an empty focus path. A window that gains focus and
    // activation together can therefore settle with neither flag set, painting
    // a hollow cursor in the pane the user is typing into. Activation is the
    // one signal that outranks the frame — re-derive both flags from it.
    let sub_act = cx.observe_window_activation(window, |this, window, cx| {
        let active = window.is_window_active();
        let mine = this.focus.contains_focused(window, cx);
        if active {
            this.pane_active = mine;
        }
        let focused = active && mine;
        if this.focused != focused {
            this.focused = focused;
            this.report_focus(focused);
            if focused {
                this.clear_attention(cx);
            }
        }
        cx.notify();
    });
    [sub_in, sub_out, sub_act]
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
    /// Shared by `Rc`, so handing it to the renderer never deep-copies it.
    results: Rc<Vec<vt::Match>>,
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
        /// The last submitted request matched nothing; shown until edited.
        miss: bool,
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
        /// OSC 52 aimed at the primary selection rather than the clipboard.
        primary: bool,
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
    pub smart_select: bool,
    pub middle_click_paste: bool,
    pub option_as_alt: config::OptionAsAlt,
    pub paste_protection: bool,
    pub clipboard_write: config::ClipboardAccess,
    pub suggest: crate::suggest::SuggestConfig,
    /// Opacity applied to a pane while it is not focused, so the active split is
    /// obvious. `1.0` disables the dimming.
    pub unfocused_split_opacity: f32,
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
    /// Double-click selects the semantic token under the pointer (`smart-select`).
    smart_select: bool,
    /// Middle-click pastes the current selection (`middle-click-paste`).
    middle_click_paste: bool,
    /// How the macOS Option key is treated for pty input (`macos-option-as-alt`).
    option_as_alt: config::OptionAsAlt,
    /// When set, a risky paste prompts for confirmation before reaching the
    /// shell (`clipboard-paste-protection`).
    paste_protection: bool,
    /// Policy for program-initiated clipboard writes via OSC 52
    /// (`clipboard-write`): allow silently, ask first, or deny outright.
    clipboard_write: config::ClipboardAccess,
    /// Opacity applied while this pane is unfocused, so the active split reads
    /// clearly. `1.0` means no dimming.
    unfocused_split_opacity: f32,
    /// Open right-click menu, at its window-coordinate anchor.
    context_menu: Option<Point<Pixels>>,
    /// The grid's window-space bounds, captured each frame, so a right-click
    /// position can be mapped back to a cell (and thus a link) for the menu.
    grid_bounds: gpui::Bounds<Pixels>,
    /// Pointer state shared with the element's per-frame event closures.
    mouse: Rc<RefCell<MouseState>>,
    /// Copy-on-select hook handed to the element: redaction and clipboard
    /// history layered over the plain clipboard write.
    copy: Rc<CopyHook>,
    /// Decoded sixel textures, keyed by placement id; persists across frames.
    image_cache: Rc<RefCell<std::collections::HashMap<u64, Arc<gpui::RenderImage>>>>,
    /// Previous frame's render snapshot, reused while vt reports no damage
    /// and every snapshot input is unchanged.
    snap_cache: Rc<RefCell<libsinclair::element::SnapCache>>,
    focus: FocusHandle,
    /// Last vt title (OSC 0/2); `None` until the child sets one.
    title: Option<String>,
    /// User-set title; overrides the vt title and fallback when present.
    override_title: Option<String>,
    /// Title fallback: the shell name.
    fallback: String,
    /// When set, keystrokes and pastes are not forwarded to the pty.
    read_only: bool,
    /// Set while the visual bell flashes (BEL arrived with `visual-bell` on);
    /// a short timer clears it.
    pub bell: bool,
    /// Set when this pane posts a desktop notification (OSC 9/777/99) while
    /// unfocused; drives the tab/pane attention indicator. Cleared on focus.
    attention: bool,
    /// Tracks true focus — window *and* pane — kept in sync by the focus-in/out
    /// subscriptions. Drives ?1004 reporting and the attention indicator (a
    /// background notification), both of which react to the window losing focus.
    focused: bool,
    /// Tracks the active *pane* only — unlike `focused` it survives the whole
    /// window losing focus. Drives the unfocused-split dimming and the hidden
    /// cursor, so those reflect which split is active, not window focus.
    pane_active: bool,
    /// True while a repaint is being withheld for synchronized output
    /// (?2026), with a safety timer armed to release it.
    sync_pending: bool,
    /// Active scrollback search, if the overlay is open.
    search: Option<Search>,
    /// Active hint mode (keyboard link-following), if open.
    hints: Option<hints::Hints>,
    /// Active copy mode (vi-style keyboard selection), if open.
    copy_mode: Option<copymode::CopyMode>,
    /// High-water stable line sequence (vt `committed_lines` space) already
    /// scanned for output triggers; `u64::MAX` until the first scan (which
    /// skips pre-existing content).
    trigger_hwm: u64,
    /// Rate limiter for this pane's desktop notifications (OSC 9/777/99 and
    /// output triggers), so bursts can't spawn unbounded helper processes.
    notify_limit: notify::NotifyLimit,
    /// Capture times (epoch secs) parallel to scrollback, for the timestamp
    /// gutter; kept aligned via vt's `committed_lines`.
    line_times: std::collections::VecDeque<u64>,
    /// vt committed-lines mark; `u64::MAX` until the first timestamp scan.
    committed_last: u64,
    /// Line annotations, keyed by stable line sequence.
    annotations: std::collections::HashMap<u64, String>,
    /// Active local-assist overlay, if any.
    assist: Option<Assist>,
    /// Enabled autosuggestion behaviors/sources.
    suggest_cfg: crate::suggest::SuggestConfig,
    /// Live autosuggestion state (candidates, ghost, popup, tab-cycle, AI).
    suggest: crate::suggest::Suggest,
    /// Wakeup counter; the idle compaction task compares it across its waits
    /// to detect new output.
    activity: u64,
    /// True while an idle compaction task is scheduled for this pane.
    compact_armed: bool,
    /// Focus in/out listeners plus the window-activation resync; together they
    /// drive focus reporting (?1004) and the focused/unfocused cursor.
    _focus_subs: [Subscription; 3],
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
        smart_select: bool,
        middle_click_paste: bool,
        option_as_alt: config::OptionAsAlt,
        paste_protection: bool,
        clipboard_write: config::ClipboardAccess,
        unfocused_split_opacity: f32,
        suggest_cfg: crate::suggest::SuggestConfig,
        fallback: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        session.with_term(|term| term.set_report_colors(colors::report_colors(&colors)));
        let chars = paneopts::word_chars(cx);
        session.with_term(|term| term.set_word_chars(&chars));
        let focus = cx.focus_handle();
        let subs = focus_subs(&focus, window, cx);
        Self {
            session,
            colors,
            font,
            font_size,
            cell,
            pad,
            cursor_default,
            copy_on_select,
            smart_select,
            middle_click_paste,
            option_as_alt,
            paste_protection,
            clipboard_write,
            unfocused_split_opacity,
            context_menu: None,
            grid_bounds: gpui::Bounds::default(),
            mouse: Rc::new(RefCell::new(MouseState::default())),
            copy: Rc::new(|text, cx| {
                let text = crate::redact::mask(text, cx);
                crate::clipboard::remember(&text, cx);
                cx.write_to_clipboard(ClipboardItem::new_string(text));
            }),
            image_cache: Rc::new(RefCell::new(std::collections::HashMap::new())),
            snap_cache: Rc::new(RefCell::new(libsinclair::element::SnapCache::default())),
            focus,
            title: None,
            override_title: None,
            fallback,
            read_only: false,
            bell: false,
            attention: false,
            focused: false,
            pane_active: false,
            sync_pending: false,
            search: None,
            hints: None,
            copy_mode: None,
            trigger_hwm: u64::MAX,
            notify_limit: notify::NotifyLimit::default(),
            line_times: std::collections::VecDeque::new(),
            committed_last: u64::MAX,
            annotations: std::collections::HashMap::new(),
            assist: None,
            suggest_cfg,
            suggest: crate::suggest::Suggest::default(),
            activity: 0,
            compact_armed: false,
            _focus_subs: subs,
        }
    }

    /// Move this view's focus listeners onto `window`. A torn-off pane keeps
    /// its `TerminalView` — and with it subscriptions bound to the window it
    /// was dragged out of, which go on reporting *that* window's focus and
    /// activation. Without this the pane arrives in its new window with
    /// `pane_active` still cleared by the tear-off's focus-out and no listener
    /// left that can set it again, so it paints a hollow cursor until the user
    /// clicks away and back.
    pub(crate) fn rehome(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self._focus_subs = focus_subs(&self.focus, window, cx);
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
        self.smart_select = a.smart_select;
        self.middle_click_paste = a.middle_click_paste;
        self.option_as_alt = a.option_as_alt;
        self.paste_protection = a.paste_protection;
        self.clipboard_write = a.clipboard_write;
        self.unfocused_split_opacity = a.unfocused_split_opacity;
        self.suggest_cfg = a.suggest;
        self.session.with_term(|term| {
            term.set_report_colors(colors::report_colors(&self.colors));
        });
        // A reload delivered a fresh appearance; pick up the pane options
        // that ride outside it in the same pass.
        paneopts::refresh(cx);
        let chars = paneopts::word_chars(cx);
        self.session.with_term(|term| term.set_word_chars(&chars));
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
                self.ring_bell(cx);
                cx.emit(ViewEvent::Trigger(TriggerEvent::Bell));
            }
            Event::Notify { title, body } => {
                let heading = title.clone().unwrap_or_else(|| self.title().to_string());
                self.post_notification(&heading, &body);
                if !self.focused {
                    self.attention = true;
                    cx.emit(ViewEvent::Attention);
                    cx.notify();
                }
                cx.emit(ViewEvent::Trigger(TriggerEvent::Notify { title, body }));
            }
            Event::Clipboard { kind, data } => {
                let text = String::from_utf8_lossy(&data).into_owned();
                // OSC 52 can aim at the primary selection (`p`) rather than
                // the clipboard; honor that instead of clobbering the system
                // clipboard.
                let primary = kind.contains('p') && !kind.contains('c');
                match self.clipboard_write {
                    config::ClipboardAccess::Deny => {}
                    config::ClipboardAccess::Ask => {
                        self.search = None;
                        self.assist = Some(Assist::ClipboardWrite { text, primary });
                        cx.notify();
                    }
                    config::ClipboardAccess::Allow => self.write_clipboard(text, primary, cx),
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
        self.activity += 1;
        self.arm_compaction(cx);
        self.scan_triggers(cx);
        self.update_line_times(cx);
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
        self.recompute_suggestions(cx);
        cx.notify();
    }

    /// Schedule idle scrollback compaction: once this pane has been quiet
    /// for [`COMPACT_IDLE`], compress one history block per [`COMPACT_TICK`]
    /// until vt reports no work left, backing off whenever output resumes.
    /// One task per pane; wakeups while it runs just bump `activity`.
    fn arm_compaction(&mut self, cx: &mut Context<Self>) {
        if self.compact_armed {
            return;
        }
        self.compact_armed = true;
        let session = self.session.clone();
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            loop {
                let Ok(seen) = this.update(cx, |this, _| this.activity) else {
                    return;
                };
                executor.timer(COMPACT_IDLE).await;
                let Ok(now) = this.update(cx, |this, _| this.activity) else {
                    return;
                };
                if now != seen {
                    continue; // output arrived; wait out a fresh idle window
                }
                loop {
                    if !session.with_term(|t| t.compact_scrollback()) {
                        // Done. Disarm, unless output slipped in meanwhile -
                        // then keep the task and go back to the idle wait.
                        let Ok(restart) = this.update(cx, |this, _| {
                            if this.activity != seen {
                                true
                            } else {
                                this.compact_armed = false;
                                false
                            }
                        }) else {
                            return;
                        };
                        if restart {
                            break;
                        }
                        return;
                    }
                    executor.timer(COMPACT_TICK).await;
                    let Ok(cur) = this.update(cx, |this, _| this.activity) else {
                        return;
                    };
                    if cur != seen {
                        break; // output resumed mid-compaction
                    }
                }
            }
        })
        .detach();
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

    /// Flash the pane briefly on BEL when `visual-bell` is enabled.
    fn ring_bell(&mut self, cx: &mut Context<Self>) {
        if self.bell || !paneopts::visual_bell(cx) {
            return;
        }
        self.bell = true;
        cx.notify();
        let timer = cx.background_executor().timer(BELL_FLASH);
        cx.spawn(async move |this, cx| {
            timer.await;
            let _ = this.update(cx, |this, cx| {
                this.bell = false;
                cx.notify();
            });
        })
        .detach();
    }

    /// A subtle whole-pane flash while the visual bell is ringing.
    fn bell_overlay(&self) -> Option<AnyElement> {
        if !self.bell {
            return None;
        }
        let mut flash = colors::hsla(self.colors.fg);
        flash.a = 0.12;
        Some(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .bg(flash)
                .into_any_element(),
        )
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
        let query = self.search.as_ref().map(move |s| libsinclair::element::SearchQuery {
            query: s.edit.text(),
            current: s.current,
            matches,
        });
        let bar = self.search.as_ref().map(|s| {
            let pos = if total == 0 { 0 } else { s.current + 1 };
            match s.edit.split_selection() {
                Some((before, selected, after)) => (pos, before, Some(selected), after),
                None => {
                    let (before, after) = s.edit.split();
                    (pos, before, None, after)
                }
            }
        });
        let bar = bar.map(|(pos, before, selected, after)| {
            self.search_bar(&before, selected.as_deref(), &after, pos, total, cx)
        });
        let hits = self.semantic_hits();
        let assist = self.assist_panel(&hits);
        let menu = self
            .context_menu
            .map(|pos| self.context_menu_overlay(pos, cx));
        // Dim a pane while it is not the active split so the active one is
        // obvious; `1.0` (the opt default when unset) leaves it untouched. Keyed
        // off `pane_active`, not `focused`, so backgrounding the window doesn't
        // dim every pane.
        let dim = if self.pane_active {
            1.0
        } else {
            self.unfocused_split_opacity.clamp(0.0, 1.0)
        };
        // Pointing-hand cursor while the open-modifier hovers a link.
        let link_hover = self.mouse.borrow().hover_link.is_some();
        div()
            .relative()
            .size_full()
            .opacity(dim)
            .when(link_hover, |d| d.cursor_pointer())
            .key_context("Terminal")
            .track_focus(&self.focus)
            .capture_key_down(cx.listener(Self::capture_key))
            .on_key_down(cx.listener(Self::key_down))
            .on_mouse_down(MouseButton::Right, cx.listener(Self::right_down))
            .child({
                // Record the grid's bounds each frame for the context menu's
                // position-to-link mapping (see `context_menu_overlay`).
                let this = cx.entity();
                gpui::canvas(
                    move |bounds, _window, cx| {
                        this.update(cx, |view, _| view.grid_bounds = bounds);
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full()
            })
            .child(TerminalElement::new(
                self.session.clone(),
                self.colors.clone(),
                self.font.clone(),
                self.font_size,
                self.cell,
                self.pad,
                cursor_shape(self.cursor_default),
                self.mouse.clone(),
                self.copy_on_select,
                self.copy.clone(),
                self.smart_select,
                self.middle_click_paste,
                self.pane_active,
                query,
                self.suggestion_ghost(),
                self.image_cache.clone(),
                self.snap_cache.clone(),
            ))
            .children(self.bell_overlay())
            .children(self.badge_overlay(cx))
            .children(self.suggestion_popup_overlay(cx))
            .children(self.timestamps_overlay(cx))
            .children(self.annotations_overlay())
            .children(self.hints_overlay())
            .children(self.copy_cursor_overlay())
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
