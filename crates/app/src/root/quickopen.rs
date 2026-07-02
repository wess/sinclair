//! The cmd+P quick-open: a guise `Spotlight` over the workspace's actions,
//! plugin commands, and plugin panels. Rebuilt on each open so newly installed
//! plugins appear, then rendered as an in-window overlay (see `render.rs`).

use super::*;

impl WorkspaceView {
    /// Open a Spotlight overlay over the given launchable entries —
    /// `(label, optional shortcut hint, action)`. Shared by the command
    /// palette and quick-open.
    fn open_spotlight(
        &mut self,
        items: Vec<(String, Option<String>, Action)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = cx.entity();
        let spotlight = cx.new(|scx| {
            let mut spot = guise::Spotlight::new(scx);
            for (label, hint, action) in items {
                let view = view.clone();
                let run = move |window: &mut Window, app: &mut App| {
                    let action = action.clone();
                    view.update(app, |this, cx| this.run_action(action, window, cx));
                };
                spot = match hint {
                    Some(hint) => spot.item_hint(label, hint, run),
                    None => spot.item(label, run),
                };
            }
            spot
        });
        spotlight.update(cx, |spot, scx| spot.open(window, scx));
        self.spotlight = Some(spotlight);
        cx.notify();
    }

    /// Open a Spotlight over the clipboard history; picking an entry pastes it
    /// into the focused pane.
    pub(crate) fn open_clipboard_history(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let entries = crate::clipboard::entries(cx);
        if entries.is_empty() {
            return;
        }
        let view = cx.entity();
        let spotlight = cx.new(|scx| {
            let mut spot = guise::Spotlight::new(scx);
            for text in entries {
                let view = view.clone();
                let paste = text.clone();
                let run = move |_window: &mut Window, app: &mut App| {
                    let paste = paste.clone();
                    view.update(app, |this, cx| {
                        this.onfocused(cx, |v, cx| v.write_paste(&paste, cx))
                    });
                };
                spot = spot.item(clip_label(&text), run);
            }
            spot
        });
        spotlight.update(cx, |spot, scx| spot.open(window, scx));
        self.spotlight = Some(spotlight);
        cx.notify();
    }

    /// Open a Spotlight over the configured snippets (`snippet = label | cmd`);
    /// picking one inserts the command into the focused pane (not run — the user
    /// can edit before pressing Enter). Warp-style workflows.
    pub(crate) fn open_snippets(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let snippets: Vec<(String, String)> = self
            .opts
            .snippet
            .iter()
            .filter(|s| !s.trim().is_empty())
            .map(|s| match s.split_once('|') {
                Some((label, cmd)) => (label.trim().to_string(), cmd.trim().to_string()),
                None => (s.trim().to_string(), s.trim().to_string()),
            })
            .collect();
        if snippets.is_empty() {
            return;
        }
        let view = cx.entity();
        let spotlight = cx.new(|scx| {
            let mut spot = guise::Spotlight::new(scx);
            for (label, cmd) in snippets {
                let view = view.clone();
                let cmd = cmd.clone();
                let run = move |_window: &mut Window, app: &mut App| {
                    let cmd = cmd.clone();
                    view.update(app, |this, cx| {
                        this.onfocused(cx, |v, cx| v.write_paste(&cmd, cx))
                    });
                };
                spot = spot.item(label, run);
            }
            spot
        });
        spotlight.update(cx, |spot, scx| spot.open(window, scx));
        self.spotlight = Some(spotlight);
        cx.notify();
    }

    /// Open a Spotlight over a curated emoji/symbol set; picking one sends the
    /// glyph to the focused pane.
    pub(crate) fn open_unicode_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let view = cx.entity();
        let spotlight = cx.new(|scx| {
            let mut spot = guise::Spotlight::new(scx);
            for (glyph, name) in SYMBOLS {
                let view = view.clone();
                let bytes = glyph.as_bytes().to_vec();
                let run = move |_window: &mut Window, app: &mut App| {
                    let bytes = bytes.clone();
                    view.update(app, |this, cx| {
                        this.onfocused(cx, |v, cx| v.send_text(&bytes, cx))
                    });
                };
                spot = spot.item(format!("{glyph}  {name}"), run);
            }
            spot
        });
        spotlight.update(cx, |spot, scx| spot.open(window, scx));
        self.spotlight = Some(spotlight);
        cx.notify();
    }

    /// The keybind hint for an action, if one is bound.
    fn shortcut_hint(&self, action: &Action) -> Option<String> {
        self.keybinds
            .iter()
            .find(|k| &k.action == action)
            .and_then(keys::shortcut_glyphs_seq)
    }

    /// Open the command palette: a Spotlight over the curated action catalog,
    /// each entry tagged with its current keybind.
    pub(crate) fn open_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let items = palette_catalog()
            .into_iter()
            .map(|(label, action)| {
                let hint = self.shortcut_hint(&action);
                (label.to_string(), hint, action)
            })
            .collect();
        self.open_spotlight(items, window, cx);
    }

    /// Build and open the Spotlight quick-open overlay: the action catalog
    /// plus every plugin command and panel.
    pub(crate) fn open_quickopen(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut items: Vec<(String, Option<String>, Action)> = Vec::new();
        for (label, action) in palette_catalog() {
            let hint = self.shortcut_hint(&action);
            items.push((label.to_string(), hint, action));
        }
        for plugin in &self.plugins {
            for command in &plugin.commands {
                items.push((
                    format!("{}: {}", plugin.name, command.title),
                    None,
                    Action::PluginCommand(plugin::actionid(&plugin.id, &command.id)),
                ));
            }
            if let Some(panel) = plugin.panel.as_ref() {
                items.push((
                    format!("Open {} panel", plugin.name),
                    None,
                    Action::Sidebar(format!("right:plugin:{}", panel.id)),
                ));
            }
            if let Some(webview) = plugin.webview.as_ref() {
                items.push((
                    format!("Open {}", webview.title),
                    None,
                    Action::OpenWebview(webview.id.clone()),
                ));
            }
        }
        items.push((
            "Manage Plugins".to_string(),
            None,
            Action::ManagePlugins,
        ));
        self.open_spotlight(items, window, cx);
    }
}

