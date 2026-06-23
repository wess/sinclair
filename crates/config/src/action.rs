//! Keybind actions, Ghostty-style: a name plus an optional `:param`.

/// Direction for `new_split`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitDirection {
    Right,
    Down,
    Left,
    Up,
}

impl SplitDirection {
    fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "right" => Some(Self::Right),
            "down" => Some(Self::Down),
            "left" => Some(Self::Left),
            "up" => Some(Self::Up),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Right => "right",
            Self::Down => "down",
            Self::Left => "left",
            Self::Up => "up",
        }
    }
}

/// Direction to nudge a split divider for `resize_split`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResizeDir {
    Up,
    Down,
    Left,
    Right,
}

impl ResizeDir {
    fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "up" => Some(Self::Up),
            "down" => Some(Self::Down),
            "left" => Some(Self::Left),
            "right" => Some(Self::Right),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Up => "up",
            Self::Down => "down",
            Self::Left => "left",
            Self::Right => "right",
        }
    }
}

/// Target for `goto_split`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitFocus {
    Previous,
    Next,
    Up,
    Down,
    Left,
    Right,
}

impl SplitFocus {
    fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "previous" => Some(Self::Previous),
            "next" => Some(Self::Next),
            "up" => Some(Self::Up),
            "down" => Some(Self::Down),
            "left" => Some(Self::Left),
            "right" => Some(Self::Right),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Previous => "previous",
            Self::Next => "next",
            Self::Up => "up",
            Self::Down => "down",
            Self::Left => "left",
            Self::Right => "right",
        }
    }
}

/// A keybind action. Names follow Ghostty: `new_tab`, `goto_tab:3`,
/// `increase_font_size:1`, `unbind`, ...
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// Open a fresh top-level window.
    NewWindow,
    NewTab,
    CloseSurface,
    /// Close the current tab and all its panes.
    CloseTab,
    /// Close the current window.
    CloseWindow,
    /// Close every open window.
    CloseAllWindows,
    NewSplit(SplitDirection),
    GotoSplit(SplitFocus),
    /// Toggle maximizing the focused split pane to fill the tab.
    ZoomSplit,
    /// Reset every split divider in the tab to an even 50/50.
    EqualizeSplits,
    /// Nudge the divider next to the focused pane in a direction.
    ResizeSplit(ResizeDir),
    /// 1-based tab index; negative counts from the end (`-1` = last).
    GotoTab(i32),
    PreviousTab,
    NextTab,
    /// Move the current tab by a signed delta.
    MoveTab(i32),
    Copy,
    Paste,
    IncreaseFontSize(f32),
    DecreaseFontSize(f32),
    ResetFontSize,
    ScrollPageUp,
    ScrollPageDown,
    ScrollToTop,
    ScrollToBottom,
    /// Jump the viewport by a signed number of shell prompts (negative =
    /// toward the top/older).
    JumpToPrompt(i32),
    ClearScreen,
    /// Toggle the scrollback search overlay.
    ToggleSearch,
    /// Toggle semantic search across prompt/output blocks.
    ToggleSemanticSearch,
    /// Explain selected output, falling back to the last prompt block.
    ExplainOutput,
    /// Compose a shell command from natural language and insert it.
    ComposeCommand,
    /// Run a command contributed by a plugin, addressed as `plugin/command`.
    PluginCommand(String),
    /// Toggle command-macro recording: start capturing typed commands, or
    /// stop and name/save the capture.
    MacroRecord,
    /// Replay a saved macro by name (`macro:<name>`).
    MacroReplay(String),
    /// Toggle the settings panel.
    ToggleSettings,
    /// Open the documentation window.
    ShowHelp,
    ReloadConfig,
    ToggleFullscreen,
    /// Minimize the window to the Dock.
    MinimizeWindow,
    /// Standard macOS window zoom (green button).
    ZoomWindow,
    /// Hide every Prompt window (macOS "Hide").
    HideAll,
    /// Bring all Prompt windows to the front.
    BringAllToFront,
    /// Resize the window back to the configured default cell grid.
    ReturnToDefaultSize,
    /// Persist the current window's cell grid as the default size.
    UseAsDefault,
    /// Prompt for and set the active tab's label.
    ChangeTabTitle,
    /// Prompt for and set the focused pane's title.
    ChangeTerminalTitle,
    /// Toggle input gating on the focused pane.
    ToggleReadOnly,
    /// Toggle the Quake-style dropdown quick terminal.
    ToggleQuickTerminal,
    Quit,
    /// The special `unbind` action: removes the trigger's binding.
    Unbound,
}

