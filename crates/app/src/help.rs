//! Documentation window opened from the Help menu. A searchable sidebar of
//! articles beside a reading pane. Content is static, embedded prose so the
//! help works offline; the sidebar search filters articles by title and body.

use gpui::prelude::*;
use gpui::{
    bounds, div, point, px, size, App, Context, FocusHandle, FontWeight, KeyDownEvent, MouseButton,
    SharedString, TitlebarOptions, Window, WindowBounds, WindowOptions,
};

use crate::colors;
use crate::textedit::TextEdit;

const WIDTH: f32 = 760.0;
const HEIGHT: f32 = 820.0;
const SIDEBAR: f32 = 248.0;

/// One block of rendered documentation.
enum Block {
    /// A section heading within an article.
    Heading(&'static str),
    /// A paragraph of body text.
    Para(&'static str),
    /// A keybinding: human label plus the shortcut shown as a chip.
    Key(&'static str, &'static str),
    /// A bulleted line.
    Bullet(&'static str),
}

/// A documentation article shown in the sidebar and reading pane.
struct Article {
    title: &'static str,
    icon: &'static str,
    accent: theme::Rgb,
    summary: &'static str,
    blocks: &'static [Block],
}

impl Article {
    /// Whether `query` (already lowercased) appears in the title, summary, or
    /// any block text. Empty query matches everything.
    fn matches(&self, query: &str) -> bool {
        if query.is_empty() {
            return true;
        }
        let hit = |s: &str| s.to_lowercase().contains(query);
        if hit(self.title) || hit(self.summary) {
            return true;
        }
        self.blocks.iter().any(|b| match b {
            Block::Heading(t) | Block::Para(t) | Block::Bullet(t) => hit(t),
            Block::Key(l, k) => hit(l) || hit(k),
        })
    }
}

const ARTICLES: &[Article] = &[
    Article {
        title: "Getting Started",
        icon: "\u{2691}",
        accent: theme::Rgb::new(10, 132, 255),
        summary: "Welcome to Prompt — a fast, GPU-rendered terminal.",
        blocks: &[
            Block::Para(
                "Prompt is a terminal emulator built around tabs of split panes, each \
                 running its own shell. Everything you see is drawn directly on the GPU, \
                 so scrolling and redraw stay smooth even under heavy output.",
            ),
            Block::Heading("Opening shells"),
            Block::Para(
                "Use the File and Workspace menus or the shortcuts below to open new \
                 windows, tabs, and splits. Closing the last pane in a tab closes the tab; \
                 closing the last tab closes the window.",
            ),
            Block::Key("New window", "\u{2318} N"),
            Block::Key("New tab", "\u{2318} T"),
            Block::Key("Close pane", "\u{2318} W"),
            Block::Heading("Preferences"),
            Block::Para(
                "Open Settings to change the theme, font, padding, and scrollback. \
                 Settings write back to your config file, which also reloads live when \
                 edited by hand.",
            ),
            Block::Key("Open settings", "\u{2318} ,"),
        ],
    },
    Article {
        title: "Tabs & Windows",
        icon: "\u{25a4}",
        accent: theme::Rgb::new(88, 86, 214),
        summary: "Organize work across tabs and top-level windows.",
        blocks: &[
            Block::Para(
                "Each window holds an ordered row of tabs. The tab bar only appears once \
                 a window has more than one tab. Tabs can be reordered and jumped to \
                 directly by index.",
            ),
            Block::Heading("Moving between tabs"),
            Block::Key("Previous tab", "\u{2318} \u{21e7} ["),
            Block::Key("Next tab", "\u{2318} \u{21e7} ]"),
            Block::Key("Jump to tab 1-9", "\u{2318} 1 \u{2026} \u{2318} 9"),
            Block::Heading("Closing"),
            Block::Key("Close tab", "\u{2318} \u{2325} W"),
            Block::Key("Close window", "\u{2318} \u{21e7} W"),
            Block::Key("Close all windows", "\u{2318} \u{2325} \u{21e7} W"),
        ],
    },
    Article {
        title: "Splits",
        icon: "\u{25eb}",
        accent: theme::Rgb::new(52, 199, 89),
        summary: "Divide a tab into multiple panes you can resize and navigate.",
        blocks: &[
            Block::Para(
                "Split any pane horizontally or vertically to run several shells side by \
                 side within one tab. Drag the divider between panes to resize them. Every \
                 split, resize, and saved-layout command lives in the Workspace menu.",
            ),
            Block::Heading("Creating splits"),
            Block::Key("Split right", "\u{2318} D"),
            Block::Key("Split down", "\u{2318} \u{21e7} D"),
            Block::Heading("Focusing panes"),
            Block::Para("Move focus directionally between the panes of the active tab."),
            Block::Key("Focus up", "\u{2318} \u{2325} \u{2191}"),
            Block::Key("Focus down", "\u{2318} \u{2325} \u{2193}"),
            Block::Key("Focus left", "\u{2318} \u{2325} \u{2190}"),
            Block::Key("Focus right", "\u{2318} \u{2325} \u{2192}"),
        ],
    },
    Article {
        title: "Search & Scrollback",
        icon: "\u{2315}",
        accent: theme::Rgb::new(255, 159, 10),
        summary: "Find text in the buffer and move through prompt history.",
        blocks: &[
            Block::Para(
                "Prompt keeps a scrollback buffer per pane. Search the visible buffer with \
                 a find overlay, or jump between shell prompts to navigate long sessions \
                 by command rather than by line.",
            ),
            Block::Heading("Finding text"),
            Block::Key("Find in buffer", "\u{2318} F"),
            Block::Heading("Prompt navigation"),
            Block::Para(
                "Prompt jumps require shell integration so Prompt knows where each command \
                 began.",
            ),
            Block::Key("Jump to previous prompt", "\u{2318} \u{2191}"),
            Block::Key("Jump to next prompt", "\u{2318} \u{2193}"),
            Block::Key("Clear screen", "\u{2318} K"),
        ],
    },
    Article {
        title: "AI Assist",
        icon: "\u{2728}",
        accent: theme::Rgb::new(191, 90, 242),
        summary: "Explain output, compose commands, and search semantically.",
        blocks: &[
            Block::Para(
                "Prompt's assist features turn the terminal into a conversation. Select \
                 output to explain it, describe a command in plain language to compose it, \
                 or search by meaning across prompt and output blocks.",
            ),
            Block::Heading("Explain output"),
            Block::Para(
                "Explains the current selection, falling back to the most recent command's \
                 output when nothing is selected.",
            ),
            Block::Key("Explain output", "\u{2318} \u{21e7} E"),
            Block::Heading("Compose command"),
            Block::Para(
                "Describe what you want to do; Prompt drafts the shell command and inserts \
                 it at the prompt for you to review before running.",
            ),
            Block::Key("Compose command", "\u{2318} \u{21e7} G"),
            Block::Heading("Semantic find"),
            Block::Key("Semantic find", "\u{2318} \u{21e7} F"),
            Block::Heading("Agent mesh"),
            Block::Para(
                "Enable Relay in Settings \u{2192} AI to run a team of coding agents that \
                 coordinate through Prompt. The AI menu then offers Launch Agent, the live \
                 Feed, Teams, and a Relay submenu to start, stop, restart, and check the \
                 server and view its logs.",
            ),
        ],
    },
    Article {
        title: "Command Line",
        icon: "\u{2305}",
        accent: theme::Rgb::new(255, 214, 10),
        summary: "macOS-style word and line motion at the shell prompt.",
        blocks: &[
            Block::Para(
                "Prompt ships the familiar macOS readline shortcuts so you can move and edit \
                 the current command the way you would in any native text field. These are \
                 macOS defaults; rebind or add your own in the config.",
            ),
            Block::Heading("Move"),
            Block::Key("Start / end of line", "\u{2318} \u{2190} / \u{2318} \u{2192}"),
            Block::Key("Word left / right", "\u{2325} \u{2190} / \u{2325} \u{2192}"),
            Block::Heading("Edit"),
            Block::Key("Delete to line start", "\u{2318} \u{232b}"),
            Block::Key("Delete previous word", "\u{2325} \u{232b}"),
            Block::Heading("Selection"),
            Block::Para(
                "Right-click anywhere in a pane for a quick menu: Copy, Paste, Select All, \
                 Split, and Clear.",
            ),
            Block::Key("Select all (scrollback + screen)", "\u{2318} A"),
            Block::Key("Copy", "\u{2318} C"),
            Block::Key("Paste", "\u{2318} V"),
        ],
    },
    Article {
        title: "Configuration",
        icon: "\u{2699}",
        accent: theme::Rgb::new(142, 142, 147),
        summary: "Customize Prompt with a plain-text config file.",
        blocks: &[
            Block::Para(
                "Prompt reads a simple config file: one `key = value` per line. \
                 Edits apply live — saving the file reloads appearance without restarting.",
            ),
            Block::Heading("Common keys"),
            Block::Bullet("theme — a built-in theme name, e.g. catppuccin-mocha"),
            Block::Bullet("font-family / font-size — terminal text rendering"),
            Block::Bullet("window-padding-x / window-padding-y — inner margins"),
            Block::Bullet("scrollback-limit — lines of history kept per pane"),
            Block::Bullet("copy-on-select — copy selections to the clipboard automatically"),
            Block::Bullet(
                "clipboard-paste-protection — confirm before a risky paste (off by default)",
            ),
            Block::Bullet(
                "shell-integration — inject OSC 133/7 hooks for prompt-jump and cwd (on)",
            ),
            Block::Bullet("session-restore — reopen tabs/splits on launch (off by default)"),
            Block::Bullet(
                "tab-title-show-host — keep user@host: in tab titles (off; tabs show just the path)",
            ),
            Block::Bullet("confirm-quit — warn on quit if a process is still running (on)"),
            Block::Heading("Keybindings"),
            Block::Para(
                "Bind keys with `keybind = trigger=action`, for example \
                 `keybind = cmd+shift+d=new_split:down`. Use `unbind` to drop a default.",
            ),
        ],
    },
    Article {
        title: "Keyboard Shortcuts",
        icon: "\u{2328}",
        accent: theme::Rgb::new(255, 69, 58),
        summary: "The full default keymap in one place.",
        blocks: &[
            Block::Heading("File"),
            Block::Key("New window", "\u{2318} N"),
            Block::Key("New tab", "\u{2318} T"),
            Block::Key("Split right", "\u{2318} D"),
            Block::Key("Split down", "\u{2318} \u{21e7} D"),
            Block::Key("Close pane", "\u{2318} W"),
            Block::Key("Close tab", "\u{2318} \u{2325} W"),
            Block::Key("Close window", "\u{2318} \u{21e7} W"),
            Block::Heading("Edit"),
            Block::Key("Copy", "\u{2318} C"),
            Block::Key("Paste", "\u{2318} V"),
            Block::Key("Select all", "\u{2318} A"),
            Block::Key("Start / end of line", "\u{2318} \u{2190} / \u{2318} \u{2192}"),
            Block::Key("Word left / right", "\u{2325} \u{2190} / \u{2325} \u{2192}"),
            Block::Key("Find", "\u{2318} F"),
            Block::Key("Semantic find", "\u{2318} \u{21e7} F"),
            Block::Key("Explain output", "\u{2318} \u{21e7} E"),
            Block::Key("Compose command", "\u{2318} \u{21e7} G"),
            Block::Heading("Workspace"),
            Block::Key("Split right", "\u{2318} D"),
            Block::Key("Split down", "\u{2318} \u{21e7} D"),
            Block::Key("Focus split", "\u{2318} \u{2325} arrows"),
            Block::Key("Broadcast input", "\u{2318} \u{21e7} B"),
            Block::Key("Record session", "\u{2318} \u{21e7} R"),
            Block::Heading("View"),
            Block::Key("Command palette", "\u{2318} \u{21e7} P"),
            Block::Key("Increase font size", "\u{2318} +"),
            Block::Key("Decrease font size", "\u{2318} -"),
            Block::Key("Reset font size", "\u{2318} 0"),
            Block::Key("Clear screen", "\u{2318} K"),
            Block::Key("Previous prompt", "\u{2318} \u{2191}"),
            Block::Key("Next prompt", "\u{2318} \u{2193}"),
            Block::Heading("Window"),
            Block::Key("Previous tab", "\u{2318} \u{21e7} ["),
            Block::Key("Next tab", "\u{2318} \u{21e7} ]"),
            Block::Key("Settings", "\u{2318} ,"),
            Block::Key("Quit", "\u{2318} Q"),
        ],
    },
];

/// Open the documentation window centered over `parent`.
pub fn open(parent: &Window, cx: &mut App) {
    let center = parent.bounds().center();
    let bounds = bounds(
        center - point(px(WIDTH / 2.0), px(HEIGHT / 2.0)),
        size(px(WIDTH), px(HEIGHT)),
    );
    let _ = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            is_resizable: false,
            titlebar: Some(TitlebarOptions {
                title: Some("Documentation".into()),
                appears_transparent: true,
                traffic_light_position: Some(point(px(20.0), px(20.0))),
            }),
            ..Default::default()
        },
        |window, cx| {
            window.set_window_title("Documentation");
            cx.new(HelpView::new)
        },
    );
}

pub struct HelpView {
    query: TextEdit,
    /// Selected article, an index into `ARTICLES`.
    selected: usize,
    focus: FocusHandle,
}

impl HelpView {
    fn new(cx: &mut Context<Self>) -> Self {
        Self {
            query: TextEdit::default(),
            selected: 0,
            focus: cx.focus_handle(),
        }
    }

