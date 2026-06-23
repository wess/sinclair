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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OptionAsAlt {
    #[default]
    False,
    True,
    Left,
    Right,
}

impl OptionAsAlt {
    /// Parse from the config file value: booleans plus left/right.
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
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

/// All configuration options with Ghostty-flavored defaults.
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
    /// File key: `mouse-hide-while-typing`.
    pub mouse_hide_while_typing: bool,
    /// File key: `palette`, repeated `N=#rrggbb` entries (accumulated).
    pub palette: Vec<(u8, String)>,
    /// File key: `plugin`, repeated plugin directories or manifest paths.
    pub plugin: Vec<String>,
    /// File key: `keybind`, raw strings (accumulated, parsed later).
    pub keybind: Vec<String>,
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
            macos_option_as_alt: OptionAsAlt::False,
            window_inherit_working_directory: true,
            quit_after_last_window_closed: true,
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
            mouse_hide_while_typing: false,
            palette: Vec::new(),
            plugin: Vec::new(),
            keybind: Vec::new(),
        }
    }
}

#[cfg(test)]
#[path = "../tests/options.rs"]
mod tests;
