use super::*;

#[test]
fn defaults() {
    let o = Options::default();
    assert!(o.font_family.is_empty());
    assert_eq!(o.primary_font(), "Menlo");
    assert_eq!(o.font_size, 13.0);
    assert_eq!(o.theme, "");
    assert_eq!(o.background, None);
    assert_eq!(o.foreground, None);
    assert_eq!(o.cursor_style, CursorStyle::Block);
    assert!(o.cursor_style_blink);
    assert_eq!(o.scrollback_limit, 10_000);
    assert_eq!(o.window_padding_x, 2);
    assert_eq!(o.window_padding_y, 2);
    assert_eq!(o.window_width, 0);
    assert_eq!(o.window_height, 0);
    assert_eq!(o.shell, None);
    assert_eq!(o.working_directory, None);
    assert!(!o.copy_on_select);
    assert!(o.confirm_close_surface);
    assert!(!o.mouse_hide_while_typing);
    assert!(o.palette.is_empty());
    assert!(o.plugin.is_empty());
    assert!(o.keybind.is_empty());
    assert_eq!(o.font_style, FontStyle::Normal);
    assert!(o.font_feature.is_empty());
    assert_eq!(o.adjust_cell_width, 0);
    assert_eq!(o.adjust_cell_height, 0);
    assert_eq!(o.cursor_color, None);
    assert_eq!(o.cursor_text, None);
    assert_eq!(o.selection_foreground, None);
    assert_eq!(o.selection_background, None);
    assert!(!o.bold_is_bright);
    assert_eq!(o.minimum_contrast, 1.0);
    assert_eq!(o.unfocused_split_opacity, 0.7);
    assert_eq!(o.split_divider_color, None);
    assert_eq!(o.mouse_scroll_multiplier, 1.0);
    assert_eq!(o.macos_option_as_alt, OptionAsAlt::False);
    assert!(o.window_inherit_working_directory);
    assert!(o.quit_after_last_window_closed);
    assert_eq!(o.title, None);
    assert_eq!(o.clipboard_read, ClipboardAccess::Ask);
    assert_eq!(o.clipboard_write, ClipboardAccess::Allow);
}

#[test]
fn font_style_parse() {
    assert_eq!(FontStyle::parse("normal"), Some(FontStyle::Normal));
    assert_eq!(FontStyle::parse("Bold"), Some(FontStyle::Bold));
    assert_eq!(FontStyle::parse("ITALIC"), Some(FontStyle::Italic));
    assert_eq!(FontStyle::parse("bold-italic"), Some(FontStyle::BoldItalic));
    assert_eq!(FontStyle::parse("bold italic"), None);
    assert_eq!(FontStyle::parse(""), None);
}

#[test]
fn option_as_alt_parse() {
    assert_eq!(OptionAsAlt::parse("false"), Some(OptionAsAlt::False));
    assert_eq!(OptionAsAlt::parse("true"), Some(OptionAsAlt::True));
    assert_eq!(OptionAsAlt::parse("no"), Some(OptionAsAlt::False));
    assert_eq!(OptionAsAlt::parse("1"), Some(OptionAsAlt::True));
    assert_eq!(OptionAsAlt::parse("Left"), Some(OptionAsAlt::Left));
    assert_eq!(OptionAsAlt::parse("RIGHT"), Some(OptionAsAlt::Right));
    assert_eq!(OptionAsAlt::parse("middle"), None);
    assert_eq!(OptionAsAlt::parse(""), None);
}

#[test]
fn clipboard_access_parse() {
    assert_eq!(
        ClipboardAccess::parse("allow"),
        Some(ClipboardAccess::Allow)
    );
    assert_eq!(ClipboardAccess::parse("Ask"), Some(ClipboardAccess::Ask));
    assert_eq!(ClipboardAccess::parse("DENY"), Some(ClipboardAccess::Deny));
    assert_eq!(ClipboardAccess::parse("never"), None);
    assert_eq!(ClipboardAccess::parse(""), None);
}

#[test]
fn cursor_style_parse() {
    assert_eq!(CursorStyle::parse("block"), Some(CursorStyle::Block));
    assert_eq!(CursorStyle::parse("Bar"), Some(CursorStyle::Bar));
    assert_eq!(
        CursorStyle::parse("UNDERLINE"),
        Some(CursorStyle::Underline)
    );
    assert_eq!(CursorStyle::parse("beam"), None);
    assert_eq!(CursorStyle::parse(""), None);
}
