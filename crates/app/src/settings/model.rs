//! Pure data model for the settings window: every editable option, how to
//! read its current value from [`config::Options`], and how to compute the
//! config write each control performs. No UI lives here.

use config::Options;

/// Sidebar sections.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Section {
    General,
    Appearance,
    Terminal,
    Keyboard,
    Macros,
    Plugins,
    Ai,
}

impl Section {
    pub const ALL: [Section; 7] = [
        Section::General,
        Section::Appearance,
        Section::Terminal,
        Section::Keyboard,
        Section::Macros,
        Section::Plugins,
        Section::Ai,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Section::General => "General",
            Section::Appearance => "Appearance",
            Section::Terminal => "Terminal",
            Section::Keyboard => "Keyboard",
            Section::Macros => "Macros",
            Section::Plugins => "Plugins",
            Section::Ai => "AI",
        }
    }

    pub fn subtitle(self) -> &'static str {
        match self {
            Section::General => "Manage Prompt's overall setup and terminal behavior.",
            Section::Appearance => "Tune themes, colors, fonts, cursor, and the color palette.",
            Section::Terminal => "Control grid density, padding, scrollback, and scrolling.",
            Section::Keyboard => "Edit the keymap: change a trigger, remove it, or add one.",
            Section::Macros => "Replay recorded command macros and assign each a shortcut.",
            Section::Plugins => "Manage the directories Prompt loads extensions from.",
            Section::Ai => "Expose this terminal over MCP and run the Relay agent mesh.",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Section::General => "\u{2699}",
            Section::Appearance => "\u{25d0}",
            Section::Terminal => "\u{25a3}",
            Section::Keyboard => "\u{2328}",
            Section::Macros => "\u{25b6}",
            Section::Plugins => "\u{2739}",
            Section::Ai => "\u{2728}",
        }
    }

    pub fn accent(self) -> theme::Rgb {
        match self {
            Section::General => theme::Rgb::new(142, 142, 147),
            Section::Appearance => theme::Rgb::new(88, 86, 214),
            Section::Terminal => theme::Rgb::new(52, 199, 89),
            Section::Keyboard => theme::Rgb::new(10, 132, 255),
            Section::Macros => theme::Rgb::new(255, 45, 85),
            Section::Plugins => theme::Rgb::new(255, 159, 10),
            Section::Ai => theme::Rgb::new(191, 90, 242),
        }
    }
}

/// A boolean option rendered as a switch.
#[derive(Clone, Copy)]
pub enum Bool {
    InheritCwd,
    QuitLast,
    ConfirmClose,
    ConfirmQuit,
    PasteProtection,
    ShellIntegration,
    AutoUpdate,
    AutosuggestGhost,
    AutosuggestPopup,
    AutosuggestTab,
    AutosuggestAi,
    AutosuggestHistory,
    AutosuggestCommands,
    AutosuggestPaths,
    AutosuggestAssist,
    SessionRestore,
    TabTitleShowHost,
    CopyOnSelect,
    SmartSelect,
    MiddleClickPaste,
    FocusFollowsMouse,
    Timestamps,
    CursorBlink,
    BoldIsBright,
    MouseHide,
    AiEnabled,
    OptimizeTokens,
    McpServer,
    RelayEnabled,
    RelayStartOnLaunch,
    ToolClaude,
    ToolCodex,
    ToolOllama,
    ToolGemini,
}

