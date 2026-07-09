use super::*;
use gpui::prelude::*;

/// One pane's restore seed: its working directory, and (for agent panes) the
/// launch command plus the native session id used to resume it.
#[derive(Default, Clone)]
pub(crate) struct RestoredPane {
    cwd: Option<std::path::PathBuf>,
    command: Option<String>,
    session: Option<String>,
}

impl WorkspaceView {
    /// Realize a restored arrangement: spawn its root item into the focused
    /// pane, then split to rebuild the tree, seeding each pane from its saved
    /// cwd (and resuming agents that saved a session).
    fn restore_layout(
        &mut self,
        layout: &crate::tiles::Layout,
        panes: &[RestoredPane],
        title: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let seed = panes.first().cloned().unwrap_or_default();
        let Some(root) = self.spawn_restored(&seed, window, cx) else {
            return;
        };
        self.group.update(cx, |g, cx| g.add_to_focused(root, cx));
        let host = self.group.read(cx).focused_pane();
        self.realize_restore(layout, host, 0, panes, window, cx);
        if let Some(t) = title {
            self.rename_item(root, t, cx);
        }
        self.focusactive(window, cx);
    }

    /// Like [`Self::realize_into`] but seeds panes from saved working
    /// directories (and resumes agents) instead of fresh commands.
    fn realize_restore(
        &mut self,
        node: &crate::tiles::Layout,
        host: PaneId,
        host_index: usize,
        panes: &[RestoredPane],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let crate::tiles::Layout::Split {
            axis,
            ratio,
            first,
            second,
        } = node
        else {
            return;
        };
        let second_index = host_index + first.leaves();
        let seed = panes.get(second_index).cloned().unwrap_or_default();
        let Some(item) = self.spawn_restored(&seed, window, cx) else {
            return;
        };
        let new_pane = self.split_pane(host, axis.axis(), *ratio, item, cx);
        self.realize_restore(first, host, host_index, panes, window, cx);
        self.realize_restore(second, new_pane, second_index, panes, window, cx);
    }

