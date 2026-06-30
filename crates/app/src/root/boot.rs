use super::*;

/// Upsert one `key = value` line into the user's config file, creating it if
/// needed. Shared shape with the settings panel's writer.
pub(crate) fn write_config(key: &str, value: &str) {
    let Some(path) = config::default_path() else {
        return;
    };
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let updated = config::upsert(&text, key, value);
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(&path, updated);
}

pub(crate) fn loadplugins(opts: &config::Options) -> Vec<plugin::Plugin> {
    let (plugins, diags) = plugin::load(&opts.plugin);
    for d in &diags {
        if d.line == 0 {
            eprintln!("prompt: plugin {}: {}", d.path.display(), d.message);
        } else {
            eprintln!(
                "prompt: plugin {} line {}: {}",
                d.path.display(),
                d.line,
                d.message
            );
        }
    }
    plugins
}

/// Strip a leading `user@host:` prefix from a shell-set title, leaving just the
/// path. Shells default the terminal title to `\u@\h: \w`, which wraps to two
/// lines in a tab and reads poorly; the host is noise in a tab label.
pub(crate) fn strip_user_host(title: &str) -> &str {
    if let Some((head, rest)) = title.split_once(':') {
        if head.contains('@') && !head.contains([' ', '/']) {
            return rest.trim_start();
        }
    }
    title
}

/// The curated set of actions the command palette offers, with display
/// labels. Ordered roughly by how often they're reached.
pub(crate) fn palette_catalog() -> Vec<(&'static str, Action)> {
    vec![
        ("New Window", Action::NewWindow),
        ("New Tab", Action::NewTab),
        ("New OS Tab", Action::NewContainerTab),
        ("Close Pane", Action::CloseSurface),
        ("Close Tab", Action::CloseTab),
        ("Close Window", Action::CloseWindow),
        ("Split Right", Action::NewSplit(SplitDirection::Right)),
        ("Split Left", Action::NewSplit(SplitDirection::Left)),
        ("Split Down", Action::NewSplit(SplitDirection::Down)),
        ("Zoom Split", Action::ZoomSplit),
        ("Equalize Splits", Action::EqualizeSplits),
        ("Select Split Up", Action::GotoSplit(SplitFocus::Up)),
        ("Select Split Down", Action::GotoSplit(SplitFocus::Down)),
        ("Select Split Left", Action::GotoSplit(SplitFocus::Left)),
        ("Select Split Right", Action::GotoSplit(SplitFocus::Right)),
        ("Broadcast Input", Action::ToggleBroadcast),
        ("Save Current Layout", Action::SaveLayout),
        ("Previous Tab", Action::PreviousTab),
        ("Next Tab", Action::NextTab),
        ("Copy", Action::Copy),
        ("Paste", Action::Paste),
        ("Select All", Action::SelectAll),
        ("Find", Action::ToggleSearch),
        ("Semantic Find", Action::ToggleSemanticSearch),
        ("Explain Output", Action::ExplainOutput),
        ("Compose Command", Action::ComposeCommand),
        ("Clear Screen", Action::ClearScreen),
        ("Jump to Previous Prompt", Action::JumpToPrompt(-1)),
        ("Jump to Next Prompt", Action::JumpToPrompt(1)),
        ("Increase Font Size", Action::IncreaseFontSize(1.0)),
        ("Decrease Font Size", Action::DecreaseFontSize(1.0)),
        ("Reset Font Size", Action::ResetFontSize),
        ("Change Tab Title", Action::ChangeTabTitle),
        ("Change Terminal Title", Action::ChangeTerminalTitle),
        ("Terminal Read-only", Action::ToggleReadOnly),
        ("Toggle Full Screen", Action::ToggleFullscreen),
        ("Quick Terminal", Action::ToggleQuickTerminal),
        ("Record Macro", Action::MacroRecord),
        ("Record Session (cast)", Action::ToggleRecording),
        ("Settings", Action::ToggleSettings),
        ("Documentation", Action::ShowHelp),
        ("Relay: Launch Agent", Action::RelayLaunch),
        ("Relay: Open Feed", Action::RelayFeed),
        ("Relay: View Logs", Action::RelayLog),
        ("Relay: Start Server", Action::RelayStart),
        ("Relay: Stop Server", Action::RelayStop),
        ("Relay: Restart Server", Action::RelayRestart),
        ("Quit", Action::Quit),
    ]
}

/// Load saved macros from the default macro directory (empty if unconfigured
/// or absent).
pub(crate) fn loadmacros() -> Vec<macros::Macro> {
    macros::defaultdir()
        .map(|dir| macros::load(&dir))
        .unwrap_or_default()
}

pub(crate) fn resolvekeys(
    opts: &config::Options,
    plugins: &[plugin::Plugin],
) -> (Vec<Keybind>, Vec<config::Diagnostic>) {
    let mut raw = plugin::keybinds(plugins);
    raw.extend(opts.keybind.iter().cloned());
    let (mut binds, diags) = config::resolve(&raw);
    for action in [Action::NewSplit(SplitDirection::Left)] {
        if !binds.iter().any(|b| b.action == action) {
            binds.push(Keybind {
                mods: config::Mods::default(),
                key: String::new(),
                tail: Vec::new(),
                action,
            });
        }
    }
    (binds, diags)
}

pub(crate) fn commandspawn(opts: &config::Options, command: &str) -> pty::SpawnOptions {
    let shell = opts
        .shell
        .as_ref()
        .and_then(|shell| shell.split_whitespace().next())
        .filter(|shell| !shell.is_empty())
        .map(str::to_string)
        .unwrap_or_else(pty::default_shell);
    pty::SpawnOptions::command(vec![
        shell,
        "-i".to_string(),
        "-l".to_string(),
        "-c".to_string(),
        command.to_string(),
    ])
}