impl Bool {
    pub fn key(self) -> &'static str {
        match self {
            Bool::InheritCwd => "window-inherit-working-directory",
            Bool::QuitLast => "quit-after-last-window-closed",
            Bool::ConfirmClose => "confirm-close-surface",
            Bool::ConfirmQuit => "confirm-quit",
            Bool::PasteProtection => "clipboard-paste-protection",
            Bool::ShellIntegration => "shell-integration",
            Bool::AutoUpdate => "auto-update",
            Bool::AutosuggestGhost => "autosuggest-ghost",
            Bool::AutosuggestPopup => "autosuggest-popup",
            Bool::AutosuggestTab => "autosuggest-tab",
            Bool::AutosuggestAi => "autosuggest-ai",
            Bool::AutosuggestHistory => "autosuggest-history",
            Bool::AutosuggestCommands => "autosuggest-commands",
            Bool::AutosuggestPaths => "autosuggest-paths",
            Bool::AutosuggestAssist => "autosuggest-assist",
            Bool::SessionRestore => "session-restore",
            Bool::TabTitleShowHost => "tab-title-show-host",
            Bool::CopyOnSelect => "copy-on-select",
            Bool::SmartSelect => "smart-select",
            Bool::MiddleClickPaste => "middle-click-paste",
            Bool::FocusFollowsMouse => "focus-follows-mouse",
            Bool::Timestamps => "timestamps",
            Bool::CursorBlink => "cursor-style-blink",
            Bool::BoldIsBright => "bold-is-bright",
            Bool::MouseHide => "mouse-hide-while-typing",
            Bool::AiEnabled => "ai-enabled",
            Bool::OptimizeTokens => "ai-optimize-tokens",
            Bool::McpServer => "mcp-server-enabled",
            Bool::RelayEnabled => "relay-enabled",
            Bool::RelayStartOnLaunch => "relay-start-on-launch",
            Bool::ToolClaude => "agent-claude",
            Bool::ToolCodex => "agent-codex",
            Bool::ToolOllama => "agent-ollama",
            Bool::ToolGemini => "agent-gemini",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Bool::InheritCwd => "Inherit working directory",
            Bool::QuitLast => "Quit after last window",
            Bool::ConfirmClose => "Confirm before closing",
            Bool::ConfirmQuit => "Confirm quit while running",
            Bool::PasteProtection => "Confirm risky paste",
            Bool::ShellIntegration => "Shell integration",
            Bool::AutoUpdate => "Check for updates automatically",
            Bool::AutosuggestGhost => "Inline suggestions (ghost text)",
            Bool::AutosuggestPopup => "Completion popup",
            Bool::AutosuggestTab => "Tab cycles completions",
            Bool::AutosuggestAi => "AI suggestions (needs AI enabled)",
            Bool::AutosuggestHistory => "Suggest from history",
            Bool::AutosuggestCommands => "Suggest common commands",
            Bool::AutosuggestPaths => "Complete file paths",
            Bool::AutosuggestAssist => "Rank with assist model",
            Bool::SessionRestore => "Restore session on launch",
            Bool::TabTitleShowHost => "Show user@host in tab title",
            Bool::CopyOnSelect => "Copy on select",
            Bool::SmartSelect => "Smart selection (URLs, paths, hashes)",
            Bool::MiddleClickPaste => "Middle-click paste",
            Bool::FocusFollowsMouse => "Focus follows mouse",
            Bool::Timestamps => "Line timestamps",
            Bool::CursorBlink => "Cursor blink",
            Bool::BoldIsBright => "Bold is bright",
            Bool::MouseHide => "Hide mouse while typing",
            Bool::AiEnabled => "Enable AI features",
            Bool::OptimizeTokens => "Optimize tokens (compact prompts sent to agents)",
            Bool::McpServer => "MCP server (expose this terminal)",
            Bool::RelayEnabled => "Relay agent mesh",
            Bool::RelayStartOnLaunch => "Start Relay on launch",
            Bool::ToolClaude => "Claude Code",
            Bool::ToolCodex => "Codex",
            Bool::ToolOllama => "Ollama",
            Bool::ToolGemini => "Gemini",
        }
    }

    pub fn get(self, o: &Options) -> bool {
        match self {
            Bool::InheritCwd => o.window_inherit_working_directory,
            Bool::QuitLast => o.quit_after_last_window_closed,
            Bool::ConfirmClose => o.confirm_close_surface,
            Bool::ConfirmQuit => o.confirm_quit,
            Bool::PasteProtection => o.clipboard_paste_protection,
            Bool::ShellIntegration => o.shell_integration,
            Bool::AutoUpdate => o.auto_update,
            Bool::AutosuggestGhost => o.autosuggest_ghost,
            Bool::AutosuggestPopup => o.autosuggest_popup,
            Bool::AutosuggestTab => o.autosuggest_tab,
            Bool::AutosuggestAi => o.autosuggest_ai,
            Bool::AutosuggestHistory => o.autosuggest_history,
            Bool::AutosuggestCommands => o.autosuggest_commands,
            Bool::AutosuggestPaths => o.autosuggest_paths,
            Bool::AutosuggestAssist => o.autosuggest_assist,
            Bool::SessionRestore => o.session_restore,
            Bool::TabTitleShowHost => o.tab_title_show_host,
            Bool::CopyOnSelect => o.copy_on_select,
            Bool::SmartSelect => o.smart_select,
            Bool::MiddleClickPaste => o.middle_click_paste,
            Bool::FocusFollowsMouse => o.focus_follows_mouse,
            Bool::Timestamps => o.timestamps,
            Bool::CursorBlink => o.cursor_style_blink,
            Bool::BoldIsBright => o.bold_is_bright,
            Bool::MouseHide => o.mouse_hide_while_typing,
            Bool::AiEnabled => o.ai_enabled,
            Bool::OptimizeTokens => o.ai_optimize_tokens,
            Bool::McpServer => o.mcp_server_enabled,
            Bool::RelayEnabled => o.relay_enabled,
            Bool::RelayStartOnLaunch => o.relay_start_on_launch,
            Bool::ToolClaude => o.agent_claude,
            Bool::ToolCodex => o.agent_codex,
            Bool::ToolOllama => o.agent_ollama,
            Bool::ToolGemini => o.agent_gemini,
        }
    }
}

