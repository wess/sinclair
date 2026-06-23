use super::*;

#[test]
fn simple_actions() {
    let cases = [
        ("new_window", Action::NewWindow),
        ("new_tab", Action::NewTab),
        ("close_surface", Action::CloseSurface),
        ("close_tab", Action::CloseTab),
        ("close_window", Action::CloseWindow),
        ("close_all_windows", Action::CloseAllWindows),
        ("previous_tab", Action::PreviousTab),
        ("next_tab", Action::NextTab),
        ("copy_to_clipboard", Action::Copy),
        ("copy", Action::Copy),
        ("paste_from_clipboard", Action::Paste),
        ("paste", Action::Paste),
        ("reset_font_size", Action::ResetFontSize),
        ("scroll_page_up", Action::ScrollPageUp),
        ("scroll_page_down", Action::ScrollPageDown),
        ("scroll_to_top", Action::ScrollToTop),
        ("scroll_to_bottom", Action::ScrollToBottom),
        ("clear_screen", Action::ClearScreen),
        ("toggle_semantic_search", Action::ToggleSemanticSearch),
        ("explain_output", Action::ExplainOutput),
        ("compose_command", Action::ComposeCommand),
        ("reload_config", Action::ReloadConfig),
        ("show_help", Action::ShowHelp),
        ("help", Action::ShowHelp),
        ("toggle_fullscreen", Action::ToggleFullscreen),
        ("zoom_split", Action::ZoomSplit),
        ("toggle_split_zoom", Action::ZoomSplit),
        ("equalize_splits", Action::EqualizeSplits),
        ("minimize_window", Action::MinimizeWindow),
        ("zoom_window", Action::ZoomWindow),
        ("hide_all", Action::HideAll),
        ("bring_all_to_front", Action::BringAllToFront),
        ("return_to_default_size", Action::ReturnToDefaultSize),
        ("use_as_default", Action::UseAsDefault),
        ("change_tab_title", Action::ChangeTabTitle),
        ("change_terminal_title", Action::ChangeTerminalTitle),
        ("toggle_read_only", Action::ToggleReadOnly),
        ("toggle_quick_terminal", Action::ToggleQuickTerminal),
        ("quick_terminal", Action::ToggleQuickTerminal),
        ("quit", Action::Quit),
        ("unbind", Action::Unbound),
    ];
    for (src, want) in cases {
        assert_eq!(Action::parse(src), Ok(want), "{src}");
    }
}

#[test]
fn name_is_case_insensitive() {
    assert_eq!(Action::parse("NEW_TAB"), Ok(Action::NewTab));
    assert_eq!(
        Action::parse("New_Split:Right"),
        Ok(Action::NewSplit(SplitDirection::Right))
    );
}

#[test]
fn new_split_params() {
    let cases = [
        ("new_split:right", SplitDirection::Right),
        ("new_split:down", SplitDirection::Down),
        ("new_split:left", SplitDirection::Left),
        ("new_split:up", SplitDirection::Up),
    ];
    for (src, dir) in cases {
        assert_eq!(Action::parse(src), Ok(Action::NewSplit(dir)), "{src}");
    }
    assert!(Action::parse("new_split:sideways").is_err());
    assert!(Action::parse("new_split").is_err());
    assert!(Action::parse("new_split:").is_err());
}

#[test]
fn resize_split_params() {
    let cases = [
        ("resize_split:up", ResizeDir::Up),
        ("resize_split:down", ResizeDir::Down),
        ("resize_split:left", ResizeDir::Left),
        ("resize_split:right", ResizeDir::Right),
    ];
    for (src, dir) in cases {
        assert_eq!(Action::parse(src), Ok(Action::ResizeSplit(dir)), "{src}");
    }
    assert!(Action::parse("resize_split:bigger").is_err());
    assert!(Action::parse("resize_split").is_err());
}

#[test]
fn goto_split_params() {
    let cases = [
        ("goto_split:previous", SplitFocus::Previous),
        ("goto_split:next", SplitFocus::Next),
        ("goto_split:up", SplitFocus::Up),
        ("goto_split:down", SplitFocus::Down),
        ("goto_split:left", SplitFocus::Left),
        ("goto_split:right", SplitFocus::Right),
    ];
    for (src, focus) in cases {
        assert_eq!(Action::parse(src), Ok(Action::GotoSplit(focus)), "{src}");
    }
    assert!(Action::parse("goto_split:over").is_err());
    assert!(Action::parse("goto_split").is_err());
}

