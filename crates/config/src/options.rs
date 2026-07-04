//! The resolved configuration options and their defaults.

/// Cursor shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CursorStyle {
    #[default]
    Block,
    Bar,
    Underline,
}

impl CursorStyle {
    /// Parse from the config file value (case-insensitive).
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "block" => Some(Self::Block),
            "bar" => Some(Self::Bar),
            "underline" => Some(Self::Underline),
            _ => None,
        }
    }
}

/// Base font style, file key `font-style`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FontStyle {
    #[default]
    Normal,
    Bold,
    Italic,
    BoldItalic,
}

impl FontStyle {
    /// Parse from the config file value (case-insensitive).
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "normal" => Some(Self::Normal),
            "bold" => Some(Self::Bold),
            "italic" => Some(Self::Italic),
            "bold-italic" => Some(Self::BoldItalic),
            _ => None,
        }
    }
}

/// How the macOS option key behaves, file key `macos-option-as-alt`.
///
/// `Auto` (the default) picks `True` or `False` per the active keyboard layout:
/// on US / US-International / ABC layouts Option acts as Alt (so `option+b`
/// sends `ESC b`), while layouts that rely on Option for everyday characters
/// keep it as a character composer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OptionAsAlt {
    #[default]
    Auto,
    False,
    True,
    Left,
    Right,
}

impl OptionAsAlt {
    /// Parse from the config file value: `auto`, booleans, plus left/right.
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "auto" => Some(Self::Auto),
            "left" => Some(Self::Left),
            "right" => Some(Self::Right),
            other => match crate::value::parse_bool(other)? {
                true => Some(Self::True),
                false => Some(Self::False),
            },
        }
    }
}

/// Clipboard access policy, file keys `clipboard-read` / `clipboard-write`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardAccess {
    Allow,
    Ask,
    Deny,
}

impl ClipboardAccess {
    /// Parse from the config file value (case-insensitive).
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "allow" => Some(Self::Allow),
            "ask" => Some(Self::Ask),
            "deny" => Some(Self::Deny),
            _ => None,
        }
    }
}