/// A numeric option rendered as a stepper.
#[derive(Clone, Copy)]
pub enum Num {
    FontSize,
    CellWidth,
    CellHeight,
    PaddingX,
    PaddingY,
    WindowWidth,
    WindowHeight,
    Scrollback,
    MinContrast,
    ScrollMultiplier,
    SplitOpacity,
    BgOpacity,
}

impl Num {
    pub fn key(self) -> &'static str {
        match self {
            Num::FontSize => "font-size",
            Num::CellWidth => "adjust-cell-width",
            Num::CellHeight => "adjust-cell-height",
            Num::PaddingX => "window-padding-x",
            Num::PaddingY => "window-padding-y",
            Num::WindowWidth => "window-width",
            Num::WindowHeight => "window-height",
            Num::Scrollback => "scrollback-limit",
            Num::MinContrast => "minimum-contrast",
            Num::ScrollMultiplier => "mouse-scroll-multiplier",
            Num::SplitOpacity => "unfocused-split-opacity",
            Num::BgOpacity => "background-opacity",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Num::FontSize => "Font size",
            Num::CellWidth => "Cell width adjust",
            Num::CellHeight => "Cell height adjust",
            Num::PaddingX => "Padding X",
            Num::PaddingY => "Padding Y",
            Num::WindowWidth => "Window width (cells)",
            Num::WindowHeight => "Window height (cells)",
            Num::Scrollback => "Scrollback lines",
            Num::MinContrast => "Minimum contrast",
            Num::ScrollMultiplier => "Scroll multiplier",
            Num::SplitOpacity => "Unfocused split opacity",
            Num::BgOpacity => "Background opacity",
        }
    }

    fn current(self, o: &Options) -> f32 {
        match self {
            Num::FontSize => o.font_size,
            Num::CellWidth => o.adjust_cell_width as f32,
            Num::CellHeight => o.adjust_cell_height as f32,
            Num::PaddingX => o.window_padding_x as f32,
            Num::PaddingY => o.window_padding_y as f32,
            Num::WindowWidth => o.window_width as f32,
            Num::WindowHeight => o.window_height as f32,
            Num::Scrollback => o.scrollback_limit as f32,
            Num::MinContrast => o.minimum_contrast,
            Num::ScrollMultiplier => o.mouse_scroll_multiplier,
            Num::SplitOpacity => o.unfocused_split_opacity,
            Num::BgOpacity => o.background_opacity,
        }
    }

    /// `(step, min, max, integer?)`.
    fn spec(self) -> (f32, f32, f32, bool) {
        match self {
            Num::FontSize => (1.0, 6.0, 72.0, false),
            Num::CellWidth => (1.0, -64.0, 64.0, true),
            Num::CellHeight => (1.0, -64.0, 64.0, true),
            Num::PaddingX => (1.0, 0.0, 200.0, true),
            Num::PaddingY => (1.0, 0.0, 200.0, true),
            Num::WindowWidth => (1.0, 0.0, 500.0, true),
            Num::WindowHeight => (1.0, 0.0, 500.0, true),
            Num::Scrollback => (1000.0, 0.0, 1_000_000.0, true),
            Num::MinContrast => (0.5, 1.0, 21.0, false),
            Num::ScrollMultiplier => (0.5, 0.01, 10_000.0, false),
            Num::SplitOpacity => (0.05, 0.15, 1.0, false),
            Num::BgOpacity => (0.05, 0.2, 1.0, false),
        }
    }

    /// The current value formatted for display.
    pub fn display(self, o: &Options) -> String {
        let v = self.current(o);
        if matches!(self, Num::WindowWidth | Num::WindowHeight) && v == 0.0 {
            return "auto".to_string();
        }
        let (_, _, _, is_int) = self.spec();
        fmt_num(v, is_int)
    }

    /// The value to persist after stepping by `dir` (+1 / -1).
    pub fn write_value(self, o: &Options, dir: i32) -> String {
        let (step, min, max, is_int) = self.spec();
        let next = (self.current(o) + dir as f32 * step).clamp(min, max);
        fmt_num(next, is_int)
    }
}

