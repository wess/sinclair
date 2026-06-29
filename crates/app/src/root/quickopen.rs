//! The cmd+P quick-open: a guise `Spotlight` over the workspace's actions,
//! plugin commands, and plugin panels. Rebuilt on each open so newly installed
//! plugins appear, then rendered as an in-window overlay (see `render.rs`).

use super::*;

impl WorkspaceView {
    /// Build and open the Spotlight quick-open overlay.
    pub(crate) fn open_quickopen(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let view = cx.entity();

        // (label, optional shortcut hint, action) for everything launchable.
        let mut items: Vec<(String, Option<String>, Action)> = Vec::new();
        for (label, action) in palette_catalog() {
            let hint = self
                .keybinds
                .iter()
                .find(|k| k.action == action)
                .and_then(keys::shortcut_glyphs_seq);
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
        }
        items.push((
            "Plugins: browse & install".to_string(),
            None,
            Action::Sidebar("right:plugins".to_string()),
        ));

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
}