    /// Article indices matching the current search query, in order.
    fn visible(&self) -> Vec<usize> {
        let q = self.query.text().to_lowercase();
        ARTICLES
            .iter()
            .enumerate()
            .filter(|(_, a)| a.matches(&q))
            .map(|(i, _)| i)
            .collect()
    }

    /// The article to display: the selection if it still matches, else the
    /// first visible one.
    fn current(&self, visible: &[usize]) -> Option<usize> {
        if visible.contains(&self.selected) {
            Some(self.selected)
        } else {
            visible.first().copied()
        }
    }

    fn key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let ks = &event.keystroke;
        if ks.modifiers.platform || ks.modifiers.control {
            return;
        }
        match ks.key.as_str() {
            "down" => self.move_selection(1),
            "up" => self.move_selection(-1),
            "backspace" => {
                self.query.backspace();
            }
            "delete" => {
                self.query.delete();
            }
            "left" => self.query.left(),
            "right" => self.query.right(),
            "home" => self.query.home(),
            "end" => self.query.end(),
            "escape" => {
                self.query = TextEdit::default();
            }
            _ => {
                if let Some(text) = ks
                    .key_char
                    .as_deref()
                    .filter(|t| !t.is_empty() && !ks.modifiers.alt)
                {
                    self.query.insert(text);
                }
            }
        }
        cx.notify();
        cx.stop_propagation();
    }