fn fmt_num(v: f32, is_int: bool) -> String {
    if is_int {
        format!("{}", v.round() as i64)
    } else {
        format!("{v}")
    }
}

/// A multi-choice option rendered as a left/right cycle.
#[derive(Clone, Copy)]
pub enum Choice {
    Theme,
    CursorStyle,
    FontStyle,
    OptionAsAlt,
    ClipboardRead,
    ClipboardWrite,
}

impl Choice {
    pub fn key(self) -> &'static str {
        match self {
            Choice::Theme => "theme",
            Choice::CursorStyle => "cursor-style",
            Choice::FontStyle => "font-style",
            Choice::OptionAsAlt => "macos-option-as-alt",
            Choice::ClipboardRead => "clipboard-read",
            Choice::ClipboardWrite => "clipboard-write",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Choice::Theme => "Theme",
            Choice::CursorStyle => "Cursor style",
            Choice::FontStyle => "Font style",
            Choice::OptionAsAlt => "Option as Alt",
            Choice::ClipboardRead => "Clipboard read",
            Choice::ClipboardWrite => "Clipboard write",
        }
    }

    /// The choice list and the currently selected index.
    fn options(self, o: &Options) -> (Vec<String>, usize) {
        match self {
            Choice::Theme => {
                let names: Vec<String> = theme::names().iter().map(|s| s.to_string()).collect();
                let cur = names
                    .iter()
                    .position(|n| n.eq_ignore_ascii_case(o.theme.trim()))
                    .unwrap_or(0);
                (names, cur)
            }
            Choice::CursorStyle => {
                let cur = match o.cursor_style {
                    config::CursorStyle::Block => 0,
                    config::CursorStyle::Bar => 1,
                    config::CursorStyle::Underline => 2,
                };
                (strs(&["block", "bar", "underline"]), cur)
            }
            Choice::FontStyle => {
                let cur = match o.font_style {
                    config::FontStyle::Normal => 0,
                    config::FontStyle::Bold => 1,
                    config::FontStyle::Italic => 2,
                    config::FontStyle::BoldItalic => 3,
                };
                (strs(&["normal", "bold", "italic", "bold-italic"]), cur)
            }
            Choice::OptionAsAlt => {
                let cur = match o.macos_option_as_alt {
                    config::OptionAsAlt::Auto => 0,
                    config::OptionAsAlt::False => 1,
                    config::OptionAsAlt::True => 2,
                    config::OptionAsAlt::Left => 3,
                    config::OptionAsAlt::Right => 4,
                };
                (strs(&["auto", "false", "true", "left", "right"]), cur)
            }
            Choice::ClipboardRead => (strs(&["allow", "ask", "deny"]), clip(o.clipboard_read)),
            Choice::ClipboardWrite => (strs(&["allow", "ask", "deny"]), clip(o.clipboard_write)),
        }
    }

    /// The current selection formatted for display.
    pub fn display(self, o: &Options) -> String {
        if matches!(self, Choice::Theme) && o.theme.trim().is_empty() {
            return "default".to_string();
        }
        let (values, cur) = self.options(o);
        values[cur].clone()
    }

    /// The value to persist when cycling by `dir` (+1 / -1).
    pub fn write_value(self, o: &Options, dir: i32) -> String {
        let (values, cur) = self.options(o);
        let n = values.len() as i32;
        let idx = (((cur as i32 + dir) % n + n) % n) as usize;
        values[idx].clone()
    }
}