/// A one-line, length-capped label for a clipboard entry.
fn clip_label(text: &str) -> String {
    let one = text.replace('\n', " \u{23ce} ");
    if one.chars().count() > 80 {
        format!("{}\u{2026}", one.chars().take(79).collect::<String>())
    } else {
        one
    }
}

/// A curated set of emoji and symbols for the insert picker: `(glyph, name)`.
/// Names make entries fuzzy-searchable in the Spotlight.
const SYMBOLS: &[(&str, &str)] = &[
    ("\u{1f600}", "grinning face"), ("\u{1f602}", "joy laughing tears"),
    ("\u{1f605}", "sweat smile"), ("\u{1f609}", "wink"), ("\u{1f60e}", "sunglasses cool"),
    ("\u{1f914}", "thinking"), ("\u{1f644}", "eye roll"), ("\u{1f480}", "skull"),
    ("\u{1f525}", "fire"), ("\u{2728}", "sparkles"), ("\u{1f389}", "party tada"),
    ("\u{1f680}", "rocket ship launch"), ("\u{1f4a1}", "idea bulb"), ("\u{1f41b}", "bug"),
    ("\u{2705}", "check mark done"), ("\u{274c}", "cross fail x"), ("\u{26a0}\u{fe0f}", "warning"),
    ("\u{1f6d1}", "stop"), ("\u{1f44d}", "thumbs up"), ("\u{1f44e}", "thumbs down"),
    ("\u{1f440}", "eyes looking"), ("\u{1f64f}", "pray thanks"), ("\u{1f4af}", "hundred perfect"),
    ("\u{2764}\u{fe0f}", "heart red"), ("\u{1f9e0}", "brain"), ("\u{1f916}", "robot"),
    ("\u{1f9d1}\u{200d}\u{1f4bb}", "developer"), ("\u{1f4c1}", "folder"), ("\u{1f4c4}", "document page"),
    ("\u{1f4cc}", "pin"), ("\u{1f4dd}", "memo note"), ("\u{1f512}", "lock secure"),
    ("\u{1f513}", "unlock"), ("\u{1f511}", "key"), ("\u{2699}\u{fe0f}", "gear settings"),
    ("\u{1f527}", "wrench tool"), ("\u{1f4e6}", "package box"), ("\u{1f550}", "clock time"),
    ("\u{2b50}", "star"), ("\u{1f30d}", "globe earth"), ("\u{1f4ca}", "chart bar stats"),
    ("\u{1f4c8}", "chart up trend"), ("\u{1f4c9}", "chart down"), ("\u{1f6a7}", "construction wip"),
    ("\u{2192}", "arrow right"), ("\u{2190}", "arrow left"), ("\u{2191}", "arrow up"),
    ("\u{2193}", "arrow down"), ("\u{21b5}", "return enter"), ("\u{2713}", "check tick"),
    ("\u{2717}", "ballot x"), ("\u{2026}", "ellipsis dots"), ("\u{2022}", "bullet dot"),
    ("\u{2018}", "left single quote"), ("\u{2019}", "right single quote apostrophe"),
    ("\u{201c}", "left double quote"), ("\u{201d}", "right double quote"),
    ("\u{2013}", "en dash"), ("\u{2014}", "em dash"), ("\u{00b0}", "degree"),
    ("\u{03bb}", "lambda greek"), ("\u{03c0}", "pi greek"), ("\u{221e}", "infinity"),
    ("\u{2211}", "sum sigma"), ("\u{2260}", "not equal"), ("\u{2264}", "less equal"),
    ("\u{2265}", "greater equal"), ("\u{00b1}", "plus minus"),
];