    /// Step the selection through the currently visible articles.
    fn move_selection(&mut self, delta: i32) {
        let visible = self.visible();
        if visible.is_empty() {
            return;
        }
        let pos = visible
            .iter()
            .position(|&i| i == self.selected)
            .unwrap_or(0) as i32;
        let n = visible.len() as i32;
        let next = ((pos + delta) % n + n) % n;
        self.selected = visible[next as usize];
    }

    fn icon(&self, glyph: &str, color: theme::Rgb, sz: gpui::Pixels) -> impl IntoElement {
        div()
            .w(sz)
            .h(sz)
            .rounded(px(5.0))
            .bg(hsla(color))
            .text_color(hsla(TEXT))
            .flex()
            .items_center()
            .justify_center()
            .child(SharedString::from(glyph.to_string()))
    }

    fn search(&self) -> impl IntoElement {
        let (before, after) = self.query.split();
        let empty = self.query.text().is_empty();
        let mut placeholder = hsla(TEXT);
        placeholder.a = 0.64;
        let mut row = div()
            .h(px(28.0))
            .mb_3()
            .rounded(px(14.0))
            .px_3()
            .flex()
            .items_center()
            .gap_2()
            .bg(hsla(SEARCH_BG))
            .text_color(if empty { placeholder } else { hsla(TEXT) })
            .child(SharedString::from("\u{2315}"));
        if empty {
            row = row.child(SharedString::from("Search help"));
        } else {
            row = row
                .child(SharedString::from(before))
                .child(div().w(px(1.0)).h(px(15.0)).bg(hsla(TEXT)))
                .child(SharedString::from(after));
        }
        row
    }