fn clip(a: config::ClipboardAccess) -> usize {
    match a {
        config::ClipboardAccess::Allow => 0,
        config::ClipboardAccess::Ask => 1,
        config::ClipboardAccess::Deny => 2,
    }
}

fn strs(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

/// A free-text option rendered as an editable field.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Shell,
    WorkingDirectory,
    Title,
    Foreground,
    Background,
    CursorColor,
    CursorText,
    SelectionForeground,
    SelectionBackground,
    SplitDivider,
    RelayAddress,
    RelayDefaultAgent,
    ClaudePath,
    CodexPath,
    GeminiPath,
    ClaudeArgs,
    CodexArgs,
    GeminiArgs,
    ThemeLight,
    ThemeDark,
    BgImage,
    Badge,
}

impl Field {
    pub fn key(self) -> &'static str {
        match self {
            Field::Shell => "command",
            Field::WorkingDirectory => "working-directory",
            Field::Title => "title",
            Field::Foreground => "foreground",
            Field::Background => "background",
            Field::CursorColor => "cursor-color",
            Field::CursorText => "cursor-text",
            Field::SelectionForeground => "selection-foreground",
            Field::SelectionBackground => "selection-background",
            Field::SplitDivider => "split-divider-color",
            Field::RelayAddress => "relay-address",
            Field::RelayDefaultAgent => "relay-default-agent",
            Field::ThemeLight => "theme-light",
            Field::ThemeDark => "theme-dark",
            Field::BgImage => "background-image",
            Field::Badge => "badge",
            Field::ClaudePath => "agent-claude-path",
            Field::CodexPath => "agent-codex-path",
            Field::GeminiPath => "agent-gemini-path",
            Field::ClaudeArgs => "agent-claude-args",
            Field::CodexArgs => "agent-codex-args",
            Field::GeminiArgs => "agent-gemini-args",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Field::Shell => "Shell",
            Field::WorkingDirectory => "Working directory",
            Field::Title => "Window title",
            Field::Foreground => "Foreground",
            Field::Background => "Background",
            Field::CursorColor => "Cursor color",
            Field::CursorText => "Cursor text color",
            Field::SelectionForeground => "Selection foreground",
            Field::SelectionBackground => "Selection background",
            Field::SplitDivider => "Split divider color",
            Field::RelayAddress => "Relay address",
            Field::RelayDefaultAgent => "Default agent",
            Field::ThemeLight => "Light-mode theme",
            Field::ThemeDark => "Dark-mode theme",
            Field::BgImage => "Background image",
            Field::Badge => "Badge watermark",
            Field::ClaudePath => "Claude path",
            Field::CodexPath => "Codex path",
            Field::GeminiPath => "Gemini path",
            Field::ClaudeArgs => "Claude flags",
            Field::CodexArgs => "Codex flags",
            Field::GeminiArgs => "Gemini flags",
        }
    }

    pub fn placeholder(self) -> &'static str {
        match self {
            Field::Shell => "Login shell",
            Field::WorkingDirectory => "Inherit",
            Field::Title => "Default",
            Field::RelayAddress => "127.0.0.1:7777",
            Field::RelayDefaultAgent => "claude",
            Field::ThemeLight | Field::ThemeDark => "scheme name (blank = off)",
            Field::BgImage => "/path/to/image.png",
            Field::Badge => "{host}  ·  {cwd}",
            Field::ClaudePath => "Search PATH (e.g. /usr/local/bin/claude)",
            Field::CodexPath => "Search PATH",
            Field::GeminiPath => "Search PATH",
            Field::ClaudeArgs => "--dangerously-skip-permissions",
            Field::CodexArgs => "extra CLI flags",
            Field::GeminiArgs => "extra CLI flags",
            Field::Foreground
            | Field::Background
            | Field::CursorColor
            | Field::CursorText
            | Field::SelectionForeground
            | Field::SelectionBackground
            | Field::SplitDivider => "Theme",
        }
    }

    pub fn value(self, o: &Options) -> String {
        match self {
            Field::RelayAddress => return o.relay_address.clone(),
            Field::RelayDefaultAgent => return o.relay_default_agent.clone(),
            Field::ThemeLight => return o.theme_light.clone(),
            Field::ThemeDark => return o.theme_dark.clone(),
            _ => {}
        }
        let opt = match self {
            Field::Shell => &o.shell,
            Field::WorkingDirectory => &o.working_directory,
            Field::Title => &o.title,
            Field::Foreground => &o.foreground,
            Field::Background => &o.background,
            Field::CursorColor => &o.cursor_color,
            Field::CursorText => &o.cursor_text,
            Field::SelectionForeground => &o.selection_foreground,
            Field::SelectionBackground => &o.selection_background,
            Field::SplitDivider => &o.split_divider_color,
            Field::ClaudePath => &o.agent_claude_path,
            Field::CodexPath => &o.agent_codex_path,
            Field::GeminiPath => &o.agent_gemini_path,
            Field::ClaudeArgs => &o.agent_claude_args,
            Field::CodexArgs => &o.agent_codex_args,
            Field::GeminiArgs => &o.agent_gemini_args,
            Field::BgImage => &o.background_image,
            Field::Badge => &o.badge,
            Field::RelayAddress => return o.relay_address.clone(),
            Field::RelayDefaultAgent => return o.relay_default_agent.clone(),
            Field::ThemeLight => return o.theme_light.clone(),
            Field::ThemeDark => return o.theme_dark.clone(),
        };
        opt.clone().unwrap_or_default()
    }
}

