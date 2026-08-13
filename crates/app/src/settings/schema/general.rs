//! General section: shell, windows, clipboard, and OS-tab containers.

use super::{choice, list, opt, slider, strs, text, toggle, ListKind, Section, Setting};

pub(super) fn settings() -> Vec<Setting> {
    let s = Section::General;
    vec![
        text(
            "command",
            "Shell",
            "Command run in new panes instead of your login shell.",
            s,
            |o| opt(&o.shell),
            "Login shell",
        ),
        text(
            "working-directory",
            "Working directory",
            "Directory new windows start in.",
            s,
            |o| opt(&o.working_directory),
            "Inherit",
        ),
        text(
            "title",
            "Window title",
            "Fixed window title, overriding what the shell reports.",
            s,
            |o| opt(&o.title),
            "Default",
        ),
        toggle(
            "window-inherit-working-directory",
            "Inherit working directory",
            "New tabs and splits start in the focused pane's directory.",
            s,
            |o| o.window_inherit_working_directory,
        ),
        toggle(
            "quit-after-last-window-closed",
            "Quit after last window",
            "Exit Sinclair when the last window closes.",
            s,
            |o| o.quit_after_last_window_closed,
        ),
        toggle(
            "confirm-close-surface",
            "Confirm before closing",
            "Ask before closing a pane with a running process.",
            s,
            |o| o.confirm_close_surface,
        ),
        toggle(
            "confirm-quit",
            "Confirm quit while running",
            "Warn before quitting while a process is still running.",
            s,
            |o| o.confirm_quit,
        ),
        toggle(
            "clipboard-paste-protection",
            "Confirm risky paste",
            "Ask before a paste that looks dangerous reaches the shell.",
            s,
            |o| o.clipboard_paste_protection,
        ),
        toggle(
            "shell-integration",
            "Shell integration",
            "Inject prompt marks and cwd reporting into zsh, bash, and fish.",
            s,
            |o| o.shell_integration,
        ),
        toggle(
            "auto-update",
            "Check for updates automatically",
            "Check GitHub for a newer release on launch and hourly.",
            s,
            |o| o.auto_update,
        ),
        toggle(
            "session-restore",
            "Restore session on launch",
            "Save tabs, splits, and directories on quit and reopen them.",
            s,
            |o| o.session_restore,
        ),
        toggle(
            "tab-title-show-host",
            "Show user@host in tab title",
            "Keep the leading user@host: shells put in the terminal title.",
            s,
            |o| o.tab_title_show_host,
        ),
        toggle(
            "tab-peek",
            "Tab peek",
            "Let a strip of live tab previews drop under the tab bar.",
            s,
            |o| o.tab_peek,
        ),
        slider(
            "tab-peek-hover",
            "Open peek on hover after",
            "Milliseconds resting on the tab bar before the peek drops; 0 is off.",
            s,
            |o| o.tab_peek_hover as f32,
            (0.0, 2000.0, 50.0),
            true,
        ),
        slider(
            "tab-peek-height",
            "Peek card height",
            "How tall each live tab preview is drawn, in pixels.",
            s,
            |o| o.tab_peek_height as f32,
            (60.0, 400.0, 10.0),
            true,
        ),
        toggle(
            "copy-on-select",
            "Copy on select",
            "Selecting text copies it to the clipboard immediately.",
            s,
            |o| o.copy_on_select,
        ),
        choice(
            "macos-option-as-alt",
            "Option as Alt",
            "Whether the macOS Option key sends Alt or composes characters.",
            s,
            |o| {
                match o.macos_option_as_alt {
                    config::OptionAsAlt::Auto => "auto",
                    config::OptionAsAlt::False => "false",
                    config::OptionAsAlt::True => "true",
                    config::OptionAsAlt::Left => "left",
                    config::OptionAsAlt::Right => "right",
                }
                .to_string()
            },
            || strs(&["auto", "false", "true", "left", "right"]),
            None,
        ),
        choice(
            "clipboard-read",
            "Clipboard read",
            "Whether programs in the terminal may read the clipboard.",
            s,
            |o| clip(o.clipboard_read),
            || strs(&["allow", "ask", "deny"]),
            None,
        ),
        choice(
            "clipboard-write",
            "Clipboard write",
            "Whether programs in the terminal may write the clipboard.",
            s,
            |o| clip(o.clipboard_write),
            || strs(&["allow", "ask", "deny"]),
            None,
        ),
        choice(
            "container-engine",
            "Container engine",
            "Engine for OS tabs; auto detects whichever is installed.",
            s,
            |o| o.container_engine.clone().unwrap_or_default(),
            || strs(&["docker", "podman"]),
            Some("auto"),
        ),
        toggle(
            "container-persist",
            "Keep containers after close",
            "Keep an OS tab's container instead of removing it on close.",
            s,
            |o| o.container_persist,
        ),
        list(
            ListKind::Container,
            "OS profiles for the OS Tabs picker: label | image | command | lifecycle.",
            s,
        ),
    ]
}

fn clip(a: config::ClipboardAccess) -> String {
    match a {
        config::ClipboardAccess::Allow => "allow",
        config::ClipboardAccess::Ask => "ask",
        config::ClipboardAccess::Deny => "deny",
    }
    .to_string()
}