    fn sidebar_item(&self, index: usize, selected: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let article = &ARTICLES[index];
        let mut bg = hsla(if selected { BLUE } else { SIDEBAR_BG });
        bg.a = if selected { 1.0 } else { 0.0 };
        div()
            .flex()
            .items_center()
            .gap_2()
            .h(px(32.0))
            .px_2()
            .rounded(px(7.0))
            .bg(bg)
            .text_color(hsla(TEXT))
            .child(self.icon(article.icon, article.accent, px(20.0)))
            .child(SharedString::from(article.title))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _ev, _window, cx| {
                    this.selected = index;
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
    }

    fn sidebar(&self, visible: &[usize], current: Option<usize>, cx: &mut Context<Self>) -> impl IntoElement {
        let mut col = div()
            .w(px(SIDEBAR))
            .h_full()
            .px_3()
            .pt(px(58.0))
            .pb_3()
            .bg(hsla(SIDEBAR_BG))
            .child(self.search());
        if visible.is_empty() {
            col = col.child(
                div()
                    .px_2()
                    .py_2()
                    .text_color(hsla(MUTED))
                    .child(SharedString::from("No matching topics")),
            );
        }
        for &index in visible {
            col = col.child(self.sidebar_item(index, current == Some(index), cx));
        }
        col
    }

    /// Left-aligned article header: icon, title, summary, and a hairline rule.
    fn header(&self, article: &Article) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .pb_4()
            .mb_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(self.icon(article.icon, article.accent, px(34.0)))
                    .child(
                        div()
                            .text_size(px(24.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(hsla(TEXT))
                            .child(SharedString::from(article.title)),
                    ),
            )
            .child(
                div()
                    .pt_2()
                    .text_color(hsla(MUTED))
                    .line_height(px(21.0))
                    .child(SharedString::from(article.summary)),
            )
            .child(div().mt_4().h(px(1.0)).bg(hsla(LINE)))
    }

    fn heading(&self, text: &str) -> gpui::AnyElement {
        div()
            .pt_5()
            .pb_2()
            .text_size(px(12.0))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(hsla(MUTED))
            .child(SharedString::from(text.to_uppercase()))
            .into_any_element()
    }

    fn paragraph(&self, text: &str) -> gpui::AnyElement {
        div()
            .pb_2()
            .text_color(hsla(BODY))
            .line_height(px(22.0))
            .child(SharedString::from(text.to_string()))
            .into_any_element()
    }

    fn bullet(&self, text: &str) -> gpui::AnyElement {
        div()
            .flex()
            .gap_2()
            .pb_1()
            .text_color(hsla(BODY))
            .line_height(px(22.0))
            .child(div().w(px(12.0)).flex_none().child(SharedString::from("\u{2022}")))
            .child(div().flex_1().child(SharedString::from(text.to_string())))
            .into_any_element()
    }

    /// A run of keybindings rendered as one rounded panel with separators,
    /// matching the Settings list style.
    fn keylist(&self, keys: &[(&str, &str)]) -> gpui::AnyElement {
        let mut panel = div().mt_1().flex().flex_col().rounded(px(10.0)).bg(hsla(PANEL));
        for (i, (label, shortcut)) in keys.iter().enumerate() {
            if i > 0 {
                panel = panel.child(div().mx_3().h(px(1.0)).bg(hsla(LINE)));
            }
            panel = panel.child(
                div()
                    .h(px(44.0))
                    .px_3()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().text_color(hsla(TEXT)).child(SharedString::from(*label)))
                    .child(
                        div()
                            .px_2()
                            .py(px(3.0))
                            .rounded(px(6.0))
                            .border_1()
                            .border_color(hsla(FIELD_BORDER))
                            .bg(hsla(FIELD_BG))
                            .text_size(px(13.0))
                            .text_color(hsla(TEXT))
                            .child(SharedString::from(*shortcut)),
                    ),
            );
        }
        panel.into_any_element()
    }