/// A repeated option rendered as an editable list with add/remove.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ListKind {
    FontFamily,
    FontFeature,
    Palette,
    Plugin,
    Keybind,
    AgentTool,
    Redact,
    Trigger,
    Snippet,
    Profile,
}

impl ListKind {
    pub fn label(self) -> &'static str {
        match self {
            ListKind::FontFamily => "Font family",
            ListKind::FontFeature => "Font features",
            ListKind::Palette => "Color palette",
            ListKind::Plugin => "Plugin directories",
            ListKind::Keybind => "Keybindings",
            ListKind::AgentTool => "Custom tools",
            ListKind::Redact => "Redact secrets on copy",
            ListKind::Trigger => "Output triggers (regex | title)",
            ListKind::Snippet => "Snippets (label | command)",
            ListKind::Profile => "Profiles (label | command)",
        }
    }

    pub fn add_label(self) -> &'static str {
        match self {
            ListKind::FontFamily => "Add font",
            ListKind::FontFeature => "Add feature",
            ListKind::Palette => "Add color",
            ListKind::Plugin => "Add plugin",
            ListKind::Keybind => "Add binding",
            ListKind::AgentTool => "Add tool",
            ListKind::Redact => "Add pattern",
            ListKind::Trigger => "Add trigger",
            ListKind::Snippet => "Add snippet",
            ListKind::Profile => "Add profile",
        }
    }

    pub fn placeholder(self) -> &'static str {
        match self {
            ListKind::FontFamily => "Font name",
            ListKind::FontFeature => "-liga or +ss01",
            ListKind::Palette => "0=#1d1f21",
            ListKind::Plugin => "~/.config/prompt/plugins/name",
            ListKind::Keybind => "cmd+shift+t=new_tab",
            ListKind::AgentTool => "mytool|/path/to/bin {prompt} --mcp {mcp}",
            ListKind::Redact => "regex, e.g. sk-[A-Za-z0-9]{20,}",
            ListKind::Trigger => "error|Build failed",
            ListKind::Snippet => "deploy | git push origin main",
            ListKind::Profile => "prod | ssh prod.example.com",
        }
    }

    /// The current entries, as the strings the user edits.
    pub fn values(self, o: &Options) -> Vec<String> {
        match self {
            ListKind::FontFamily => o.font_family.clone(),
            ListKind::FontFeature => o.font_feature.clone(),
            ListKind::Palette => o
                .palette
                .iter()
                .map(|(n, c)| format!("{n}={c}"))
                .collect(),
            ListKind::Plugin => o.plugin.clone(),
            ListKind::Keybind => {
                let (binds, _) = config::resolve(&o.keybind);
                binds.iter().map(|kb| kb.config_line()).collect()
            }
            ListKind::AgentTool => o.agent_custom.clone(),
            ListKind::Redact => o.redact.clone(),
            ListKind::Trigger => o.trigger.clone(),
            ListKind::Snippet => o.snippet.clone(),
            ListKind::Profile => o.profile.clone(),
        }
    }

    /// Translate the edited entries into the config key and the values to
    /// persist for it. Keybinds collapse to the minimal diff against the
    /// defaults; every other list is written verbatim.
    pub fn to_config(self, entries: &[String]) -> (&'static str, Vec<String>) {
        match self {
            ListKind::FontFamily => ("font-family", clean(entries)),
            ListKind::FontFeature => ("font-feature", clean(entries)),
            ListKind::Palette => ("palette", clean(entries)),
            ListKind::Plugin => ("plugin", clean(entries)),
            ListKind::Redact => ("redact", clean(entries)),
            ListKind::Trigger => ("trigger", clean(entries)),
            ListKind::Snippet => ("snippet", clean(entries)),
            ListKind::Profile => ("profile", clean(entries)),
            ListKind::Keybind => {
                let desired: Vec<config::Keybind> = entries
                    .iter()
                    .filter_map(|e| config::parse_keybind(e.trim()).ok())
                    .collect();
                ("keybind", config::diff_from_defaults(&desired))
            }
            ListKind::AgentTool => ("agent-custom", clean(entries)),
        }
    }
}

/// Drop blank entries and trim surrounding whitespace.
fn clean(entries: &[String]) -> Vec<String> {
    entries
        .iter()
        .map(|e| e.trim().to_string())
        .filter(|e| !e.is_empty())
        .collect()
}
