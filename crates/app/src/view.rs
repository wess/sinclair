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

/// Maximum time a frame is withheld for synchronized output before it is
/// painted anyway, so a stuck ?2026 cannot freeze the view (xterm/contour
/// use a similar bound).
const SYNC_TIMEOUT: Duration = Duration::from_millis(150);

/// Macro replay: how often the prompt-mark count is polled while waiting for
/// a replayed command to finish.
const REPLAY_POLL: Duration = Duration::from_millis(40);
/// Macro replay: give up waiting for a command's next prompt after this and
/// send the following command anyway.
const REPLAY_TIMEOUT: Duration = Duration::from_secs(20);
/// Macro replay gap used when the shell emits no OSC 133 prompt marks, so
/// pacing cannot key off a fresh prompt.
const REPLAY_FALLBACK_GAP: Duration = Duration::from_millis(150);

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
    edit: crate::textedit::TextEdit,
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
        edit: crate::textedit::TextEdit,
        current: usize,
    },
    Compose {
        edit: crate::textedit::TextEdit,
    },
    Message {
        title: String,
        body: String,
    },
    Paste {
        text: String,
        risk: assist::PasteRisk,
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

/// The key name as text only when it is a single character (a literal key
/// such as `b` or `/`), never a named key like `left` or `enter`.
fn single_char(key: &str) -> Option<&str> {
    let mut chars = key.chars();
    match (chars.next(), chars.next()) {
        (Some(_), None) => Some(key),
        _ => None,
    }
}

/// The `macos-option-as-alt` decision for one keystroke: the effective
/// modifiers and the text `encode_key` should use. Pure so it can be tested
/// off the macOS event path. On non-macOS, or when Option isn't held, the
/// platform's composed text passes straight through.
fn option_policy<'k>(
    policy: config::OptionAsAlt,
    is_macos: bool,
    option_held: bool,
    key: &'k str,
    key_char: Option<&'k str>,
    mut mods: input::Mods,
) -> (input::Mods, Option<&'k str>) {
    if !is_macos || !option_held {
        return (mods, key_char);
    }
    match policy {
        // Option composes (´, ∫, …): don't treat it as Alt. Drop the modifier
        // so arrows stay plain (`ESC[D`) and emit the glyph macOS produced.
        config::OptionAsAlt::False => {
            mods.alt = false;
            (mods, key_char)
        }
        // Option is Alt/Meta: ESC-prefix the *base* key (Option+b -> ESC b)
        // rather than the composed glyph. Named keys (arrows, etc.) carry no
        // text and encode from `key` directly.
        _ => (mods, single_char(key)),
    }
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
        fallback: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        // Install the colors the child can query via OSC 4/10/11/12.
        session.with_term(|term| term.set_report_colors(colors::report_colors(&colors)));
        let focus = cx.focus_handle();
        // Focus reporting (?1004): tell the child when this pane gains or
        // loses focus.
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
        self.session
            .with_term(|term| term.set_report_colors(colors::report_colors(&self.colors)));
        cx.notify();
    }

    /// The pane's OSC 7 working directory report, if any.
    pub fn cwd(&self) -> Option<String> {
        self.session
            .with_term(|term| term.cwd().map(str::to_string))
    }

    /// Apply one session event; called from the bridge task.
    pub fn apply(&mut self, event: Event, cx: &mut Context<Self>) {
        match event {
            Event::Wakeup => self.wakeup(cx),
            Event::TitleChanged(title) => {
                self.title = Some(title);
                cx.emit(ViewEvent::Title);
            }
            Event::Bell => self.bell = true,
            Event::Notify { title, body } => {
                // Post a native banner, and — if we're not the focused pane —
                // raise the in-app attention indicator on the tab/pane.
                let heading = title.unwrap_or_else(|| self.title().to_string());
                post_os_notification(&heading, &body);
                if !self.focused {
                    self.attention = true;
                    cx.emit(ViewEvent::Attention);
                    cx.notify();
                }
            }
            Event::Clipboard { data, .. } => {
                // OSC 52 write; macOS has no primary selection, so any kind
                // goes to the system clipboard.
                let text = String::from_utf8_lossy(&data).into_owned();
                cx.write_to_clipboard(ClipboardItem::new_string(text));
            }
            Event::Exit(_) => cx.emit(ViewEvent::Exited),
        }
    }

    /// Handle new child output. While the program holds synchronized output
    /// (?2026), withhold the repaint so the frame lands atomically, but arm
    /// a short safety timer so a program that never clears ?2026 can't
    /// freeze the view.
    fn wakeup(&mut self, cx: &mut Context<Self>) {
        // New output may have changed the search hits; invalidate the cache.
        if let Some(s) = &mut self.search {
            s.dirty = true;
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

    /// Open/close the scrollback search overlay.
    pub fn toggle_search(&mut self, cx: &mut Context<Self>) {
        self.assist = None;
        self.search = match self.search {
            Some(_) => None,
            None => Some(Search {
                edit: crate::textedit::TextEdit::new(""),
                current: 0,
                cached_query: None,
                results: Vec::new(),
                dirty: true,
            }),
        };
        cx.notify();
    }

    /// Current search results, cached until the query changes or new output
    /// marks them stale — the full-buffer scan never runs on an idle repaint.
    fn search_matches(&mut self) -> Vec<vt::Match> {
        let q = match &self.search {
            Some(s) => s.edit.text(),
            None => return Vec::new(),
        };
        if let Some(s) = &self.search {
            if !s.dirty && s.cached_query.as_deref() == Some(q.as_str()) {
                return s.results.clone();
            }
        }
        let hits = if q.is_empty() {
            Vec::new()
        } else {
            self.session.with_term(|t| t.search(&q, false))
        };
        if let Some(s) = &mut self.search {
            s.results = hits.clone();
            s.cached_query = Some(q);
            s.dirty = false;
        }
        hits
    }

    /// Clamp the focused match and scroll it into view.
    fn search_jump(&mut self, cx: &mut Context<Self>) {
        let matches = self.search_matches();
        let Some(s) = self.search.as_mut() else {
            return;
        };
        if matches.is_empty() {
            cx.notify();
            return;
        }
        s.current = s.current.min(matches.len() - 1);
        let line = matches[s.current].line;
        self.session.with_term(|t| {
            let sb = t.grid().scrollback().len();
            t.set_display_offset(sb.saturating_sub(line));
        });
        cx.notify();
    }

    /// Move the focused match by `delta`, wrapping.
    fn search_step(&mut self, delta: i64, cx: &mut Context<Self>) {
        let len = self.search_matches().len() as i64;
        if len == 0 {
            cx.notify();
            return;
        }
        if let Some(s) = self.search.as_mut() {
            s.current = (((s.current as i64 + delta) % len + len) % len) as usize;
        }
        self.search_jump(cx);
    }

    /// Handle a keystroke while the search overlay is open.
    fn search_key(&mut self, ks: &gpui::Keystroke, mods: input::Mods, cx: &mut Context<Self>) {
        if mods.cmd {
            return; // leave cmd chords (incl. toggle) to the action system
        }
        match ks.key.as_str() {
            "escape" => {
                self.search = None;
                cx.notify();
            }
            "enter" | "down" => self.search_step(1, cx),
            "up" => self.search_step(-1, cx),
            "left" => {
                if let Some(s) = self.search.as_mut() {
                    s.edit.left();
                }
                cx.notify();
            }
            "right" => {
                if let Some(s) = self.search.as_mut() {
                    s.edit.right();
                }
                cx.notify();
            }
            "home" => {
                if let Some(s) = self.search.as_mut() {
                    s.edit.home();
                }
                cx.notify();
            }
            "end" => {
                if let Some(s) = self.search.as_mut() {
                    s.edit.end();
                }
                cx.notify();
            }
            "backspace" | "delete" => {
                if let Some(s) = self.search.as_mut() {
                    if ks.key == "backspace" {
                        s.edit.backspace();
                    } else {
                        s.edit.delete();
                    }
                    s.current = 0;
                }
                self.search_jump(cx);
            }
            _ => {
                let text = ks
                    .key_char
                    .as_deref()
                    .filter(|t| !t.is_empty() && !mods.ctrl && !mods.alt);
                if let Some(text) = text {
                    if let Some(s) = self.search.as_mut() {
                        s.edit.insert(text);
                        s.current = 0;
                    }
                    self.search_jump(cx);
                }
            }
        }
    }

    /// The floating search overlay (bottom-right), with a caret in the query.
    fn search_bar(&self, before: &str, after: &str, pos: usize, total: usize) -> impl IntoElement {
        let mut caret = colors::hsla(self.colors.cursor);
        caret.a = 0.9;
        div()
            .absolute()
            .bottom(px(8.0))
            .right(px(8.0))
            .px_2()
            .py_1()
            .flex()
            .items_center()
            .bg(colors::rgba(self.colors.selection_bg))
            .text_color(colors::rgba(self.colors.selection_fg))
            .text_size(self.font_size)
            .child(SharedString::from("\u{2315} "))
            .child(SharedString::from(before.to_string()))
            .child(div().w(px(1.0)).h(px(14.0)).bg(caret))
            .child(SharedString::from(after.to_string()))
            .child(SharedString::from(format!("    {pos}/{total}")))
    }

    fn lines(&self) -> Vec<assist::Line> {
        self.session.with_term(|term| {
            term.text_lines()
                .into_iter()
                .map(|(number, text, prompt)| assist::Line {
                    number,
                    text,
                    prompt,
                })
                .collect()
        })
    }

    fn semantic_hits(&self) -> Vec<assist::context::Hit> {
        match &self.assist {
            Some(Assist::Semantic { edit, .. }) => assist::search(&edit.text(), &self.lines(), 6),
            _ => Vec::new(),
        }
    }

    pub fn toggle_semantic_search(&mut self, cx: &mut Context<Self>) {
        self.search = None;
        self.assist = match self.assist {
            Some(Assist::Semantic { .. }) => None,
            _ => Some(Assist::Semantic {
                edit: crate::textedit::TextEdit::new(""),
                current: 0,
            }),
        };
        cx.notify();
    }

    pub fn explain_output(&mut self, cx: &mut Context<Self>) {
        let selected = self.session.with_term(|term| term.selection_text());
        let text = selected
            .filter(|text| !text.trim().is_empty())
            .or_else(|| assist::lastblock(&self.lines()).map(|block| block.text))
            .unwrap_or_default();
        self.search = None;
        self.assist = Some(Assist::Message {
            title: "Explain".to_string(),
            body: assist::explain(&text),
        });
        cx.notify();
    }

    pub fn compose_command(&mut self, cx: &mut Context<Self>) {
        self.search = None;
        self.assist = Some(Assist::Compose {
            edit: crate::textedit::TextEdit::new(""),
        });
        cx.notify();
    }

    fn semantic_jump(&mut self, cx: &mut Context<Self>) {
        let hits = self.semantic_hits();
        let Some(Assist::Semantic { current, .. }) = self.assist.as_mut() else {
            return;
        };
        if hits.is_empty() {
            cx.notify();
            return;
        }
        *current = (*current).min(hits.len() - 1);
        let line = hits[*current].block.start;
        self.session.with_term(|term| {
            let sb = term.grid().scrollback().len();
            term.set_display_offset(sb.saturating_sub(line));
        });
        cx.notify();
    }

    fn semantic_step(&mut self, delta: i64, cx: &mut Context<Self>) {
        let len = self.semantic_hits().len() as i64;
        if len == 0 {
            cx.notify();
            return;
        }
        if let Some(Assist::Semantic { current, .. }) = self.assist.as_mut() {
            *current = (((*current as i64 + delta) % len + len) % len) as usize;
        }
        self.semantic_jump(cx);
    }

    fn write_paste(&mut self, text: &str, cx: &mut Context<Self>) {
        let bracketed = self.session.with_term(|term| term.bracketed_paste());
        self.scroll_to_bottom(cx);
        let _ = self.session.write(&input::encode_paste(text, bracketed));
    }

    fn assist_key(&mut self, ks: &gpui::Keystroke, mods: input::Mods, cx: &mut Context<Self>) {
        if mods.cmd || mods.ctrl {
            return;
        }
        let Some(assist) = self.assist.as_mut() else {
            return;
        };
        match assist {
            Assist::Semantic { edit, current } => match ks.key.as_str() {
                "escape" => self.assist = None,
                "enter" => self.semantic_jump(cx),
                "down" => self.semantic_step(1, cx),
                "up" => self.semantic_step(-1, cx),
                "left" => edit.left(),
                "right" => edit.right(),
                "home" => edit.home(),
                "end" => edit.end(),
                "backspace" => {
                    edit.backspace();
                    *current = 0;
                }
                "delete" => {
                    edit.delete();
                    *current = 0;
                }
                _ => {
                    if let Some(text) = ks
                        .key_char
                        .as_deref()
                        .filter(|t| !t.is_empty() && !mods.alt)
                    {
                        edit.insert(text);
                        *current = 0;
                    }
                }
            },
            Assist::Compose { edit } => match ks.key.as_str() {
                "escape" => self.assist = None,
                "enter" => {
                    let command = assist::compose(&edit.text());
                    if !command.trim().is_empty() {
                        self.scroll_to_bottom(cx);
                        let _ = self.session.write(command.as_bytes());
                    }
                    self.assist = None;
                }
                "left" => edit.left(),
                "right" => edit.right(),
                "home" => edit.home(),
                "end" => edit.end(),
                "backspace" => {
                    edit.backspace();
                }
                "delete" => {
                    edit.delete();
                }
                _ => {
                    if let Some(text) = ks
                        .key_char
                        .as_deref()
                        .filter(|t| !t.is_empty() && !mods.alt)
                    {
                        edit.insert(text);
                    }
                }
            },
            Assist::Message { .. } => {
                if matches!(ks.key.as_str(), "escape" | "enter") {
                    self.assist = None;
                }
            }
            Assist::Paste { text, .. } => match ks.key.as_str() {
                "escape" | "n" => self.assist = None,
                "enter" | "y" => {
                    let text = text.clone();
                    self.assist = None;
                    self.write_paste(&text, cx);
                }
                _ => {}
            },
        }
        cx.notify();
    }

    fn key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        let mods = input::Mods {
            shift: keystroke.modifiers.shift,
            alt: keystroke.modifiers.alt,
            ctrl: keystroke.modifiers.control,
            cmd: keystroke.modifiers.platform,
        };
        // Escape closes an open right-click menu before anything else.
        if self.context_menu.is_some() {
            if keystroke.key == "escape" {
                self.context_menu = None;
                cx.notify();
                cx.stop_propagation();
            }
            return;
        }
        if self.search.is_some() {
            self.search_key(keystroke, mods, cx);
            cx.stop_propagation();
            return;
        }
        if self.assist.is_some() {
            self.assist_key(keystroke, mods, cx);
            cx.stop_propagation();
            return;
        }
        // Read-only panes swallow input without forwarding it to the pty.
        if self.read_only {
            cx.stop_propagation();
            return;
        }
        // Feed an active command-macro recording with the typed key before
        // forwarding it to the pty, so the recorder captures exactly what the
        // user types at the shell.
        if cx.try_global::<crate::root::MacroRecorder>().is_some() {
            cx.update_global::<crate::root::MacroRecorder, _>(|rec, _| {
                rec.0.key(&keystroke.key, keystroke.key_char.as_deref());
            });
        }
        let state = self.session.with_term(|term| input::TermState {
            cursor_keys_app: term.cursor_keys_app(),
            keypad_app: term.keypad_app(),
            bracketed_paste: term.bracketed_paste(),
            kitty_flags: term.kitty_keyboard_flags(),
        });
        // Resolve how macOS Option should behave for this keystroke. gpui sets
        // `alt` for the Option key; whether that counts as Alt/Meta is the
        // `macos-option-as-alt` policy. gpui can't tell left from right Option,
        // so any non-`false` setting treats Option as Alt.
        let (mods, text) = self.resolve_option(keystroke, mods);
        if let Some(bytes) = input::encode_key(&keystroke.key, text, mods, state) {
            self.scroll_to_bottom(cx);
            let _ = self.session.write(&bytes);
            // Broadcast mode: hand the encoded bytes to the workspace, which
            // mirrors them to every other pane in the active tab.
            if cx
                .try_global::<crate::root::Broadcast>()
                .is_some_and(|b| b.0)
            {
                cx.emit(ViewEvent::Input(bytes));
            }
            cx.stop_propagation();
        }
    }

    /// Apply the `macos-option-as-alt` policy, returning the effective
    /// modifiers and the text source for `encode_key`. On non-macOS, or when
    /// Option isn't held, it passes the platform's composed text straight
    /// through.
    fn resolve_option<'k>(
        &self,
        keystroke: &'k gpui::Keystroke,
        mods: input::Mods,
    ) -> (input::Mods, Option<&'k str>) {
        let option_held = keystroke.modifiers.alt && !keystroke.modifiers.platform;
        option_policy(
            self.option_as_alt,
            cfg!(target_os = "macos"),
            option_held,
            &keystroke.key,
            keystroke.key_char.as_deref(),
            mods,
        )
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

    /// Whether this pane is recording an asciinema cast.
    pub fn is_recording(&self) -> bool {
        self.session.is_recording()
    }

    /// Start or stop recording this pane to an asciinema `.cast` file. On stop,
    /// surfaces the saved path in a dismissable message.
    pub fn toggle_recording(&mut self, cx: &mut Context<Self>) {
        if self.session.is_recording() {
            let body = match self.session.stop_recording() {
                Some(path) => path.display().to_string(),
                None => "(no file written)".to_string(),
            };
            self.assist = Some(Assist::Message {
                title: "Recording saved".to_string(),
                body,
            });
        } else if let Some((path, ts)) = recording_target() {
            let title = self.title().to_string();
            if self.session.start_recording(path, Some(&title), Some(ts)).is_err() {
                self.assist = Some(Assist::Message {
                    title: "Recording failed".to_string(),
                    body: "could not create the cast file".to_string(),
                });
            }
        }
        cx.notify();
    }

    /// Right mouse button: open the context menu, unless the app is capturing
    /// the mouse (then the press is reported to the child instead).
    fn right_down(&mut self, e: &MouseDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let reporting = self
            .session
            .with_term(|t| crate::mouse::reports(t.mouse_mode(), e.modifiers.shift));
        if reporting {
            return;
        }
        self.context_menu = Some(e.position);
        cx.stop_propagation();
        cx.notify();
    }

    /// The right-click menu overlay anchored at `pos`, with a full-pane
    /// backdrop that dismisses it on an outside click.
    fn context_menu_overlay(&self, pos: Point<Pixels>, cx: &mut Context<Self>) -> AnyElement {
        // (label, action); `None` is a separator.
        let items: [Option<(&str, Action)>; 8] = [
            Some(("Copy", Action::Copy)),
            Some(("Paste", Action::Paste)),
            Some(("Select All", Action::SelectAll)),
            None,
            Some(("Split Right", Action::NewSplit(SplitDirection::Right))),
            Some(("Split Down", Action::NewSplit(SplitDirection::Down))),
            None,
            Some(("Clear", Action::ClearScreen)),
        ];
        let mut menu = div()
            .flex()
            .flex_col()
            .min_w(px(180.0))
            .p_1()
            .rounded(px(8.0))
            .border_1()
            .border_color(colors::rgba(self.colors.selection_bg))
            .bg(colors::rgba(self.colors.bg))
            .text_color(colors::rgba(self.colors.fg))
            .shadow_lg();
        for item in items {
            match item {
                None => {
                    let mut line = colors::rgba(self.colors.fg);
                    line.a = 0.15;
                    menu = menu.child(div().my_1().mx_2().h(px(1.0)).bg(line));
                }
                Some((label, action)) => {
                    let action = action.clone();
                    menu = menu.child(
                        div()
                            .px_2()
                            .py(px(4.0))
                            .rounded(px(5.0))
                            .hover(|s| s.bg(colors::rgba(self.colors.selection_bg)))
                            .child(SharedString::from(label))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _e: &MouseDownEvent, _w, cx| {
                                    this.context_menu = None;
                                    cx.emit(ViewEvent::Action(action.clone()));
                                    cx.stop_propagation();
                                    cx.notify();
                                }),
                            ),
                    );
                }
            }
        }
        let dismiss = |this: &mut Self, _e: &MouseDownEvent, _w: &mut Window, cx: &mut Context<Self>| {
            this.context_menu = None;
            cx.stop_propagation();
            cx.notify();
        };
        deferred(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .on_mouse_down(MouseButton::Left, cx.listener(dismiss))
                .on_mouse_down(MouseButton::Right, cx.listener(dismiss))
                .child(
                    anchored()
                        .position(pos)
                        .snap_to_window_with_margin(px(6.0))
                        .child(menu),
                ),
        )
        .into_any_element()
    }

    /// Copy the current selection to the clipboard, if any.
    pub fn copy_selection(&mut self, cx: &mut Context<Self>) {
        let Some(text) = self.session.with_term(|term| term.selection_text()) else {
            return;
        };
        if !text.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    /// Paste the clipboard into the pty (bracketed when the app requested it).
    pub fn paste_clipboard(&mut self, cx: &mut Context<Self>) {
        if self.read_only {
            return;
        }
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        if text.is_empty() {
            return;
        }
        if self.paste_protection {
            let risk = assist::analyze(&text);
            if risk.risky() {
                self.search = None;
                self.assist = Some(Assist::Paste { text, risk });
                cx.notify();
                return;
            }
        }
        self.write_paste(&text, cx);
    }

    /// Write raw bytes to the pty, snapping the view to the live bottom.
    /// Backs the macOS readline navigation keybinds (`text:`/`esc:`).
    pub fn send_text(&mut self, bytes: &[u8], cx: &mut Context<Self>) {
        if self.read_only || bytes.is_empty() {
            return;
        }
        self.scroll_to_bottom(cx);
        let _ = self.session.write(bytes);
    }

    /// Select the entire buffer — every scrollback row plus the live screen —
    /// so it can be copied. Mirrors a top-left to bottom-right cell drag.
    pub fn select_all(&mut self, cx: &mut Context<Self>) {
        self.session.with_term(|term| {
            let grid = term.grid();
            let (rows, cols) = (grid.rows(), grid.cols());
            if rows == 0 || cols == 0 {
                return;
            }
            let top = -(grid.scrollback().len() as isize);
            let bottom = rows as isize - 1;
            term.start_selection(vt::SelectionMode::Cell, vt::Point::new(top, 0));
            term.update_selection(vt::Point::new(bottom, cols - 1));
        });
        cx.notify();
    }

    /// Run a trusted plugin command in the focused shell.
    pub fn run_command(&mut self, command: &str, cx: &mut Context<Self>) {
        if self.read_only {
            return;
        }
        let command = command.trim();
        if command.is_empty() {
            return;
        }
        self.scroll_to_bottom(cx);
        let mut input = command.as_bytes().to_vec();
        input.push(b'\n');
        let _ = self.session.write(&input);
    }

    /// Replay a macro's commands into this pane, one submitted line each.
    ///
    /// Pacing keys off OSC 133 prompt marks: after sending a command we wait
    /// for the prompt-mark count to grow (the shell printed a fresh prompt,
    /// i.e. the command finished) before sending the next, bounded by
    /// [`REPLAY_TIMEOUT`]. Shells without shell integration emit no marks, so
    /// we fall back to a fixed gap between commands.
    pub fn run_macro(&self, commands: Vec<String>, cx: &mut Context<Self>) {
        if self.read_only || commands.is_empty() {
            return;
        }
        let session = self.session.clone();
        let executor = cx.background_executor().clone();
        // Track replay in an app-global counter and repaint so the workspace
        // shows its indicator. Driven from the (detached) task so it clears
        // even if this pane is closed mid-replay.
        crate::root::replays_changed(cx, 1);
        cx.spawn(async move |view, cx| {
            // Decide pacing once: if the buffer carries any prompt marks the
            // shell speaks OSC 133, so we can wait for command completion.
            let paced = session.with_term(|t| !t.prompt_lines().is_empty());
            for command in commands {
                let before = session.with_term(|t| {
                    t.set_display_offset(0);
                    t.prompt_lines().len()
                });
                let mut bytes = command.into_bytes();
                bytes.push(b'\n');
                if session.write(&bytes).is_err() {
                    break;
                }
                if !paced {
                    executor.timer(REPLAY_FALLBACK_GAP).await;
                    continue;
                }
                let start = Instant::now();
                loop {
                    executor.timer(REPLAY_POLL).await;
                    if session.with_term(|t| t.prompt_lines().len()) > before {
                        break;
                    }
                    if start.elapsed() >= REPLAY_TIMEOUT {
                        break;
                    }
                }
            }
            let _ = view.update(cx, |view, cx| view.scroll_to_bottom(cx));
            cx.update(|cx| crate::root::replays_changed(cx, -1));
        })
        .detach();
    }

    /// Up to `lines` of the most recent screen text (scrollback + live grid),
    /// defaulting to the visible row count. Backs the MCP `read_screen` tool.
    pub fn screen_text(&self, lines: Option<usize>) -> String {
        self.session.with_term(|term| {
            let all = term.text_lines();
            let want = lines.unwrap_or_else(|| term.rows());
            let start = all.len().saturating_sub(want);
            let text = all[start..]
                .iter()
                .map(|(_, line, _)| line.trim_end())
                .collect::<Vec<_>>()
                .join("\n");
            text.trim_end().to_string()
        })
    }

    /// Scroll the viewport by `delta` rows into (positive) or out of
    /// (negative) scrollback history.
    pub fn scroll_lines(&mut self, delta: isize, cx: &mut Context<Self>) {
        let moved = self.session.with_term(|term| {
            let before = term.display_offset();
            term.scroll_display(delta);
            term.display_offset() != before
        });
        if moved {
            cx.notify();
        }
    }

    /// Scroll by whole pages (the pane's row count), sign as in
    /// [`Self::scroll_lines`].
    pub fn scroll_pages(&mut self, pages: isize, cx: &mut Context<Self>) {
        let rows = self.session.with_term(|term| term.rows()) as isize;
        self.scroll_lines(pages * rows.max(1), cx);
    }

    /// Jump to the oldest scrollback line.
    pub fn scroll_to_top(&mut self, cx: &mut Context<Self>) {
        let moved = self.session.with_term(|term| {
            let max = term.grid().scrollback().len();
            let before = term.display_offset();
            term.set_display_offset(max);
            term.display_offset() != before
        });
        if moved {
            cx.notify();
        }
    }

    /// Jump to the live bottom (alias of the input scroll-to-bottom path).
    pub fn scroll_to_live(&mut self, cx: &mut Context<Self>) {
        self.scroll_to_bottom(cx);
    }

    /// Move the viewport by `delta` shell prompts (OSC 133;A marks);
    /// negative scrolls toward older prompts.
    pub fn jump_prompt(&mut self, delta: i32, cx: &mut Context<Self>) {
        if delta == 0 {
            return;
        }
        let moved = self.session.with_term(|term| {
            let prompts = term.prompt_lines();
            if prompts.is_empty() {
                return false;
            }
            let sb = term.grid().scrollback().len();
            let mut top = sb - term.display_offset().min(sb);
            let mut changed = false;
            for _ in 0..delta.unsigned_abs() {
                let next = if delta < 0 {
                    prompts.iter().rev().find(|&&p| p < top).copied()
                } else {
                    prompts.iter().find(|&&p| p > top).copied()
                };
                match next {
                    Some(p) => {
                        top = p;
                        changed = true;
                    }
                    None => break,
                }
            }
            if changed {
                term.set_display_offset(sb.saturating_sub(top));
            }
            changed
        });
        if moved {
            cx.notify();
        }
    }

    /// Clear the visible screen the way most terminals' "clear" does: send
    /// a form feed so the shell redraws its prompt at the top.
    pub fn clear_screen(&mut self, cx: &mut Context<Self>) {
        self.scroll_to_bottom(cx);
        let _ = self.session.write(b"\x0c");
    }

    fn input_line(&self, label: &str, edit: &crate::textedit::TextEdit) -> impl IntoElement {
        let (before, after) = edit.split();
        let mut caret = colors::hsla(self.colors.cursor);
        caret.a = 0.9;
        div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .text_color(colors::rgba(self.colors.selection_fg))
                    .child(label.to_string()),
            )
            .child(SharedString::from(before))
            .child(div().w(px(1.0)).h(px(14.0)).bg(caret))
            .child(SharedString::from(after))
    }

    fn assist_panel(&self) -> Option<AnyElement> {
        let assist = self.assist.as_ref()?;
        let mut border = colors::hsla(self.colors.fg);
        border.a = 0.24;
        let panel = div()
            .absolute()
            .bottom(px(8.0))
            .left(px(8.0))
            .max_w(px(620.0))
            .bg(colors::rgba(self.colors.bg))
            .border_1()
            .border_color(border)
            .p_3()
            .flex()
            .flex_col()
            .gap_2()
            .text_size(self.font_size)
            .text_color(colors::rgba(self.colors.fg));
        let panel = match assist {
            Assist::Semantic { edit, current } => {
                let hits = self.semantic_hits();
                let mut panel = panel.child(self.input_line("Semantic", edit));
                for (i, hit) in hits.iter().enumerate() {
                    let color = if i == *current {
                        colors::rgba(self.colors.selection_fg)
                    } else {
                        colors::rgba(self.colors.fg)
                    };
                    let bg = if i == *current {
                        colors::rgba(self.colors.selection_bg)
                    } else {
                        colors::rgba(self.colors.bg)
                    };
                    let first = hit
                        .block
                        .text
                        .lines()
                        .find(|line| !line.trim().is_empty())
                        .unwrap_or("")
                        .trim()
                        .chars()
                        .take(96)
                        .collect::<String>();
                    panel = panel.child(div().px_1().py_1().bg(bg).text_color(color).child(
                        SharedString::from(format!("{:.0}%  {}", hit.score * 100.0, first)),
                    ));
                }
                panel
            }
            Assist::Compose { edit } => panel.child(self.input_line("Command", edit)),
            Assist::Message { title, body } => panel
                .child(
                    div()
                        .text_color(colors::rgba(self.colors.selection_fg))
                        .child(title.clone()),
                )
                .child(SharedString::from(body.clone())),
            Assist::Paste { risk, .. } => {
                let reasons = if risk.reasons.is_empty() {
                    "unknown risk".to_string()
                } else {
                    risk.reasons.join(", ")
                };
                panel
                    .child(
                        div()
                            .text_color(colors::rgba(self.colors.selection_fg))
                            .child(format!("Paste risk: {:?}", risk.level)),
                    )
                    .child(SharedString::from(reasons))
            }
        };
        Some(panel.into_any_element())
    }
}