    fn content(&self, article: &Article) -> impl IntoElement {
        let mut body = div().flex().flex_col().min_w(px(0.0));
        let blocks = article.blocks;
        let mut i = 0;
        while i < blocks.len() {
            match &blocks[i] {
                // Batch consecutive keybindings into a single grouped panel.
                Block::Key(..) => {
                    let mut keys = Vec::new();
                    while let Some(Block::Key(label, shortcut)) = blocks.get(i) {
                        keys.push((*label, *shortcut));
                        i += 1;
                    }
                    body = body.child(self.keylist(&keys));
                }
                Block::Heading(text) => {
                    body = body.child(self.heading(text));
                    i += 1;
                }
                Block::Para(text) => {
                    body = body.child(self.paragraph(text));
                    i += 1;
                }
                Block::Bullet(text) => {
                    body = body.child(self.bullet(text));
                    i += 1;
                }
            }
        }
        div()
            .id("help-content")
            .flex_1()
            // Without min-width:0 the widest line sets a min-content width and
            // the pane overflows the window (clipping the right edge) instead
            // of wrapping. Same trick as the workspace root's min_h(0).
            .min_w(px(0.0))
            .h_full()
            .px(px(40.0))
            .pt(px(56.0))
            .pb(px(40.0))
            .bg(hsla(CONTENT_BG))
            .overflow_y_scroll()
            .child(self.header(article))
            .child(body)
    }
}