impl Action {
    /// Parse `name` or `name:param`. Unknown names or bad params are errors.
    pub fn parse(s: &str) -> Result<Self, String> {
        let (name, param) = match s.split_once(':') {
            Some((n, p)) => (n.trim().to_ascii_lowercase(), Some(p.trim())),
            None => (s.trim().to_ascii_lowercase(), None),
        };
        match name.as_str() {
            "new_window" => only(Self::NewWindow, &name, param),
            "new_tab" => only(Self::NewTab, &name, param),
            "close_surface" => only(Self::CloseSurface, &name, param),
            "close_tab" => only(Self::CloseTab, &name, param),
            "close_window" => only(Self::CloseWindow, &name, param),
            "close_all_windows" => only(Self::CloseAllWindows, &name, param),
            "new_split" => {
                let p = req(&name, param)?;
                let dir = SplitDirection::parse(p)
                    .ok_or_else(|| format!("invalid new_split direction `{p}`"))?;
                Ok(Self::NewSplit(dir))
            }
            "goto_split" => {
                let p = req(&name, param)?;
                let focus = SplitFocus::parse(p)
                    .ok_or_else(|| format!("invalid goto_split target `{p}`"))?;
                Ok(Self::GotoSplit(focus))
            }
            "zoom_split" | "toggle_split_zoom" => only(Self::ZoomSplit, &name, param),
            "equalize_splits" => only(Self::EqualizeSplits, &name, param),
            "resize_split" => {
                let p = req(&name, param)?;
                let dir = ResizeDir::parse(p)
                    .ok_or_else(|| format!("invalid resize_split direction `{p}`"))?;
                Ok(Self::ResizeSplit(dir))
            }
            "goto_tab" => {
                let n = int(&name, param)?;
                if n == 0 {
                    return Err("goto_tab requires a non-zero index".to_string());
                }
                Ok(Self::GotoTab(n))
            }
            "previous_tab" => only(Self::PreviousTab, &name, param),
            "next_tab" => only(Self::NextTab, &name, param),
            "move_tab" => Ok(Self::MoveTab(int(&name, param)?)),
            "copy_to_clipboard" | "copy" => only(Self::Copy, &name, param),
            "paste_from_clipboard" | "paste" => only(Self::Paste, &name, param),
            "increase_font_size" => Ok(Self::IncreaseFontSize(amount(&name, param)?)),
            "decrease_font_size" => Ok(Self::DecreaseFontSize(amount(&name, param)?)),
            "reset_font_size" => only(Self::ResetFontSize, &name, param),
            "scroll_page_up" => only(Self::ScrollPageUp, &name, param),
            "scroll_page_down" => only(Self::ScrollPageDown, &name, param),
            "scroll_to_top" => only(Self::ScrollToTop, &name, param),
            "scroll_to_bottom" => only(Self::ScrollToBottom, &name, param),
            "jump_to_prompt" => Ok(Self::JumpToPrompt(int(&name, param)?)),
            "clear_screen" => only(Self::ClearScreen, &name, param),
            "toggle_search" => only(Self::ToggleSearch, &name, param),
            "toggle_semantic_search" => only(Self::ToggleSemanticSearch, &name, param),
            "explain_output" => only(Self::ExplainOutput, &name, param),
            "compose_command" => only(Self::ComposeCommand, &name, param),
            "plugin_command" => {
                let p = req(&name, param)?;
                if valid_plugin_command(p) {
                    Ok(Self::PluginCommand(p.to_string()))
                } else {
                    Err("plugin_command requires `plugin/command`".to_string())
                }
            }
            "macro_record" => only(Self::MacroRecord, &name, param),
            "macro" => {
                let p = req(&name, param)?;
                if valid_id(p) {
                    Ok(Self::MacroReplay(p.to_string()))
                } else {
                    Err("macro requires a name ([a-z0-9.-])".to_string())
                }
            }
            "open_settings" | "toggle_settings" => only(Self::ToggleSettings, &name, param),
            "show_help" | "help" => only(Self::ShowHelp, &name, param),
            "reload_config" => only(Self::ReloadConfig, &name, param),
            "toggle_fullscreen" => only(Self::ToggleFullscreen, &name, param),
            "minimize_window" | "minimize" => only(Self::MinimizeWindow, &name, param),
            "zoom_window" => only(Self::ZoomWindow, &name, param),
            "hide_all" | "toggle_visibility" => only(Self::HideAll, &name, param),
            "bring_all_to_front" => only(Self::BringAllToFront, &name, param),
            "return_to_default_size" | "reset_window_size" => {
                only(Self::ReturnToDefaultSize, &name, param)
            }
            "use_as_default" => only(Self::UseAsDefault, &name, param),
            "change_tab_title" => only(Self::ChangeTabTitle, &name, param),
            "change_terminal_title" => only(Self::ChangeTerminalTitle, &name, param),
            "toggle_read_only" => only(Self::ToggleReadOnly, &name, param),
            "toggle_quick_terminal" | "quick_terminal" => {
                only(Self::ToggleQuickTerminal, &name, param)
            }
            "quit" => only(Self::Quit, &name, param),
            "unbind" => only(Self::Unbound, &name, param),
            _ => Err(format!("unknown action `{name}`")),
        }
    }