    /// Spawn one restored pane: relaunch (and resume) a saved agent when both a
    /// command and a native session id are present, else a plain shell at the
    /// saved cwd. Only session-backed agent panes are relaunched, so ordinary
    /// shells never re-run a stale command.
    fn spawn_restored(
        &mut self,
        seed: &RestoredPane,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<ItemId> {
        match (seed.command.as_deref(), seed.session.as_deref()) {
            (Some(command), Some(session)) if !session.is_empty() => {
                let run = crate::resume::resume_command(command, session);
                let id = self.spawn_command_cwd(&run, seed.cwd.clone(), window, cx)?;
                // Keep the original command + session so a further restart resumes
                // again (resume_command is a no-op on an already-resumed command).
                if let Some(it) = self.items.borrow_mut().get_mut(&id) {
                    it.command = Some(command.to_string());
                    it.agent_session = Some(session.to_string());
                }
                Some(id)
            }
            _ => self.spawn_cwd(seed.cwd.clone(), window, cx),
        }
    }

    /// Spawn `command` rooted at `cwd` (like [`Self::spawncommand`] but with an
    /// explicit working directory rather than the focused pane's).
    fn spawn_command_cwd(
        &mut self,
        command: &str,
        cwd: Option<std::path::PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<ItemId> {
        let mut options = session::options(&self.opts, SPAWN_COLS, SPAWN_ROWS, cwd);
        let saved_cwd = options.spawn.cwd.clone();
        options.spawn = commandspawn(&self.opts, command);
        options.spawn.cwd = saved_cwd;
        let id = self.spawn(options, window, cx)?;
        self.set_item_command(id, command);
        Some(id)
    }

    /// Rebuild the saved session into this fresh window, then drop the empty
    /// default item it launched with. No-op without a saved session.
    pub(crate) fn try_restore(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = crate::sessionstate::load() else {
            return;
        };
        if state.tabs.is_empty() {
            return;
        }
        let initial = self.group.read(cx).active_item();
        for tab in &state.tabs {
            let panes: Vec<RestoredPane> = (0..tab.cwds.len().max(tab.commands.len()))
                .map(|i| RestoredPane {
                    cwd: tab.cwds.get(i).cloned().flatten().map(std::path::PathBuf::from),
                    command: tab.commands.get(i).cloned().flatten(),
                    session: tab.sessions.get(i).cloned().flatten(),
                })
                .collect();
            self.restore_layout(&tab.layout, &panes, tab.title.as_deref(), window, cx);
        }
        // Drop the placeholder shell the window launched with.
        self.close_item(initial, window, cx);
    }

    /// Quit, but warn first when a process is still running in an item and
    /// `confirm-quit` is on. The native dialog runs async; we quit only if the
    /// user confirms.
    pub(crate) fn request_quit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.opts.confirm_quit || !self.any_process_running(cx) {
            self.save_state(cx);
            cx.quit();
            return;
        }
        let answer = window.prompt(
            gpui::PromptLevel::Warning,
            "Quit Sinclair?",
            Some("A process is still running in one of your terminals. Quitting will end it."),
            &["Quit", "Cancel"],
            cx,
        );
        let weak = cx.weak_entity();
        window
            .spawn(cx, async move |cx| {
                if let Ok(0) = answer.await {
                    let _ = weak.update(cx, |this, cx| {
                        this.save_state(cx);
                        cx.quit();
                    });
                }
            })
            .detach();
    }

    /// Whether any item in this window has a live foreground process.
    fn any_process_running(&self, cx: &App) -> bool {
        self.items
            .borrow()
            .values()
            .any(|it| it.content.has_running_process(cx))
    }

    /// Persist this window's split arrangement and per-pane cwds for the next
    /// launch. The whole window is stored as one arrangement; only each pane's
    /// first item's cwd is captured. Windows containing a webview item are not
    /// saved (they can't round-trip through the terminal-only restore path).
    pub(crate) fn save_state(&self, cx: &App) {
        if !self.opts.session_restore {
            return;
        }
        let tree = self.group.read(cx).tree().clone();
        let panes = tree.panes();
        // Skip saving if any item is a non-terminal (webview) surface.
        let has_webview = self
            .items
            .borrow()
            .values()
            .any(|it| it.content.as_terminal().is_none());
        if has_webview {
            crate::sessionstate::save(&crate::sessionstate::SessionState {
                tabs: Vec::new(),
                active: 0,
            });
            return;
        }
        let mut cwds = Vec::with_capacity(panes.len());
        let mut commands = Vec::with_capacity(panes.len());
        let mut sessions = Vec::with_capacity(panes.len());
        for &p in &panes {
            let first = self
                .group
                .read(cx)
                .pane_items(p)
                .and_then(|items| items.first().copied());
            let items = self.items.borrow();
            let it = first.and_then(|id| items.get(&id));
            cwds.push(
                it.and_then(|it| it.content.cwd(cx))
                    .and_then(|osc| session::cwdpath(&osc))
                    .map(|p| p.to_string_lossy().into_owned()),
            );
            // Only agent panes (those that reported a native session) carry a
            // command; ordinary shells restore fresh so nothing is re-run.
            let is_agent = it.is_some_and(|it| it.agent_session.is_some());
            commands.push(is_agent.then(|| it.and_then(|it| it.command.clone())).flatten());
            sessions.push(it.and_then(|it| it.agent_session.clone()));
        }
        let tabs = vec![crate::sessionstate::TabState {
            layout: crate::tiles::from_tree(tree.root()),
            cwds,
            title: None,
            commands,
            sessions,
        }];
        crate::sessionstate::save(&crate::sessionstate::SessionState { tabs, active: 0 });
    }

    /// Split `host` along `axis` at `ratio`, putting `item` in the new (second)
    /// pane. Returns the new pane id.
    fn split_pane(
        &mut self,
        host: PaneId,
        axis: SplitAxis,
        ratio: f32,
        item: ItemId,
        cx: &mut Context<Self>,
    ) -> PaneId {
        self.group.update(cx, |g, cx| {
            let new_pane = g.split(host, axis, false, item, cx);
            if let Some(sid) = g.tree().nearest_split(new_pane, axis) {
                g.set_ratio(sid, ratio, cx);
            }
            new_pane
        })
    }

    /// Recursively split `host` to realize `node`; `host_index` is the pre-order
    /// index of the subtree's anchor (left/top-most) leaf.
    pub(crate) fn realize_into(
        &mut self,
        node: &crate::tiles::Layout,
        host: PaneId,
        host_index: usize,
        commands: &[Option<String>],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let crate::tiles::Layout::Split {
            axis,
            ratio,
            first,
            second,
        } = node
        else {
            return;
        };
        let second_index = host_index + first.leaves();
        let cmd = commands.get(second_index).and_then(|c| c.as_deref());
        let Some(item) = self.spawn_pane(cmd, window, cx) else {
            return;
        };
        let new_pane = self.split_pane(host, axis.axis(), *ratio, item, cx);
        self.realize_into(first, host, host_index, commands, window, cx);
        self.realize_into(second, new_pane, second_index, commands, window, cx);
    }

    /// Apply a tile layout (preset or saved custom) as plain shells.
    pub(crate) fn apply_tile(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some((layout, n)) = crate::tiles::resolve(id) else {
            eprintln!("sinclair: unknown tile `{id}`");
            return;
        };
        let label = crate::tiles::presets()
            .iter()
            .find(|p| p.0 == id)
            .map(|p| p.1.to_string())
            .unwrap_or_else(|| id.to_string());
        let commands = vec![None; n];
        self.apply_layout(&layout, &commands, Some(&label), window, cx);
    }

    /// Open a Relay team: a tile of agents, each pane launched into the mesh.
    pub(crate) fn open_team(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        crate::relay::ensure_running(&self.opts);
        let Some((shape, members)) = crate::relay::team_info(name) else {
            eprintln!("sinclair: team `{name}` not found");
            return;
        };
        if members.is_empty() {
            return;
        }
        let layout = crate::tiles::generate(&shape, members.len());
        let commands: Vec<Option<String>> = members
            .iter()
            .enumerate()
            .map(|(i, (m, role, agent))| {
                Some(crate::relay::launch_member(
                    m,
                    role,
                    agent,
                    i == 0,
                    self.opts.ai_optimize_tokens,
                ))
            })
            .collect();
        self.apply_layout(&layout, &commands, Some(name), window, cx);
    }

    /// Rebuild menus after the Team Builder saves, so the new team shows under
    /// AI → Teams without a restart.
    pub(crate) fn after_team_saved(&mut self, cx: &mut Context<Self>) {
        self.setmenus(cx);
    }

    /// Add an agent (a `relay launch` command from the New Agent modal) to the
    /// current workspace as a split.
    pub fn create_agent(&mut self, cmd: &str, window: &mut Window, cx: &mut Context<Self>) {
        crate::relay::ensure_running(&self.opts);
        self.splitcommand(cmd, SplitAxis::Horizontal, false, window, cx);
    }

    /// Sinclair for a name and save the current arrangement as a custom tile.
    pub(crate) fn open_save_layout(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let layout = crate::tiles::from_tree(self.group.read(cx).tree().root());
        self.open_rename(crate::rename::Target::Layout(layout), String::new(), window, cx);
    }

    /// Persist a captured layout under `name` and refresh the Tiles menu.
    pub fn save_layout(&mut self, name: &str, layout: crate::tiles::Layout, cx: &mut Context<Self>) {
        if name.trim().is_empty() {
            return;
        }
        match crate::tiles::save(name, &layout) {
            Ok(path) => eprintln!("sinclair: saved layout {}", path.display()),
            Err(e) => eprintln!("sinclair: save layout failed: {e}"),
        }
        self.setmenus(cx);
    }
}