impl Render for HelpView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let visible = self.visible();
        let current = self.current(&visible);
        let mut root = div()
            .size_full()
            .flex()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key_down))
            .text_color(hsla(TEXT))
            .bg(hsla(CONTENT_BG))
            .child(self.sidebar(&visible, current, cx));
        if let Some(index) = current {
            root = root.child(self.content(&ARTICLES[index]));
        } else {
            root = root.child(
                div()
                    .flex_1()
                    .h_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(hsla(CONTENT_BG))
                    .text_color(hsla(MUTED))
                    .child(SharedString::from("No matching topics")),
            );
        }
        root
    }
}

fn hsla(rgb: theme::Rgb) -> gpui::Hsla {
    colors::hsla(rgb)
}

const SIDEBAR_BG: theme::Rgb = theme::Rgb::new(30, 35, 38);
const CONTENT_BG: theme::Rgb = theme::Rgb::new(35, 42, 44);
const PANEL: theme::Rgb = theme::Rgb::new(43, 52, 54);
const SEARCH_BG: theme::Rgb = theme::Rgb::new(57, 62, 65);
const FIELD_BG: theme::Rgb = theme::Rgb::new(49, 56, 58);
const FIELD_BORDER: theme::Rgb = theme::Rgb::new(76, 84, 88);
const LINE: theme::Rgb = theme::Rgb::new(61, 70, 73);
const TEXT: theme::Rgb = theme::Rgb::new(242, 244, 246);
const BODY: theme::Rgb = theme::Rgb::new(206, 212, 217);
const MUTED: theme::Rgb = theme::Rgb::new(170, 177, 181);
const BLUE: theme::Rgb = theme::Rgb::new(10, 102, 220);
