    use crate::options::{ClipboardAccess, FontStyle, OptionAsAlt, Options};
    use crate::parse::parse_str;

    #[test]
    fn new_options_parse() {
        let src = r#"
font-style = bold-italic
font-feature = -liga
font-feature = +ss01
adjust-cell-width = 2
adjust-cell-height = -1px
cursor-color = #ff0000
cursor-text = 00ff00
selection-foreground = #FFFFFF
selection-background = #000000
bold-is-bright = true
minimum-contrast = 3
unfocused-split-opacity = 0.5
split-divider-color = #444444
mouse-scroll-multiplier = 2.5
macos-option-as-alt = left
window-inherit-working-directory = false
quit-after-last-window-closed = false
title = my terminal
clipboard-read = deny
clipboard-write = ask
"#;
        let (o, diags) = parse_str(src);
        assert!(diags.is_empty(), "{diags:?}");
        assert_eq!(o.font_style, FontStyle::BoldItalic);
        assert_eq!(
            o.font_feature,
            vec!["-liga".to_string(), "+ss01".to_string()]
        );
        assert_eq!(o.adjust_cell_width, 2);
        assert_eq!(o.adjust_cell_height, -1);
        assert_eq!(o.cursor_color.as_deref(), Some("#ff0000"));
        assert_eq!(o.cursor_text.as_deref(), Some("#00ff00"));
        assert_eq!(o.selection_foreground.as_deref(), Some("#ffffff"));
        assert_eq!(o.selection_background.as_deref(), Some("#000000"));
        assert!(o.bold_is_bright);
        assert_eq!(o.minimum_contrast, 3.0);
        assert_eq!(o.unfocused_split_opacity, 0.5);
        assert_eq!(o.split_divider_color.as_deref(), Some("#444444"));
        assert_eq!(o.mouse_scroll_multiplier, 2.5);
        assert_eq!(o.macos_option_as_alt, OptionAsAlt::Left);
        assert!(!o.window_inherit_working_directory);
        assert!(!o.quit_after_last_window_closed);
        assert_eq!(o.title.as_deref(), Some("my terminal"));
        assert_eq!(o.clipboard_read, ClipboardAccess::Deny);
        assert_eq!(o.clipboard_write, ClipboardAccess::Ask);
    }

    #[test]
    fn new_options_bad_values_diagnose() {
        let cases = [
            "font-style = fancy",
            "font-feature = no good",
            "adjust-cell-width = wide",
            "adjust-cell-height = 10%",
            "cursor-color = red",
            "cursor-text = #fff",
            "selection-foreground = #12345",
            "selection-background = blue",
            "bold-is-bright = maybe",
            "minimum-contrast = abc",
            "unfocused-split-opacity = dim",
            "split-divider-color = gray",
            "mouse-scroll-multiplier = fast",
            "macos-option-as-alt = middle",
            "window-inherit-working-directory = sometimes",
            "quit-after-last-window-closed = perhaps",
            "clipboard-read = never",
            "clipboard-write = always",
        ];
        for case in cases {
            let (o, diags) = parse_str(case);
            assert_eq!(diags.len(), 1, "no diagnostic for `{case}`");
            assert_eq!(o, Options::default(), "value applied for `{case}`");
        }
    }

    #[test]
    fn new_options_empty_value_resets() {
        let src = "font-style = bold\nfont-style =\n\
                   font-feature = -liga\nfont-feature =\n\
                   adjust-cell-width = 3\nadjust-cell-width =\n\
                   adjust-cell-height = 3\nadjust-cell-height =\n\
                   cursor-color = #ff0000\ncursor-color =\n\
                   cursor-text = #ff0000\ncursor-text =\n\
                   selection-foreground = #ff0000\nselection-foreground =\n\
                   selection-background = #ff0000\nselection-background =\n\
                   bold-is-bright = true\nbold-is-bright =\n\
                   minimum-contrast = 4\nminimum-contrast =\n\
                   unfocused-split-opacity = 0.5\nunfocused-split-opacity =\n\
                   split-divider-color = #ff0000\nsplit-divider-color =\n\
                   mouse-scroll-multiplier = 3\nmouse-scroll-multiplier =\n\
                   macos-option-as-alt = left\nmacos-option-as-alt =\n\
                   window-inherit-working-directory = false\nwindow-inherit-working-directory =\n\
                   quit-after-last-window-closed = false\nquit-after-last-window-closed =\n\
                   title = x\ntitle =\n\
                   clipboard-read = deny\nclipboard-read =\n\
                   clipboard-write = deny\nclipboard-write =\n";
        let (o, diags) = parse_str(src);
        assert!(diags.is_empty(), "{diags:?}");
        assert_eq!(o, Options::default());
    }

    #[test]
    fn ranged_values_clamp() {
        let src = "minimum-contrast = 0.5\nunfocused-split-opacity = 0.01\n\
                   mouse-scroll-multiplier = 0.001\n";
        let (o, diags) = parse_str(src);
        assert!(diags.is_empty());
        assert_eq!(o.minimum_contrast, 1.0);
        assert_eq!(o.unfocused_split_opacity, 0.15);
        assert_eq!(o.mouse_scroll_multiplier, 0.01);

        let src = "minimum-contrast = 100\nunfocused-split-opacity = 2\n\
                   mouse-scroll-multiplier = 99999999\n";
        let (o, diags) = parse_str(src);
        assert!(diags.is_empty());
        assert_eq!(o.minimum_contrast, 21.0);
        assert_eq!(o.unfocused_split_opacity, 1.0);
        assert_eq!(o.mouse_scroll_multiplier, 10_000.0);
    }

    #[test]
    fn font_feature_accumulates_and_resets() {
        let src = "font-feature = -liga\nfont-feature = ss01\n\
                   font-feature =\nfont-feature = +calt\n";
        let (o, diags) = parse_str(src);
        assert!(diags.is_empty());
        assert_eq!(o.font_feature, vec!["+calt".to_string()]);
    }

    #[test]
    fn font_family_builds_a_fallback_chain() {
        let src = "font-family = JetBrains Mono\nfont-family = Menlo\n\
                   font-family = Apple Color Emoji\n";
        let (o, diags) = parse_str(src);
        assert!(diags.is_empty(), "{diags:?}");
        assert_eq!(o.primary_font(), "JetBrains Mono");
        assert_eq!(o.font_fallbacks(), ["Menlo", "Apple Color Emoji"]);
        // An empty value resets the chain back to the default.
        let (o, _) = parse_str("font-family = X\nfont-family =\n");
        assert!(o.font_family.is_empty());
        assert_eq!(o.primary_font(), "Menlo");
    }