/// All configuration options with their defaults.
#[derive(Debug, Clone, PartialEq)]
pub struct Options {
    /// File key: `font-family`, repeated to build a fallback chain (first
    /// is primary). Empty means the built-in default.
    pub font_family: Vec<String>,
    /// File key: `font-size`.
    pub font_size: f32,
    /// File key: `font-style` (style of the base font).
    pub font_style: FontStyle,
    /// File key: `font-feature`, repeated entries like `-liga` (accumulated).
    pub font_feature: Vec<String>,
    /// File key: `adjust-cell-width` (integer pixels, may be negative).
    pub adjust_cell_width: i32,
    /// File key: `adjust-cell-height` (integer pixels, may be negative).
    pub adjust_cell_height: i32,
    /// File key: `theme`.
    pub theme: String,
    /// File key: `background` (hex color string).
    pub background: Option<String>,
    /// File key: `foreground` (hex color string).
    pub foreground: Option<String>,
    /// File key: `cursor-style`.
    pub cursor_style: CursorStyle,
    /// File key: `cursor-style-blink`.
    pub cursor_style_blink: bool,
    /// File key: `cursor-color` (hex color string).
    pub cursor_color: Option<String>,
    /// File key: `cursor-text` (hex color string).
    pub cursor_text: Option<String>,
    /// File key: `selection-foreground` (hex color string).
    pub selection_foreground: Option<String>,
    /// File key: `selection-background` (hex color string).
    pub selection_background: Option<String>,
    /// File key: `bold-is-bright`.
    pub bold_is_bright: bool,
    /// File key: `minimum-contrast` (clamped to 1..=21).
    pub minimum_contrast: f32,
    /// File key: `unfocused-split-opacity` (clamped to 0.15..=1).
    pub unfocused_split_opacity: f32,
    /// File key: `split-divider-color` (hex color string).
    pub split_divider_color: Option<String>,
    /// File key: `mouse-scroll-multiplier` (clamped to 0.01..=10000).
    pub mouse_scroll_multiplier: f32,
    /// File key: `macos-option-as-alt`.
    pub macos_option_as_alt: OptionAsAlt,
    /// File key: `window-inherit-working-directory`.
    pub window_inherit_working_directory: bool,
    /// File key: `quit-after-last-window-closed`.
    pub quit_after_last_window_closed: bool,
    /// File key: `title` (window title override).
    pub title: Option<String>,
    /// File key: `clipboard-read`.
    pub clipboard_read: ClipboardAccess,
    /// File key: `clipboard-write`.
    pub clipboard_write: ClipboardAccess,
    /// File key: `scrollback-limit`.
    pub scrollback_limit: usize,
    /// File key: `window-padding-x`.
    pub window_padding_x: u32,
    /// File key: `window-padding-y`.
    pub window_padding_y: u32,
    /// File key: `window-width` (cells, 0 = unset).
    pub window_width: u32,
    /// File key: `window-height` (cells, 0 = unset).
    pub window_height: u32,
    /// File key: `command`.
    pub shell: Option<String>,
    /// File key: `working-directory`.
    pub working_directory: Option<String>,
    /// File key: `copy-on-select`.
    pub copy_on_select: bool,
    /// File key: `confirm-close-surface`.
    pub confirm_close_surface: bool,
    /// File key: `confirm-quit`. Warn before quitting while a process is still
    /// running in one of the panes. On by default.
    pub confirm_quit: bool,
    /// File key: `clipboard-paste-protection`. When set, a paste flagged as
    /// risky prompts for confirmation before reaching the shell. Off by
    /// default so pasting is friction-free.
    pub clipboard_paste_protection: bool,
    /// File key: `mouse-hide-while-typing`.
    pub mouse_hide_while_typing: bool,
    /// File key: `session-restore`. Save the window's tabs/splits/cwds on quit
    /// and restore them on the next launch. Off by default.
    pub session_restore: bool,
    /// File key: `shell-integration`. Inject OSC 133 prompt marks + OSC 7 cwd
    /// reporting into the spawned shell (zsh/bash/fish) so jump-to-prompt and
    /// cwd inheritance work without editing your shell rc. On by default.
    pub shell_integration: bool,
    /// File key: `auto-update` - check GitHub for a newer release on launch and
    /// hourly, and offer to update. On by default.
    pub auto_update: bool,
    /// File key: `autosuggest-ghost` - fish-style inline ghost text completing
    /// the current command from history/commands/paths. On by default.
    pub autosuggest_ghost: bool,
    /// File key: `autosuggest-popup` - a dropdown of ranked completions under
    /// the cursor (arrow + Enter to accept). Off by default.
    pub autosuggest_popup: bool,
    /// File key: `autosuggest-tab` - press Tab to cycle inline completions
    /// instead of forwarding Tab to the shell. Off by default.
    pub autosuggest_tab: bool,
    /// File key: `autosuggest-ai` - when nothing local matches, ask Claude for a
    /// next-command suggestion (needs AI enabled). Off by default.
    pub autosuggest_ai: bool,
    /// File key: `autosuggest-history` - use captured command history as a
    /// suggestion source. On by default.
    pub autosuggest_history: bool,
    /// File key: `autosuggest-commands` - use the built-in common-command list
    /// as a suggestion source. On by default.
    pub autosuggest_commands: bool,
    /// File key: `autosuggest-paths` - complete filesystem paths from the cwd.
    /// On by default.
    pub autosuggest_paths: bool,
    /// File key: `autosuggest-assist` - rank suggestions with the local assist
    /// model rather than plain recency. On by default.
    pub autosuggest_assist: bool,
    /// File key: `tab-title-show-host`. Keep the leading `user@host:` that
    /// shells set in the terminal title when showing it in a tab. Off by
    /// default, so tabs show just the path (the host wraps to two lines and
    /// reads poorly in a narrow tab).
    pub tab_title_show_host: bool,
    /// File key: `palette`, repeated `N=#rrggbb` entries (accumulated).
    pub palette: Vec<(u8, String)>,
    /// File key: `plugin`, repeated plugin directories or manifest paths.
    pub plugin: Vec<String>,
    /// File key: `container`, repeated `label | image | command | lifecycle`
    /// entries (raw strings, parsed by the `container` crate). Each adds or
    /// overrides an OS profile offered by the "New OS Tab" picker.
    pub container: Vec<String>,
    /// File key: `container-engine` - `docker`, `podman`, or `auto` (the
    /// default, `None`, which detects whichever is installed).
    pub container_engine: Option<String>,
    /// File key: `container-persist` - default lifecycle for container tabs.
    /// When false (default) a fresh container is removed on tab close; when
    /// true it is kept. Per-profile `persist`/`ephemeral` overrides this.
    pub container_persist: bool,
    /// File key: `keybind`, raw strings (accumulated, parsed later).
    pub keybind: Vec<String>,
    /// File key: `ai-enabled` - master switch for all AI features.
    pub ai_enabled: bool,
    /// File key: `ai-optimize-tokens` - when true, prompts sent to AI agents are
    /// compacted (trailing whitespace, repeated spaces, and blank-line runs
    /// collapsed) to spend fewer tokens without dropping content.
    pub ai_optimize_tokens: bool,
    /// File key: `mcp-server-enabled` - expose this terminal to agents over MCP.
    pub mcp_server_enabled: bool,
    /// File key: `relay-enabled` - run the Relay agent mesh.
    pub relay_enabled: bool,
    /// File key: `relay-start-on-launch` - start Relay when Prompt launches.
    pub relay_start_on_launch: bool,
    /// File key: `relay-address` - bind address for the Relay server.
    pub relay_address: String,
    /// File key: `relay-default-agent` - default agent CLI for launches.
    pub relay_default_agent: String,
    /// File key: `agent-claude` - Claude Code available as an agent.
    pub agent_claude: bool,
    /// File key: `agent-codex` - Codex available as an agent.
    pub agent_codex: bool,
    /// File key: `agent-ollama` - Ollama available as a (bridged) agent.
    pub agent_ollama: bool,
    /// File key: `agent-gemini` - Gemini available as an agent.
    pub agent_gemini: bool,
    /// File key: `agent-claude-path` - explicit path to the `claude` binary
    /// (overrides PATH lookup). Empty means search PATH.
    pub agent_claude_path: Option<String>,
    /// File key: `agent-codex-path` - explicit path to the `codex` binary.
    pub agent_codex_path: Option<String>,
    /// File key: `agent-gemini-path` - explicit path to the `gemini` binary.
    pub agent_gemini_path: Option<String>,
    /// File key: `agent-claude-args` - extra CLI flags appended when launching
    /// Claude Code (e.g. `--dangerously-skip-permissions`). Blank = none.
    pub agent_claude_args: Option<String>,
    /// File key: `agent-codex-args` - extra CLI flags appended when launching Codex.
    pub agent_codex_args: Option<String>,
    /// File key: `agent-gemini-args` - extra CLI flags appended when launching Gemini.
    pub agent_gemini_args: Option<String>,
    /// File key: `agent-custom` (repeatable) - user-defined agent tools, each
    /// `label|command template`, where the template may use `{prompt}`, `{mcp}`,
    /// `{url}`, and `{name}` placeholders.
    pub agent_custom: Vec<String>,
    /// File key: `theme-light` - scheme used when the OS is in light mode. Set
    /// together with `theme-dark` to follow the system appearance automatically.
    pub theme_light: String,
    /// File key: `theme-dark` - scheme used when the OS is in dark mode.
    pub theme_dark: String,
    /// File key: `timestamps` - show a per-line timestamp gutter in the terminal.
    pub timestamps: bool,
    /// File key: `smart-select` - double-click selects a whole URL / path /
    /// email / hash rather than a single whitespace word. On by default.
    pub smart_select: bool,
    /// File key: `redact` (repeatable) - regexes whose matches are masked (with
    /// `•`) when copying, so secrets don't reach the clipboard.
    pub redact: Vec<String>,
    /// File key: `background-opacity` - window background alpha (0.2..=1.0).
    pub background_opacity: f32,
    /// File key: `focus-follows-mouse` - focus the split under the pointer.
    pub focus_follows_mouse: bool,
    /// File key: `middle-click-paste` - paste the selection on a middle click.
    pub middle_click_paste: bool,
    /// File key: `trigger` (repeatable) - `regex` or `regex | title`; when new
    /// output matches the regex, a desktop notification fires with the line.
    pub trigger: Vec<String>,
    /// File key: `snippet` (repeatable) - `label | command`; inserted into the
    /// focused pane from the snippet picker.
    pub snippet: Vec<String>,
    /// File key: `background-image` - path to an image drawn behind the terminal
    /// (best with background-opacity < 1 so it shows through).
    pub background_image: Option<String>,
    /// File key: `badge` - faint watermark text drawn in each pane; supports
    /// {cwd} and {host} placeholders.
    pub badge: Option<String>,
    /// File key: `profile` (repeatable) - `label | command`; opens a new tab
    /// running the command from the profile picker.
    pub profile: Vec<String>,
}