    /// The canonical config string for this action, round-tripping through
    /// [`Action::parse`]. Used to write keybinds back to the config file.
    pub fn to_config(&self) -> String {
        match self {
            Self::NewWindow => "new_window".into(),
            Self::NewTab => "new_tab".into(),
            Self::CloseSurface => "close_surface".into(),
            Self::CloseTab => "close_tab".into(),
            Self::CloseWindow => "close_window".into(),
            Self::CloseAllWindows => "close_all_windows".into(),
            Self::NewSplit(d) => format!("new_split:{}", d.as_str()),
            Self::GotoSplit(f) => format!("goto_split:{}", f.as_str()),
            Self::ZoomSplit => "zoom_split".into(),
            Self::EqualizeSplits => "equalize_splits".into(),
            Self::ResizeSplit(d) => format!("resize_split:{}", d.as_str()),
            Self::GotoTab(n) => format!("goto_tab:{n}"),
            Self::PreviousTab => "previous_tab".into(),
            Self::NextTab => "next_tab".into(),
            Self::MoveTab(n) => format!("move_tab:{n}"),
            Self::Copy => "copy_to_clipboard".into(),
            Self::Paste => "paste_from_clipboard".into(),
            Self::IncreaseFontSize(a) => font_size_action("increase_font_size", *a),
            Self::DecreaseFontSize(a) => font_size_action("decrease_font_size", *a),
            Self::ResetFontSize => "reset_font_size".into(),
            Self::ScrollPageUp => "scroll_page_up".into(),
            Self::ScrollPageDown => "scroll_page_down".into(),
            Self::ScrollToTop => "scroll_to_top".into(),
            Self::ScrollToBottom => "scroll_to_bottom".into(),
            Self::JumpToPrompt(n) => format!("jump_to_prompt:{n}"),
            Self::ClearScreen => "clear_screen".into(),
            Self::ToggleSearch => "toggle_search".into(),
            Self::ToggleSemanticSearch => "toggle_semantic_search".into(),
            Self::ExplainOutput => "explain_output".into(),
            Self::ComposeCommand => "compose_command".into(),
            Self::PluginCommand(s) => format!("plugin_command:{s}"),
            Self::MacroRecord => "macro_record".into(),
            Self::MacroReplay(s) => format!("macro:{s}"),
            Self::ToggleSettings => "toggle_settings".into(),
            Self::ShowHelp => "show_help".into(),
            Self::ReloadConfig => "reload_config".into(),
            Self::ToggleFullscreen => "toggle_fullscreen".into(),
            Self::MinimizeWindow => "minimize_window".into(),
            Self::ZoomWindow => "zoom_window".into(),
            Self::HideAll => "hide_all".into(),
            Self::BringAllToFront => "bring_all_to_front".into(),
            Self::ReturnToDefaultSize => "return_to_default_size".into(),
            Self::UseAsDefault => "use_as_default".into(),
            Self::ChangeTabTitle => "change_tab_title".into(),
            Self::ChangeTerminalTitle => "change_terminal_title".into(),
            Self::ToggleReadOnly => "toggle_read_only".into(),
            Self::ToggleQuickTerminal => "toggle_quick_terminal".into(),
            Self::Quit => "quit".into(),
            Self::Unbound => "unbind".into(),
        }
    }
}

/// `increase_font_size`/`decrease_font_size` omit the `:1` default amount.
fn font_size_action(name: &str, amount: f32) -> String {
    if amount == 1.0 {
        name.to_string()
    } else {
        format!("{name}:{amount}")
    }
}

fn valid_plugin_command(s: &str) -> bool {
    let Some((plugin, command)) = s.split_once('/') else {
        return false;
    };
    valid_id(plugin) && valid_id(command)
}

fn valid_id(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.' || b == b'-')
}

/// The action takes no parameter.
fn only(action: Action, name: &str, param: Option<&str>) -> Result<Action, String> {
    match param {
        None => Ok(action),
        Some(_) => Err(format!("action `{name}` takes no parameter")),
    }
}

/// The action requires a non-empty parameter.
fn req<'a>(name: &str, param: Option<&'a str>) -> Result<&'a str, String> {
    match param {
        Some(p) if !p.is_empty() => Ok(p),
        _ => Err(format!("action `{name}` requires a parameter")),
    }
}

/// The action requires an integer parameter.
fn int(name: &str, param: Option<&str>) -> Result<i32, String> {
    req(name, param)?
        .parse()
        .map_err(|_| format!("action `{name}` requires an integer parameter"))
}

/// Optional positive number parameter, defaulting to 1.
fn amount(name: &str, param: Option<&str>) -> Result<f32, String> {
    let Some(p) = param else {
        return Ok(1.0);
    };
    let v: f32 = p
        .parse()
        .map_err(|_| format!("action `{name}` requires a number parameter"))?;
    if v.is_finite() && v > 0.0 {
        Ok(v)
    } else {
        Err(format!("action `{name}` requires a positive number"))
    }
}

#[cfg(test)]
#[path = "../tests/action.rs"]
mod tests;
