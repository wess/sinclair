//! Plugin `[[trigger]]` dispatch. When a pane emits a [`ViewEvent::Trigger`],
//! the workspace matches it against every loaded trigger (event name + optional
//! `when` filter) and runs the action: a shell command (background or a
//! pane/tab/split), a desktop notification, or a call into the plugin runtime.

use super::*;

use crate::view::TriggerEvent;

impl WorkspaceView {
    /// Every loaded plugin trigger that matches `ev`, snapshotted so running the
    /// actions (which mutate the workspace) doesn't hold a borrow of the plugins.
    fn matched_triggers(&self, ev: &TriggerEvent) -> Vec<(plugin::Plugin, plugin::Trigger)> {
        self.plugins
            .iter()
            .flat_map(|p| {
                p.triggers
                    .iter()
                    .filter(|t| trigger_matches(t, ev))
                    .map(move |t| (p.clone(), t.clone()))
            })
            .collect()
    }

    /// Run every plugin trigger that matches `ev` (fired by `item`).
    pub(crate) fn fire_triggers(
        &mut self,
        item: ItemId,
        ev: &TriggerEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let matched = self.matched_triggers(ev);
        if matched.is_empty() {
            return;
        }
        let cwd = self
            .items
            .borrow()
            .get(&item)
            .and_then(|it| it.content.cwd_path(cx));
        for (plugin, trigger) in matched {
            self.run_trigger(&plugin, &trigger.action, ev, cwd.as_deref(), window, cx);
        }
    }

    /// Run every plugin trigger matching a workspace-level event `ev` (one not
    /// tied to a pane's own output — e.g. a worktree was created), passing `cwd`
    /// as the trigger's working directory.
    pub(crate) fn fire_workspace_trigger(
        &mut self,
        ev: &TriggerEvent,
        cwd: Option<&std::path::Path>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for (plugin, trigger) in self.matched_triggers(ev) {
            self.run_trigger(&plugin, &trigger.action, ev, cwd, window, cx);
        }
    }

    fn run_trigger(
        &mut self,
        plugin: &plugin::Plugin,
        action: &plugin::TriggerAction,
        ev: &TriggerEvent,
        cwd: Option<&std::path::Path>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            plugin::TriggerAction::Run { text, target } => match target {
                plugin::TriggerTarget::Background => run_background(text, cwd),
                other => {
                    let _ = self.mcp_dispatch(
                        "run_command",
                        &json!({ "text": text, "target": run_target(*other) }),
                        window,
                        cx,
                    );
                }
            },
            plugin::TriggerAction::Notify { text } => {
                crate::view::post_os_notification(&plugin.name, text);
            }
            plugin::TriggerAction::Invoke { method } => {
                self.invoke_trigger_tool(plugin, method, ev, window, cx);
            }
        }
    }

    /// Deliver an `invoke` trigger to the plugin's tool of that name, with the
    /// event payload as its parameters.
    ///
    /// A trigger calls a **tool** rather than some separate event entry point:
    /// tools are already the plugin's one callable surface (the palette and
    /// agents reach the same functions), so an author writes one handler instead
    /// of two, and no extra guest export has to exist for events.
    ///
    /// The call runs in-process on the resident instance, so unlike the old
    /// subprocess round-trip there is nothing to move off the UI thread. A
    /// plugin that wants the terminal to do something calls `run-command`
    /// itself, which is queued and drained here.
    fn invoke_trigger_tool(
        &mut self,
        plugin: &plugin::Plugin,
        method: &str,
        ev: &TriggerEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if plugin.runtime.is_none() {
            return;
        }
        let params = ev.payload().to_string();
        let cwd = self.focused_cwd(cx);
        let clipboard = cx.read_from_clipboard().and_then(|i| i.text());
        let result = match self.ensure_gui_wasm() {
            Some(gw) => {
                gw.set_context(cwd, clipboard);
                gw.call_tool(plugin, method, &params)
            }
            None => Err("wasm runtime unavailable".to_string()),
        };
        if let Err(e) = result {
            eprintln!(
                "sinclair: trigger `{method}` on plugin `{}` failed: {e}",
                plugin.id
            );
        }
        self.apply_wasm_clipboard(cx);
        self.drain_wasm_commands(window, cx);
    }
}

/// Does `trigger` fire for `ev`? Matches the event name, then the optional
/// `when` filter (exit-code class for exit/command-finished, substring for
/// title/notify/dir; ignored for bell).
fn trigger_matches(trigger: &plugin::Trigger, ev: &TriggerEvent) -> bool {
    if trigger.on != ev.name() {
        return false;
    }
    let Some(when) = trigger.when.as_deref() else {
        return true;
    };
    if let Some(code) = ev.exit_code() {
        // code: None == killed by signal (treated as failure).
        let success = code == Some(0);
        return match when {
            "any" | "" => true,
            "zero" | "success" | "ok" => success,
            "nonzero" | "failure" | "fail" | "error" => !success,
            _ => false,
        };
    }
    match ev.match_text() {
        Some(text) => text.contains(when),
        None => true, // e.g. bell: no text to filter on
    }
}

/// The `run_command` target token for a non-background trigger target.
fn run_target(target: plugin::TriggerTarget) -> &'static str {
    match target {
        plugin::TriggerTarget::Tab => "tab",
        plugin::TriggerTarget::SplitRight => "split_right",
        plugin::TriggerTarget::SplitDown => "split_down",
        // Pane is the default; Background is handled before this is called.
        _ => "pane",
    }
}

/// Spawn a detached background command in `cwd`, reaping it on a helper thread
/// so it can't become a zombie or block the UI.
fn run_background(text: &str, cwd: Option<&std::path::Path>) {
    let text = text.to_string();
    let cwd = cwd.map(std::path::Path::to_path_buf);
    std::thread::spawn(move || {
        let mut cmd = std::process::Command::new("/bin/sh");
        cmd.arg("-c").arg(&text);
        if let Some(dir) = cwd {
            cmd.current_dir(dir);
        }
        let _ = cmd.status();
    });
}