/// The built-in primary font when none is configured.
pub const DEFAULT_FONT: &str = "Menlo";

impl Options {
    /// The primary font family (first configured, else the built-in default).
    pub fn primary_font(&self) -> &str {
        self.font_family
            .first()
            .map(String::as_str)
            .unwrap_or(DEFAULT_FONT)
    }

    /// Fallback families after the primary, in order.
    pub fn font_fallbacks(&self) -> &[String] {
        self.font_family.get(1..).unwrap_or(&[])
    }

    /// The theme name to use for the given OS appearance. When `theme-light` /
    /// `theme-dark` are set they win for that appearance; otherwise `theme`.
    pub fn theme_for(&self, dark: bool) -> &str {
        let picked = if dark { &self.theme_dark } else { &self.theme_light };
        if picked.is_empty() {
            &self.theme
        } else {
            picked
        }
    }

    /// Whether automatic light/dark theme switching is configured.
    pub fn auto_theme(&self) -> bool {
        !self.theme_light.is_empty() || !self.theme_dark.is_empty()
    }
}

impl Default for Options {
    fn default() -> Self {
        Self {
            font_family: Vec::new(),
            font_size: 13.0,
            font_style: FontStyle::Normal,
            font_feature: Vec::new(),
            adjust_cell_width: 0,
            adjust_cell_height: 0,
            theme: String::new(),
            background: None,
            foreground: None,
            cursor_style: CursorStyle::Block,
            cursor_style_blink: true,
            cursor_color: None,
            cursor_text: None,
            selection_foreground: None,
            selection_background: None,
            bold_is_bright: false,
            minimum_contrast: 1.0,
            unfocused_split_opacity: 0.7,
            split_divider_color: None,
            mouse_scroll_multiplier: 1.0,
            macos_option_as_alt: OptionAsAlt::Auto,
            window_inherit_working_directory: true,
            quit_after_last_window_closed: false,
            title: None,
            clipboard_read: ClipboardAccess::Ask,
            clipboard_write: ClipboardAccess::Allow,
            scrollback_limit: 10_000,
            window_padding_x: 2,
            window_padding_y: 2,
            window_width: 0,
            window_height: 0,
            shell: None,
            working_directory: None,
            copy_on_select: false,
            confirm_close_surface: true,
            confirm_quit: true,
            clipboard_paste_protection: false,
            mouse_hide_while_typing: false,
            shell_integration: true,
            auto_update: true,
            autosuggest_ghost: true,
            autosuggest_popup: false,
            autosuggest_tab: false,
            autosuggest_ai: false,
            autosuggest_history: true,
            autosuggest_commands: true,
            autosuggest_paths: true,
            autosuggest_assist: true,
            session_restore: false,
            tab_title_show_host: false,
            palette: Vec::new(),
            plugin: Vec::new(),
            container: Vec::new(),
            container_engine: None,
            container_persist: false,
            keybind: Vec::new(),
            ai_enabled: false,
            ai_optimize_tokens: false,
            mcp_server_enabled: false,
            relay_enabled: false,
            relay_start_on_launch: false,
            relay_address: "127.0.0.1:7777".to_string(),
            relay_default_agent: "claude".to_string(),
            agent_claude: true,
            agent_codex: false,
            agent_ollama: false,
            agent_gemini: false,
            agent_claude_path: None,
            agent_codex_path: None,
            agent_gemini_path: None,
            agent_claude_args: None,
            agent_codex_args: None,
            agent_gemini_args: None,
            agent_custom: Vec::new(),
            theme_light: String::new(),
            theme_dark: String::new(),
            timestamps: false,
            smart_select: true,
            redact: Vec::new(),
            background_opacity: 1.0,
            focus_follows_mouse: false,
            middle_click_paste: false,
            trigger: Vec::new(),
            snippet: Vec::new(),
            background_image: None,
            badge: None,
            profile: Vec::new(),
        }
    }
}

#[cfg(test)]
#[path = "../tests/options.rs"]
mod tests;
