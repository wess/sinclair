use super::*;
use gpui::prelude::*;

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
                    .panes
                    .get(&self.tabs.focused())
                    .map(|pane| pane.view.read(cx).screen_text(lines))
                    .unwrap_or_default();
                Ok(json!({ "text": text }))
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
                Ok(json!({ "ok": true, "index": self.tabs.active_index() }))
            }
            "split" => {
                let dir = args
                    .get("direction")
                    .and_then(Value::as_str)
                    .ok_or("split requires a `direction` of right or down")?;
                let axis = match dir {
                    "right" => Axis::Horizontal,
                    "down" => Axis::Vertical,
                    other => return Err(format!("unknown split direction `{other}`")),
                };
                self.split(axis, false, window, cx);
                Ok(json!({ "ok": true }))
            }
            "list_panes" => {
                let focused = self.tabs.focused();
                let panes = self
                    .tabs
                    .active()
                    .tree
                    .panes()
                    .into_iter()
                    .map(|id| {
                        let view = self.panes.get(&id).map(|p| p.view.read(cx));
                        json!({
                            "title": view.as_ref().map(|v| v.title().to_string()).unwrap_or_default(),
                            "cwd": view
                                .as_ref()
                                .and_then(|v| v.cwd_path())
                                .map(|p| p.to_string_lossy().into_owned()),
                            "focused": id == focused,
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
                let active = self.tabs.active_index();
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
                if index >= self.tabs.len() {
                    return Err(format!("no tab at index {index}"));
                }
                self.activatetab(index, window, cx);
                Ok(json!({ "ok": true, "index": index }))
            }
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
                self.tabs.new_tab(id);
                self.focusactive(window, cx);
                cx.notify();
            }
            "split_right" => self.splitcommand(text, Axis::Horizontal, false, window, cx),
            "split_down" => self.splitcommand(text, Axis::Vertical, false, window, cx),
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
    ) -> Option<PaneId> {
        let inherit = self
            .panes
            .get(&self.tabs.focused())
            .and_then(|pane| pane.view.read(cx).cwd())
            .and_then(|osc| session::cwdpath(&osc));
        let mut options = session::options(&self.opts, SPAWN_COLS, SPAWN_ROWS, inherit);
        let cwd = options.spawn.cwd.clone();
        options.spawn = commandspawn(&self.opts, command);
        options.spawn.cwd = cwd;
        self.spawn(options, window, cx)
    }

    pub(crate) fn splitcommand(
        &mut self,
        command: &str,
        axis: Axis,
        first: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let target = self.tabs.focused();
        let Some(id) = self.spawncommand(command, window, cx) else {
            return;
        };
        if self
            .tabs
            .active_mut()
            .tree
            .split(target, axis, id, first)
            .is_none()
        {
            self.panes.remove(&id);
            return;
        }
        self.tabs.focus(id);
        self.focusactive(window, cx);
        cx.notify();
    }

    /// Spawn a pane running `command` (or a plain shell when `None`).
    pub(crate) fn spawn_pane(
        &mut self,
        command: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<PaneId> {
        match command {
            Some(c) => self.spawncommand(c, window, cx),
            None => {
                let inherit = self
                    .panes
                    .get(&self.tabs.focused())
                    .and_then(|pane| pane.view.read(cx).cwd())
                    .and_then(|osc| session::cwdpath(&osc));
                let options = session::options(&self.opts, SPAWN_COLS, SPAWN_ROWS, inherit);
                self.spawn(options, window, cx)
            }
        }
    }

    /// Open a fresh tab arranged per `layout`, one pane per slot. `commands[i]`
    /// is the command for leaf `i` in pre-order (`None` = a plain shell).
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
        self.tabs.new_tab(root);
        self.realize_into(layout, root, 0, commands, window, cx);
        if let Some(t) = title {
            let idx = self.tabs.active_index();
            self.rename_tab(idx, t, cx);
        }
        self.focusactive(window, cx);
        cx.notify();
    }

    /// Spawn a pane rooted at `cwd` (or the configured default when `None`).
    pub(crate) fn spawn_cwd(
        &mut self,
        cwd: Option<std::path::PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<PaneId> {
        let options = session::options(&self.opts, SPAWN_COLS, SPAWN_ROWS, cwd);
        self.spawn(options, window, cx)
    }
}
