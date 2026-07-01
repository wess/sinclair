//! Plugin `[[trigger]]` dispatch. When a pane emits a [`ViewEvent::Trigger`],
//! the workspace matches it against every loaded trigger (event name + optional
//! `when` filter) and runs the action: a shell command (background or a
//! pane/tab/split), a desktop notification, or a call into the plugin runtime.

use super::*;

use crate::view::TriggerEvent;

impl WorkspaceView {
    /// Run every plugin trigger that matches `ev` (fired by `pane`).
    pub(crate) fn fire_triggers(
        &mut self,
        pane: PaneId,
        ev: &TriggerEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Snapshot matches up front so we don't hold a borrow of `self.plugins`
        // while running actions (which mutate the workspace).
        let matched: Vec<(plugin::Plugin, plugin::Trigger)> = self
            .plugins
            .iter()
            .flat_map(|p| {
                p.triggers
                    .iter()
                    .filter(|t| trigger_matches(t, ev))
                    .map(move |t| (p.clone(), t.clone()))
            })
            .collect();
        if matched.is_empty() {
            return;
        }
        let cwd = self
            .panes
            .get(&pane)
            .and_then(|p| p.content.cwd_path(cx));
        for (plugin, trigger) in matched {
            self.run_trigger(&plugin, &trigger.action, ev, cwd.as_deref(), window, cx);
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
                self.invoke_trigger_runtime(plugin.clone(), method.clone(), ev, cwd, window, cx);
            }
        }
    }

    /// Call the plugin's `[runtime]` with the event payload, off the UI thread,
    /// then run any `run` directives it returns (like a panel action).
    fn invoke_trigger_runtime(
        &mut self,
        plugin: plugin::Plugin,
        method: String,
        ev: &TriggerEvent,
        cwd: Option<&std::path::Path>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if plugin.runtime.is_none() {
            return;
        }
        let Some(handle) = window.window_handle().downcast::<WorkspaceView>() else {
            return;
        };
        let params = ev.payload();
        let cwd = cwd.map(|p| p.to_string_lossy().into_owned());
        let panel = plugin
            .webview
            .as_ref()
            .map(|w| w.id.clone())
            .or_else(|| plugin.panel.as_ref().map(|p| p.id.clone()))
            .unwrap_or_else(|| plugin.id.clone());
        let executor = cx.background_executor().clone();
        cx.spawn(async move |_this, cx| {
            let resp = executor
                .spawn(async move {
                    let req = crate::pluginhost::Request {
                        kind: "message",
                        panel: &panel,
                        action: None,
                        cwd: cwd.as_deref(),
                        method: Some(&method),
                        params: Some(&params),
                    };
                    crate::pluginhost::invoke(&plugin, &req)
                })
                .await;
            let Ok(resp) = resp else {
                return;
            };
            let _ = handle.update(cx, |view, window, cx| {
                for run in &resp.run {
                    let target = run.target.as_deref().unwrap_or("pane");
                    let _ = view.mcp_dispatch(
                        "run_command",
                        &json!({ "text": run.text, "target": target }),
                        window,
                        cx,
                    );
                }
            });
        })
        .detach();
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
