
/// One block of rendered documentation.
pub(crate) enum Block {
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
pub(crate) struct Article {
    pub(crate) title: &'static str,
    pub(crate) icon: &'static str,
    pub(crate) accent: theme::Rgb,
    pub(crate) summary: &'static str,
    pub(crate) blocks: &'static [Block],
}

impl Article {
    /// Whether `query` (already lowercased) appears in the title, summary, or
    /// any block text. Empty query matches everything.
    pub(crate) fn matches(&self, query: &str) -> bool {
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

pub(crate) const ARTICLES: &[Article] = &[
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
