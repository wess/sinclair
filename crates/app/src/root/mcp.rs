use super::*;
use gpui::prelude::*;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Monotonic counter for naming persistent container tabs uniquely within a
/// run (`sinclair-<os>-<n>`).
static CONTAINER_SEQ: AtomicUsize = AtomicUsize::new(0);

/// Lowercase a profile label into a container-name-safe slug: alphanumerics
/// kept, every other run collapsed to a single `-`, edges trimmed.
fn slug(label: &str) -> String {
    let mut out = String::with_capacity(label.len());
    let mut dash = false;
    for ch in label.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            dash = false;
        } else if !out.is_empty() && !dash {
            out.push('-');
            dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        "os".to_string()
    } else {
        out
    }
}

impl WorkspaceView {
    /// Handle one MCP bridge op against this workspace, returning the JSON
    /// result body (or an error string the bridge reports as a failed tool
    /// call). The op names mirror the MCP tools defined in `mcpbridge`.
    pub fn mcp_dispatch(
        &mut self,
        op: &str,
        args: &Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Value, String> {
        match op {
            "run_command" => {
                let text = args
                    .get("text")
                    .and_then(Value::as_str)
                    .ok_or("run_command requires a `text` string")?;
                let target = args.get("target").and_then(Value::as_str).unwrap_or("pane");
                self.mcp_run(text, target, window, cx)
            }
            "read_screen" => {
                let lines = args.get("lines").and_then(Value::as_u64).map(|n| n as usize);
                let text = self
                    .focused_terminal(cx)
                    .map(|v| v.read(cx).screen_text(lines))
                    .unwrap_or_default();
                Ok(json!({ "text": text }))
            }
            #[cfg(debug_assertions)]
            "simulate_update" => {
                let version = args.get("version").and_then(Value::as_str).unwrap_or("9.9.9");
                let rel = updater::Release {
                    version: version.to_string(),
                    url: "https://github.com/wess/sinclair/releases".to_string(),
                    assets: Vec::new(),
                };
                crate::updateui::open(rel, cx);
                Ok(json!({ "opened": true }))
            }
            #[cfg(debug_assertions)]
            "update_probe" => {
                let install = format!("{:?}", updater::detect());
                let (available, latest, err) = match updater::check(crate::updateui::current()) {
                    Ok(Some(r)) => (true, r.version, String::new()),
                    Ok(None) => (false, String::new(), String::new()),
                    Err(e) => (false, String::new(), e),
                };
                Ok(json!({
                    "current": crate::updateui::current(),
                    "install": install,
                    "available": available,
                    "latest": latest,
                    "error": err,
                }))
            }
            #[cfg(debug_assertions)]
            "window_bounds" => {
                let b = window.bounds();
                Ok(json!({
                    "x": f32::from(b.origin.x),
                    "y": f32::from(b.origin.y),
                    "width": f32::from(b.size.width),
                    "height": f32::from(b.size.height),
                }))
            }
            #[cfg(debug_assertions)]
            "suggest_key" => {
                let key = args
                    .get("key")
                    .and_then(Value::as_str)
                    .ok_or("suggest_key requires a `key` string")?
                    .to_string();
                let mut consumed = false;
                self.onfocused(cx, |v, cx| consumed = v.debug_suggest_key(&key, cx));
                Ok(json!({ "consumed": consumed }))
            }
            #[cfg(debug_assertions)]
            "read_suggestion" => {
                let (input, ghost, candidates, popup_open) = self
                    .focused_terminal(cx)
                    .map(|v| v.read(cx).suggestion_debug())
                    .unwrap_or_default();
                Ok(json!({
                    "input": input,
                    "ghost": ghost,
                    "candidates": candidates,
                    "popup_open": popup_open,
                }))
            }
            "send_input" => {
                let text = args
                    .get("text")
                    .and_then(Value::as_str)
                    .ok_or("send_input requires a `text` string")?
                    .to_string();
                self.onfocused(cx, |v, cx| v.send_text(text.as_bytes(), cx));
                Ok(json!({ "ok": true }))
            }
            "new_tab" => {
                self.newtab(window, cx);
                Ok(json!({ "ok": true }))
            }
            "split" => {
                let dir = args
                    .get("direction")
                    .and_then(Value::as_str)
                    .ok_or("split requires a `direction` of right or down")?;
                let axis = match dir {
                    "right" => SplitAxis::Horizontal,
                    "down" => SplitAxis::Vertical,
                    other => return Err(format!("unknown split direction `{other}`")),
                };
                self.split(axis, false, window, cx);
                Ok(json!({ "ok": true }))
            }
            "list_panes" => {
                // Real pane structure (list_tabs already flattens items):
                // one entry per pane in traversal order, items nested.
                let group = self.group.read(cx);
                let focused_pane = group.focused_pane();
                let panes = group
                    .panes_with_items()
                    .into_iter()
                    .enumerate()
                    .map(|(index, (pane, item_ids, active))| {
                        let items = self.items.borrow();
                        let entries = item_ids
                            .iter()
                            .map(|id| {
                                let it = items.get(id);
                                json!({
                                    "title": it.map(|p| p.content.title(cx)).unwrap_or_default(),
                                    "cwd": it
                                        .and_then(|p| p.content.cwd_path(cx))
                                        .map(|p| p.to_string_lossy().into_owned()),
                                    "active": *id == active,
                                })
                            })
                            .collect::<Vec<_>>();
                        json!({
                            "index": index,
                            "focused": pane == focused_pane,
                            "items": entries,
                        })
                    })
                    .collect::<Vec<_>>();
                Ok(json!({ "panes": panes }))
            }
            "list_macros" => Ok(json!({
                "macros": self
                    .macros
                    .iter()
                    .map(|m| json!({ "name": m.name, "commands": m.commands }))
                    .collect::<Vec<_>>(),
            })),
            "run_macro" => {
                let name = args
                    .get("name")
                    .and_then(Value::as_str)
                    .ok_or("run_macro requires a `name` string")?;
                self.replay_macro(name, cx)?;
                Ok(json!({ "ok": true, "name": name }))
            }
            "list_tabs" => {
                let items = self.group.read(cx).items();
                let active_item = self.group.read(cx).active_item();
                let active = items.iter().position(|i| *i == active_item).unwrap_or(0);
                let tabs = self
                    .titles(cx)
                    .into_iter()
                    .enumerate()
                    .map(|(index, title)| json!({ "index": index, "title": title, "active": index == active }))
                    .collect::<Vec<_>>();
                Ok(json!({ "tabs": tabs, "active": active }))
            }
            "focus_tab" => {
                let index = args
                    .get("index")
                    .and_then(Value::as_u64)
                    .ok_or("focus_tab requires an `index` number")? as usize;
                let items = self.group.read(cx).items();
                let Some(&item) = items.get(index) else {
                    return Err(format!("no tab at index {index}"));
                };
                self.activate_item(item, window, cx);
                Ok(json!({ "ok": true, "index": index }))
            }
            "agent_states" => Ok(self.agent_states(cx)),
            "worktree_create" => {
                let path = args
                    .get("path")
                    .and_then(Value::as_str)
                    .ok_or("worktree_create requires a `path` string")?;
                let branch = args.get("branch").and_then(Value::as_str);
                let spec = match branch {
                    Some(b) if !b.is_empty() => format!("{path}@{b}"),
                    _ => path.to_string(),
                };
                let abs = self.worktree_create(&spec, window, cx)?;
                Ok(json!({ "ok": true, "path": abs.to_string_lossy() }))
            }
            "worktree_open" => {
                let path = args
                    .get("path")
                    .and_then(Value::as_str)
                    .ok_or("worktree_open requires a `path` string")?;
                let abs = self.worktree_open(path, window, cx)?;
                Ok(json!({ "ok": true, "path": abs.to_string_lossy() }))
            }
            "worktree_remove" => {
                let path = args
                    .get("path")
                    .and_then(Value::as_str)
                    .ok_or("worktree_remove requires a `path` string")?;
                let abs = self.worktree_remove(path, window, cx)?;
                Ok(json!({ "ok": true, "path": abs.to_string_lossy() }))
            }
            "worktree_list" => self.worktree_list(cx),
            other => Err(format!("unknown op `{other}`")),
        }
    }

    /// Run `text` per the MCP `run_command` target.
    fn mcp_run(
        &mut self,
        text: &str,
        target: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Value, String> {
        match target {
            "pane" => self.onfocused(cx, |view, cx| view.run_command(text, cx)),
            "tab" => {
                let id = self
                    .spawncommand(text, window, cx)
                    .ok_or("failed to spawn command tab")?;
                self.group.update(cx, |g, cx| g.add_to_focused(id, cx));
                self.focusactive(window, cx);
                cx.notify();
            }
            "split_right" => self.splitcommand(text, SplitAxis::Horizontal, false, window, cx),
            "split_down" => self.splitcommand(text, SplitAxis::Vertical, false, window, cx),
            other => {
                return Err(format!(
                    "unknown target `{other}` (pane|tab|split_right|split_down)"
                ))
            }
        }
        Ok(json!({ "ok": true, "target": target }))
    }

    pub(crate) fn spawncommand(
        &mut self,
        command: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<ItemId> {
        let inherit = self.focused_cwd_path(cx);
        let mut options = session::options(&self.opts, SPAWN_COLS, SPAWN_ROWS, inherit);
        let cwd = options.spawn.cwd.clone();
        options.spawn = commandspawn(&self.opts, command);
        options.spawn.cwd = cwd;
        let id = self.spawn(options, window, cx)?;
        // Remember the command so a restored session can relaunch (and, for
        // agents that reported a native session id, resume) this pane.
        self.set_item_command(id, command);
        Some(id)
    }

    /// Launch `profile` as a container-backed tab. Resolves the engine, builds
    /// the run target (honoring the global/per-profile lifecycle), spawns it,
    /// and labels the tab with the OS name.
    pub(crate) fn launch_container(
        &mut self,
        profile: &container::Profile,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(engine) = container::Engine::resolve(self.opts.container_engine.as_deref()) else {
            eprintln!("sinclair: no container engine available (install Docker or Podman)");
            return;
        };
        let n = CONTAINER_SEQ.fetch_add(1, Ordering::Relaxed) + 1;
        let name = Some(format!("prompt-{}-{n}", slug(&profile.label)));
        let target =
            container::Target::from_profile(engine, profile, self.opts.container_persist, name);
        if let Some(id) = self.spawn_container(&target, window, cx) {
            // Ephemeral containers are force-removed when their item closes.
            if !target.persist {
                if let Some(name) = &target.name {
                    self.kill_on_close.insert(id, name.clone());
                }
            }
            self.group.update(cx, |g, cx| g.add_to_focused(id, cx));
            self.rename_item(id, &profile.label, cx);
            self.focusactive(window, cx);
            cx.notify();
        }
    }

    /// Spawn an item whose backing process is the container `target`'s `engine
    /// run …` argv. The argv is run directly (no shell wrapper) via
    /// [`Self::spawn_tab_argv`], inheriting the focused item's cwd.
    pub(crate) fn spawn_container(
        &mut self,
        target: &container::Target,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<ItemId> {
        self.spawn_tab_argv(target.argv(), window, cx)
    }

    /// Split the focused pane and run `command` in the new pane.
    pub(crate) fn splitcommand(
        &mut self,
        command: &str,
        axis: SplitAxis,
        first: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(id) = self.spawncommand(command, window, cx) else {
            return;
        };
        let pane = self.group.read(cx).focused_pane();
        self.group.update(cx, |g, cx| {
            g.split(pane, axis, first, id, cx);
        });
        self.focusactive(window, cx);
        cx.notify();
    }

    /// Spawn an item running `command` (or a plain shell when `None`).
    pub(crate) fn spawn_pane(
        &mut self,
        command: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<ItemId> {
        match command {
            Some(c) => self.spawncommand(c, window, cx),
            None => {
                let inherit = self.focused_cwd_path(cx);
                let options = session::options(&self.opts, SPAWN_COLS, SPAWN_ROWS, inherit);
                self.spawn(options, window, cx)
            }
        }
    }

    /// Realize `layout` as a split arrangement: the root item becomes a new tab
    /// in the focused pane, then that pane is split to build the rest.
    /// `commands[i]` is the command for leaf `i` in pre-order (`None` = shell).
    pub(crate) fn apply_layout(
        &mut self,
        layout: &crate::tiles::Layout,
        commands: &[Option<String>],
        title: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let first = commands.first().and_then(|c| c.as_deref());
        let Some(root) = self.spawn_pane(first, window, cx) else {
            return;
        };
        self.group.update(cx, |g, cx| g.add_to_focused(root, cx));
        let host = self.group.read(cx).focused_pane();
        self.realize_into(layout, host, 0, commands, window, cx);
        if let Some(t) = title {
            self.rename_item(root, t, cx);
        }
        self.activate_item(root, window, cx);
        cx.notify();
    }

    /// Spawn an item rooted at `cwd` (or the configured default when `None`).
    pub(crate) fn spawn_cwd(
        &mut self,
        cwd: Option<std::path::PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<ItemId> {
        let options = session::options(&self.opts, SPAWN_COLS, SPAWN_ROWS, cwd);
        self.spawn(options, window, cx)
    }
}
