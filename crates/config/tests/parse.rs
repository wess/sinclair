    use super::*;
    use crate::options::CursorStyle;

    #[test]
    fn empty_input_is_defaults() {
        let (opts, diags) = parse_str("");
        assert_eq!(opts, Options::default());
        assert!(diags.is_empty());
    }

    #[test]
    fn full_sample_config() {
        let src = r#"
# Prompt configuration
font-family = JetBrains Mono
font-size = 14.5
theme = catppuccin
background = #1d1f21
foreground = #c5c8c6
cursor-style = bar
cursor-style-blink = false
scrollback-limit = 50000
window-padding-x = 8
window-padding-y = 4
window-width = 120
window-height = 40
command = /bin/zsh
working-directory = /tmp
copy-on-select = true
confirm-close-surface = false
mouse-hide-while-typing = true
palette = 0=#1d1f21
palette = 1=#cc6666
plugin = ~/.config/prompt/plugins/tools
keybind = ctrl+shift+c=copy_to_clipboard
keybind = ctrl+shift+v=paste_from_clipboard
"#;
        let (o, diags) = parse_str(src);
        assert!(diags.is_empty(), "{diags:?}");
        assert_eq!(o.primary_font(), "JetBrains Mono");
        assert_eq!(o.font_size, 14.5);
        assert_eq!(o.theme, "catppuccin");
        assert_eq!(o.background.as_deref(), Some("#1d1f21"));
        assert_eq!(o.foreground.as_deref(), Some("#c5c8c6"));
        assert_eq!(o.cursor_style, CursorStyle::Bar);
        assert!(!o.cursor_style_blink);
        assert_eq!(o.scrollback_limit, 50_000);
        assert_eq!(o.window_padding_x, 8);
        assert_eq!(o.window_padding_y, 4);
        assert_eq!(o.window_width, 120);
        assert_eq!(o.window_height, 40);
        assert_eq!(o.shell.as_deref(), Some("/bin/zsh"));
        assert_eq!(o.working_directory.as_deref(), Some("/tmp"));
        assert!(o.copy_on_select);
        assert!(!o.confirm_close_surface);
        assert!(o.mouse_hide_while_typing);
        assert_eq!(
            o.palette,
            vec![(0, "#1d1f21".to_string()), (1, "#cc6666".to_string()),]
        );
        assert_eq!(o.plugin, vec!["~/.config/prompt/plugins/tools".to_string()]);
        assert_eq!(
            o.keybind,
            vec![
                "ctrl+shift+c=copy_to_clipboard".to_string(),
                "ctrl+shift+v=paste_from_clipboard".to_string(),
            ]
        );
    }

    #[test]
    fn comments_and_blank_lines_ignored() {
        let src = "# a comment\n\n   \nfont-size = 15\n# font-size = 99\n";
        let (o, diags) = parse_str(src);
        assert!(diags.is_empty());
        assert_eq!(o.font_size, 15.0);
    }

    #[test]
    fn last_wins_for_scalars() {
        let (o, diags) = parse_str("font-size = 10\nfont-size = 20\n");
        assert!(diags.is_empty());
        assert_eq!(o.font_size, 20.0);
    }

    #[test]
    fn empty_value_resets_to_default() {
        let src = "font-size = 20\nfont-size = \n\
                   font-family = Foo\nfont-family =\n\
                   command = /bin/fish\ncommand = \n\
                   copy-on-select = true\ncopy-on-select =\n\
                   palette = 0=#000000\npalette =\n\
                   plugin = /tmp/plugin\nplugin =\n\
                   keybind = a=b\nkeybind =\n";
        let (o, diags) = parse_str(src);
        assert!(diags.is_empty(), "{diags:?}");
        assert_eq!(o, Options::default());
    }

    #[test]
    fn unknown_key_diagnostic_but_continues() {
        let (o, diags) = parse_str("bogus-key = 1\nfont-size = 18\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 1);
        assert_eq!(diags[0].key, "bogus-key");
        assert!(diags[0].message.contains("unknown key"));
        assert_eq!(o.font_size, 18.0);
    }

    #[test]
    fn bad_value_diagnostic_but_continues() {
        let src = "font-size = huge\ncursor-style = wedge\n\
                   copy-on-select = perhaps\npalette = 0=red\nfont-size = 11\n";
        let (o, diags) = parse_str(src);
        assert_eq!(diags.len(), 4);
        assert_eq!(diags[0].line, 1);
        assert_eq!(diags[0].key, "font-size");
        assert_eq!(diags[1].key, "cursor-style");
        assert_eq!(diags[2].key, "copy-on-select");
        assert_eq!(diags[3].key, "palette");
        assert_eq!(o.font_size, 11.0);
        assert!(o.palette.is_empty());
        assert_eq!(o.cursor_style, CursorStyle::Block);
    }

    #[test]
    fn line_without_equals_is_diagnostic() {
        let (o, diags) = parse_str("just some words\nfont-size = 12\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 1);
        assert_eq!(diags[0].key, "");
        assert_eq!(o.font_size, 12.0);
    }

    #[test]
    fn missing_key_is_diagnostic() {
        let (_, diags) = parse_str("= value\n");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("missing key"));
    }

    #[test]
    fn repeated_palette_accumulates_and_reset_clears() {
        let src = "palette = 0=#000000\npalette = 1=#111111\n\
                   palette =\npalette = 2=#222222\n";
        let (o, diags) = parse_str(src);
        assert!(diags.is_empty());
        assert_eq!(o.palette, vec![(2, "#222222".to_string())]);
    }

    #[test]
    fn quoted_values_are_stripped() {
        let src = "font-family = \"SF Mono\"\ntheme = \"gruvbox dark\"\n\
                   working-directory = \"/Users/me/My Stuff\"\n";
        let (o, diags) = parse_str(src);
        assert!(diags.is_empty());
        assert_eq!(o.primary_font(), "SF Mono");
        assert_eq!(o.theme, "gruvbox dark");
        assert_eq!(o.working_directory.as_deref(), Some("/Users/me/My Stuff"));
    }

    #[test]
    fn values_may_contain_equals() {
        let (o, diags) = parse_str("keybind = ctrl+a=select_all\n");
        assert!(diags.is_empty());
        assert_eq!(o.keybind, vec!["ctrl+a=select_all".to_string()]);
    }

    #[test]
    fn whitespace_around_key_and_value_is_trimmed() {
        let (o, diags) = parse_str("   font-size   =   16   \n");
        assert!(diags.is_empty());
        assert_eq!(o.font_size, 16.0);
    }