/// Post a native desktop notification without blocking the UI (spawns a thread
/// that runs [`notify_command`]). Best-effort: missing tools and errors are
/// ignored — the in-app indicator is the reliable signal.
pub fn post_os_notification(title: &str, body: &str) {
    let (title, body) = (title.to_string(), body.to_string());
    std::thread::spawn(move || notify_command(&title, &body));
}

/// Post a native desktop notification synchronously. macOS uses `osascript
/// display notification`; Linux uses `notify-send`. Used by `prompt notify`,
/// which must wait for the helper before the process exits.
pub fn notify_command(title: &str, body: &str) {
    #[cfg(target_os = "macos")]
    {
        let esc = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
        let script = format!(
            "display notification \"{}\" with title \"{}\"",
            esc(body),
            esc(title)
        );
        let _ = std::process::Command::new("osascript")
            .args(["-e", &script])
            .output();
    }
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("notify-send")
            .args([title, body])
            .output();
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let _ = (title, body);
}

/// Path for a new recording under `~/.config/prompt/recordings/`, plus the
/// unix timestamp for its header. `None` if the directory can't be made.
fn recording_target() -> Option<(std::path::PathBuf, u64)> {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();
    let dir = config::default_path()?.parent()?.join("recordings");
    std::fs::create_dir_all(&dir).ok()?;
    Some((dir.join(format!("prompt-{ts}.cast")), ts))
}

impl EventEmitter<ViewEvent> for TerminalView {}

impl Focusable for TerminalView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for TerminalView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Compute the matches once (cached) and reuse for the highlight map
        // and the result counter, instead of re-scanning the buffer per frame.
        let matches = self.search_matches();
        let query = self.search.as_ref().map(|s| crate::element::SearchQuery {
            query: s.edit.text(),
            current: s.current,
            matches: matches.clone(),
        });
        let bar = self.search.as_ref().map(|s| {
            let total = matches.len();
            let pos = if total == 0 { 0 } else { s.current + 1 };
            let (before, after) = s.edit.split();
            self.search_bar(&before, &after, pos, total)
        });
        let assist = self.assist_panel();
        let menu = self
            .context_menu
            .map(|pos| self.context_menu_overlay(pos, cx));
        div()
            .relative()
            .size_full()
            .bg(colors::rgba(self.colors.bg))
            .key_context("Terminal")
            .track_focus(&self.focus)
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
#[path = "../tests/view.rs"]
mod tests;