#[test]
fn goto_tab_params() {
    assert_eq!(Action::parse("goto_tab:3"), Ok(Action::GotoTab(3)));
    assert_eq!(Action::parse("goto_tab:-1"), Ok(Action::GotoTab(-1)));
    assert!(Action::parse("goto_tab:0").is_err());
    assert!(Action::parse("goto_tab:first").is_err());
    assert!(Action::parse("goto_tab").is_err());
}

#[test]
fn move_tab_params() {
    assert_eq!(Action::parse("move_tab:1"), Ok(Action::MoveTab(1)));
    assert_eq!(Action::parse("move_tab:-2"), Ok(Action::MoveTab(-2)));
    assert!(Action::parse("move_tab").is_err());
    assert!(Action::parse("move_tab:x").is_err());
}

#[test]
fn font_size_params() {
    assert_eq!(
        Action::parse("increase_font_size"),
        Ok(Action::IncreaseFontSize(1.0))
    );
    assert_eq!(
        Action::parse("increase_font_size:2.5"),
        Ok(Action::IncreaseFontSize(2.5))
    );
    assert_eq!(
        Action::parse("decrease_font_size"),
        Ok(Action::DecreaseFontSize(1.0))
    );
    assert_eq!(
        Action::parse("decrease_font_size:0.5"),
        Ok(Action::DecreaseFontSize(0.5))
    );
    assert!(Action::parse("increase_font_size:0").is_err());
    assert!(Action::parse("increase_font_size:-1").is_err());
    assert!(Action::parse("increase_font_size:big").is_err());
}

#[test]
fn plugin_command_param() {
    assert_eq!(
        Action::parse("plugin_command:tools/top"),
        Ok(Action::PluginCommand("tools/top".to_string()))
    );
    assert!(Action::parse("plugin_command").is_err());
    assert!(Action::parse("plugin_command:tools").is_err());
    assert!(Action::parse("plugin_command:Tools/top").is_err());
}

#[test]
fn macro_actions() {
    assert_eq!(Action::parse("macro_record"), Ok(Action::MacroRecord));
    assert_eq!(
        Action::parse("macro:deploy"),
        Ok(Action::MacroReplay("deploy".to_string()))
    );
    assert_eq!(
        Action::parse("macro:build.all"),
        Ok(Action::MacroReplay("build.all".to_string()))
    );
    assert!(Action::parse("macro").is_err());
    assert!(Action::parse("macro:Deploy").is_err());
    assert!(Action::parse("macro_record:now").is_err());
}

#[test]
fn param_on_paramless_action_is_error() {
    assert!(Action::parse("quit:now").is_err());
    assert!(Action::parse("new_tab:2").is_err());
    assert!(Action::parse("unbind:all").is_err());
}

#[test]
fn unknown_action_is_error() {
    assert!(Action::parse("select_all").is_err());
    assert!(Action::parse("").is_err());
}

#[test]
fn to_config_round_trips() {
    let cases = [
        Action::NewWindow,
        Action::NewTab,
        Action::CloseSurface,
        Action::CloseAllWindows,
        Action::NewSplit(SplitDirection::Down),
        Action::GotoSplit(SplitFocus::Left),
        Action::ResizeSplit(ResizeDir::Up),
        Action::GotoTab(3),
        Action::GotoTab(-1),
        Action::MoveTab(-2),
        Action::Copy,
        Action::Paste,
        Action::IncreaseFontSize(1.0),
        Action::IncreaseFontSize(2.5),
        Action::DecreaseFontSize(1.0),
        Action::JumpToPrompt(-1),
        Action::PluginCommand("tools/top".into()),
        Action::MacroReplay("deploy".into()),
        Action::ToggleQuickTerminal,
        Action::Quit,
        Action::Unbound,
    ];
    for want in cases {
        let s = want.to_config();
        assert_eq!(Action::parse(&s), Ok(want.clone()), "{s}");
    }
}
