use super::*;
use gpui::prelude::*;

impl WorkspaceView {
    /// Realize a restored tab: build the split tree, spawning each pane in its
    /// saved working directory.
    fn restore_layout(
        &mut self,
        layout: &crate::tiles::Layout,
        cwds: &[Option<std::path::PathBuf>],
        title: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(root) = self.spawn_cwd(cwds.first().cloned().flatten(), window, cx) else {
            return;
        };
        self.tabs.new_tab(root);
        self.realize_restore(layout, root, 0, cwds, window, cx);
        if let Some(t) = title {
            let idx = self.tabs.active_index();
            self.rename_tab(idx, t, cx);
        }
        self.focusactive(window, cx);
    }

    /// Like [`Self::realize_into`] but seeds panes from saved working
    /// directories instead of commands.
    fn realize_restore(
        &mut self,
        node: &crate::tiles::Layout,
        host: PaneId,
        host_index: usize,
        cwds: &[Option<std::path::PathBuf>],
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
        let cwd = cwds.get(second_index).cloned().flatten();
        let Some(newpane) = self.spawn_cwd(cwd, window, cx) else {
            return;
        };
        match self
            .tabs
            .active_mut()
            .tree
            .split(host, axis.axis(), newpane, false)
        {
            Some(split) => {
                self.tabs.active_mut().tree.set_ratio(split, *ratio);
            }
            None => {
                self.panes.remove(&newpane);
                return;
            }
        }
        self.realize_restore(first, host, host_index, cwds, window, cx);
        self.realize_restore(second, newpane, second_index, cwds, window, cx);
    }

    /// Rebuild the saved session into this fresh window, then drop the empty
    /// default tab it launched with. No-op without a saved session.
    pub(crate) fn try_restore(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = crate::sessionstate::load() else {
            return;
        };
        if state.tabs.is_empty() {
            return;
        }
        for tab in &state.tabs {
            let cwds: Vec<Option<std::path::PathBuf>> = tab
                .cwds
                .iter()
                .map(|s| s.as_ref().map(std::path::PathBuf::from))
                .collect();
            self.restore_layout(&tab.layout, &cwds, tab.title.as_deref(), window, cx);
        }
        self.closetab(0, window, cx);
        let active = state.active.min(self.tabs.len().saturating_sub(1));
        self.activatetab(active, window, cx);
    }

    /// Quit, but warn first when a process is still running in a pane and
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
            "Quit Prompt?",
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

    /// Whether any pane in this window has a live foreground process.
    fn any_process_running(&self, cx: &App) -> bool {
        self.panes
            .values()
            .any(|p| p.view.read(cx).has_running_process())
    }

    /// Persist this window's tabs/splits/cwds for the next launch.
    pub(crate) fn save_state(&self, cx: &App) {
        if !self.opts.session_restore {
            return;
        }
        let tabs = (0..self.tabs.len())
            .filter_map(|i| {
                let tab = self.tabs.get(i)?;
                let cwds = tab
                    .tree
                    .panes()
                    .iter()
                    .map(|id| {
                        self.panes
                            .get(id)
                            .and_then(|p| p.view.read(cx).cwd())
                            .and_then(|osc| session::cwdpath(&osc))
                            .map(|p| p.to_string_lossy().into_owned())
                    })
                    .collect();
                Some(crate::sessionstate::TabState {
                    layout: crate::tiles::from_tree(tab.tree.root()),
                    cwds,
                    title: tab.title.clone(),
                })
            })
            .collect();
        crate::sessionstate::save(&crate::sessionstate::SessionState {
            tabs,
            active: self.tabs.active_index(),
        });
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
        let Some(newpane) = self.spawn_pane(cmd, window, cx) else {
            return;
        };
        match self
            .tabs
            .active_mut()
            .tree
            .split(host, axis.axis(), newpane, false)
        {
            Some(split) => {
                self.tabs.active_mut().tree.set_ratio(split, *ratio);
            }
            None => {
                self.panes.remove(&newpane);
                return;
            }
        }
        self.realize_into(first, host, host_index, commands, window, cx);
        self.realize_into(second, newpane, second_index, commands, window, cx);
    }

    /// Apply a tile layout (preset or saved custom) as plain shells.
    pub(crate) fn apply_tile(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some((layout, n)) = crate::tiles::resolve(id) else {
            eprintln!("prompt: unknown tile `{id}`");
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
            eprintln!("prompt: team `{name}` not found");
            return;
        };
        if members.is_empty() {
            return;
        }
        let layout = crate::tiles::generate(&shape, members.len());
        let commands: Vec<Option<String>> = members
            .iter()
            .enumerate()
            .map(|(i, (m, role))| Some(crate::relay::launch_member(m, role, i == 0)))
            .collect();
        self.apply_layout(&layout, &commands, Some(name), window, cx);
    }

    /// Add an agent (a `relay launch` command from the New Agent modal) to the
    /// current workspace as a split.
    pub fn create_agent(&mut self, cmd: &str, window: &mut Window, cx: &mut Context<Self>) {
        crate::relay::ensure_running(&self.opts);
        self.splitcommand(cmd, Axis::Horizontal, false, window, cx);
    }

    /// Prompt for a name and save the current tab's arrangement as a custom tile.
    pub(crate) fn open_save_layout(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let layout = crate::tiles::from_tree(self.tabs.active().tree.root());
        let root = cx.weak_entity();
        crate::rename::open(
            window,
            root,
            crate::rename::Target::Layout(layout),
            String::new(),
            cx,
        );
    }

    /// Persist a captured layout under `name` and refresh the Tiles menu.
    pub fn save_layout(&mut self, name: &str, layout: crate::tiles::Layout, cx: &mut Context<Self>) {
        if name.trim().is_empty() {
            return;
        }
        match crate::tiles::save(name, &layout) {
            Ok(path) => eprintln!("prompt: saved layout {}", path.display()),
            Err(e) => eprintln!("prompt: save layout failed: {e}"),
        }
        self.setmenus(cx);
    }
}
