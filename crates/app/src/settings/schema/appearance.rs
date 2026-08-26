//! Appearance section: themes, colors, fonts, and the cursor.

use super::{choice, list, opt, slider, strs, text, toggle, ListKind, Section, Setting};

pub(super) fn settings() -> Vec<Setting> {
  let s = Section::Appearance;
  vec![
    choice(
      "theme",
      "Theme",
      "The built-in color scheme.",
      s,
      |o| {
        if o.theme.trim().is_empty() {
          "default".to_string()
        } else {
          o.theme.clone()
        }
      },
      theme_names,
      Some("default"),
    ),
    choice(
      "theme-light",
      "Light-mode theme",
      "Scheme used when the OS is in light mode; off follows Theme.",
      s,
      |o| {
        if o.theme_light.is_empty() {
          "off".to_string()
        } else {
          o.theme_light.clone()
        }
      },
      theme_names,
      Some("off"),
    ),
    choice(
      "theme-dark",
      "Dark-mode theme",
      "Scheme used when the OS is in dark mode; off follows Theme.",
      s,
      |o| {
        if o.theme_dark.is_empty() {
          "off".to_string()
        } else {
          o.theme_dark.clone()
        }
      },
      theme_names,
      Some("off"),
    ),
    choice(
      "font-style",
      "Font style",
      "Style of the base font.",
      s,
      |o| {
        match o.font_style {
          config::FontStyle::Normal => "normal",
          config::FontStyle::Bold => "bold",
          config::FontStyle::Italic => "italic",
          config::FontStyle::BoldItalic => "bold-italic",
        }
        .to_string()
      },
      || strs(&["normal", "bold", "italic", "bold-italic"]),
      None,
    ),
    choice(
      "cursor-style",
      "Cursor style",
      "Block, bar, or underline.",
      s,
      |o| {
        match o.cursor_style {
          config::CursorStyle::Block => "block",
          config::CursorStyle::Bar => "bar",
          config::CursorStyle::Underline => "underline",
        }
        .to_string()
      },
      || strs(&["block", "bar", "underline"]),
      None,
    ),
    toggle(
      "cursor-style-blink",
      "Cursor blink",
      "Blink the cursor when the pane is focused.",
      s,
      |o| o.cursor_style_blink,
    ),
    text(
      "foreground",
      "Foreground",
      "Default text color as #rrggbb, overriding the theme.",
      s,
      |o| opt(&o.foreground),
      "Theme",
    ),
    text(
      "background",
      "Background",
      "Background color as #rrggbb, overriding the theme.",
      s,
      |o| opt(&o.background),
      "Theme",
    ),
    text(
      "cursor-color",
      "Cursor color",
      "Cursor block color as #rrggbb.",
      s,
      |o| opt(&o.cursor_color),
      "Theme",
    ),
    text(
      "cursor-text",
      "Cursor text color",
      "Color of the character under a block cursor.",
      s,
      |o| opt(&o.cursor_text),
      "Theme",
    ),
    text(
      "selection-foreground",
      "Selection foreground",
      "Text color inside a selection.",
      s,
      |o| opt(&o.selection_foreground),
      "Theme",
    ),
    text(
      "selection-background",
      "Selection background",
      "Highlight color of a selection.",
      s,
      |o| opt(&o.selection_background),
      "Theme",
    ),
    toggle(
      "bold-is-bright",
      "Bold is bright",
      "Render bold text in the bright palette variant.",
      s,
      |o| o.bold_is_bright,
    ),
    slider(
      "minimum-contrast",
      "Minimum contrast",
      "Force at least this contrast ratio between text and background.",
      s,
      |o| o.minimum_contrast,
      (1.0, 21.0, 0.5),
      false,
    ),
    list(
      ListKind::FontFamily,
      "Font fallback chain; the first entry is the primary font.",
      s,
    ),
    list(
      ListKind::FontFeature,
      "OpenType feature tags, like -liga or +ss01.",
      s,
    ),
    list(
      ListKind::Palette,
      "Overrides for the 256-color palette, as N=#rrggbb.",
      s,
    ),
  ]
}

fn theme_names() -> Vec<String> {
  theme::names().iter().map(|s| s.to_string()).collect()
}
