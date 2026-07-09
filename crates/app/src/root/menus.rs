use super::*;
use gpui::prelude::*;

impl WorkspaceView {
    /// (Re)bind every resolved keybind to a [`RunBind`] carrying its table
    /// index. Triggers with no gpui spelling are skipped. Called at startup
    /// and after a live reload.
    pub(crate) fn applykeybinds(&self, cx: &mut Context<Self>) {
        cx.clear_key_bindings();
        let mut bindings = Vec::new();
        for (i, kb) in self.keybinds.iter().enumerate() {
            let Some(ks) = keys::keystroke_seq(kb) else {
                continue;
            };
            // `ks` may be a space-joined chord; validate each stroke since
            // `Keystroke::parse` only handles one at a time.
            if ks.split(' ').any(|s| gpui::Keystroke::parse(s).is_err()) {
                continue;
            }
            bindings.push(KeyBinding::new(&ks, RunBind(i), Some("Workspace")));
        }
        cx.bind_keys(bindings);
    }

    /// A native menu item for `action`. When the action has a bound keybind
    /// the item dispatches through [`RunBind`] so gpui shows the shortcut;
    /// otherwise it falls back to a [`MenuPick`] index into `actions` so the
    /// item still works without a binding. Used for every menu entry so the
    /// menu and keymap never drift.
    fn pick(&self, actions: &mut Vec<Action>, label: &str, action: Action) -> Option<MenuItem> {
        Some(self.pick_checked(actions, label, action, false))
    }

    /// Like [`Self::pick`] but with an explicit checkmark, for toggle items.
    fn pick_checked(
        &self,
        actions: &mut Vec<Action>,
        label: &str,
        action: Action,
        checked: bool,
    ) -> MenuItem {
        let dispatch: Box<dyn gpui::Action> =
            match self.keybinds.iter().position(|k| k.action == action) {
                Some(index) => Box::new(RunBind(index)),
                None => {
                    let index = actions.len();
                    actions.push(action);
                    Box::new(MenuPick(index))
                }
            };
        MenuItem::Action {
            name: label.to_string().into(),
            action: dispatch,
            os_action: None,
            checked,
            disabled: false,
        }
    }

    /// Install the native application menu bar (macOS). Items reuse the
    /// config-driven actions, so the menu and keymap never drift. Re-run
    /// after a reload since keybind indices may change, and after toggles
    /// (read-only) so the checkmark stays in sync.
    pub(crate) fn setmenus(&mut self, cx: &mut Context<Self>) {
        let mut actions: Vec<Action> = Vec::new();
        let mut menus = vec![
            self.prompt_menu(&mut actions),
            self.shell_menu(&mut actions, cx),
            self.edit_menu(&mut actions),
            self.view_menu(&mut actions, cx),
            self.workspace_menu(&mut actions, cx),
            self.window_menu(&mut actions),
        ];
        menus.push(self.plugins_menu(&mut actions));
        if self.opts.ai_enabled {
            menus.push(self.ai_menu(&mut actions));
        }
        menus.push(Menu {
            name: "Help".into(),
            items: vec![MenuItem::action("Documents", ShowDocs)],
            disabled: false,
        });
        self.menu_actions = actions;
        cx.set_menus(menus);
    }

    /// Rebuild the menus after a short delay, so the Relay status line catches
    /// up with a server start/stop that runs in the background.
    pub(crate) fn refresh_menus_soon(&self, window: &mut Window, cx: &mut Context<Self>) {
        let weak = cx.weak_entity();
        window
            .spawn(cx, async move |cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(1500))
                    .await;
                let _ = weak.update(cx, |this, cx| this.setmenus(cx));
            })
            .detach();
    }

    // Only built when AI is enabled (see `setmenus`), so the server is available
    // on demand — no need to gate the contents on the persistent-mesh setting.
    fn ai_menu(&self, a: &mut Vec<Action>) -> Menu {
        let mut items: Vec<Option<MenuItem>> = Vec::new();
        items.push(Some(MenuItem::submenu(self.agents_submenu(a))));
        items.push(Some(MenuItem::submenu(self.relay_submenu(a))));
        items.push(self.pick(a, "Open Feed", Action::RelayFeed));
        let teams = crate::relay::team_list();
        let mut t: Vec<Option<MenuItem>> = vec![self.pick(a, "Build Team\u{2026}", Action::BuildTeam)];
        if !teams.is_empty() {
            t.push(Some(MenuItem::separator()));
            for name in teams {
                t.push(self.pick(a, &name, Action::OpenTeam(name.clone())));
            }
        }
        items.push(Some(MenuItem::separator()));
        items.push(Some(MenuItem::submenu(Self::menu("Teams", t))));
        Self::menu("AI", items)
    }

    /// Probe each enabled provider off the UI thread and cache the set that
    /// verifies, then rebuild the menus so quick-launch lists only tools that
    /// actually resolve here. Until it finishes, every enabled provider shows.
    pub(crate) fn refresh_agent_verification(&self, cx: &mut Context<Self>) {
        if !crate::relay::available(&self.opts) {
            return;
        }
        let executor = cx.background_executor().clone();
        let opts = self.opts.clone();
        cx.spawn(async move |this, cx| {
            let set = executor
                .spawn(async move {
                    crate::relay::enabled_agents(&opts)
                        .into_iter()
                        .filter(|p| crate::relay::agent_verifies(&opts, p))
                        .collect::<HashSet<String>>()
                })
                .await;
            let _ = this.update(cx, |view, cx| {
                if view.verified_agents.as_ref() != Some(&set) {
                    view.verified_agents = Some(set);
                    view.setmenus(cx);
                }
            });
        })
        .detach();
    }

    fn agents_submenu(&self, a: &mut Vec<Action>) -> Menu {
        let mut items: Vec<Option<MenuItem>> = vec![self.pick(a, "Define Agent\u{2026}", Action::RelayLaunch)];
        // Quick-launch: one item per configured provider that verified, launched
        // straight into a split (default role, no task) via the shared path.
        let providers = crate::relay::enabled_agents(&self.opts);
        let providers = match &self.verified_agents {
            Some(ok) => providers.into_iter().filter(|p| ok.contains(p)).collect::<Vec<_>>(),
            None => providers,
        };
        if !providers.is_empty() {
            items.push(Some(MenuItem::separator()));
            for provider in providers {
                let label = format!("Launch {}", crate::relay::provider_label(&provider));
                items.push(self.pick(a, &label, Action::LaunchAgent(provider)));
            }
        }
        let defs = crate::relay::list_agent_defs();
        if !defs.is_empty() {
            items.push(Some(MenuItem::separator()));
            for d in defs {
                let label = format!("{} \u{00b7} {}", d.name, d.provider);
                items.push(self.pick(a, &label, Action::AgentDef(d.name)));
            }
        }
        Self::menu("Agents", items)
    }

    /// Relay server controls and logs (AI → Relay). Shows the live server
    /// state, then start/stop/restart and a jump to the server log.
    fn relay_submenu(&self, a: &mut Vec<Action>) -> Menu {
        let status = if crate::relay::running() {
            "\u{25cf} Server running"
        } else {
            "\u{25cb} Server stopped"
        };
        Self::menu(
            "Relay",
            vec![
                Some(Self::status_item(status)),
                Some(MenuItem::separator()),
                self.pick(a, "Resume Mesh (start)", Action::RelayStart),
                self.pick(a, "Pause Mesh (stop)", Action::RelayStop),
                self.pick(a, "Restart Server", Action::RelayRestart),
                Some(MenuItem::separator()),
                self.pick(a, "View Logs", Action::RelayLog),
            ],
        )
    }

    /// Top-level Plugins menu: each installed plugin (click opens its primary
    /// surface — a webview, else a panel, else its first command), then a
    /// "Manage Plugins…" item that opens the Plugins drawer (browse + install).
    fn plugins_menu(&self, a: &mut Vec<Action>) -> Menu {
        let mut items: Vec<Option<MenuItem>> = Vec::new();
        if self.plugins.is_empty() {
            items.push(Some(Self::status_item("No plugins installed")));
        } else {
            for plugin in &self.plugins {
                let primary = plugin
                    .webview
                    .as_ref()
                    .map(|w| Action::OpenWebview(w.id.clone()))
                    .or_else(|| {
                        plugin
                            .panel
                            .as_ref()
                            .map(|p| Action::Sidebar(format!("right:plugin:{}", p.id)))
                    })
                    .or_else(|| {
                        plugin
                            .commands
                            .first()
                            .map(|c| Action::PluginCommand(plugin::actionid(&plugin.id, &c.id)))
                    });
                match primary {
                    Some(action) => items.push(self.pick(a, &plugin.name, action)),
                    None => items.push(Some(Self::status_item(&plugin.name))),
                }
            }
        }
        items.push(Some(MenuItem::separator()));
        items.push(self.pick(a, "Manage Plugins\u{2026}", Action::ManagePlugins));
        Self::menu("Plugins", items)
    }

    /// A greyed-out, non-interactive informational menu row.
    fn status_item(label: &str) -> MenuItem {
        MenuItem::Action {
            name: label.to_string().into(),
            action: Box::new(MenuPick(usize::MAX)),
            os_action: None,
            checked: false,
            disabled: true,
        }
    }

    fn menu(name: &str, items: Vec<Option<MenuItem>>) -> Menu {
        Menu {
            name: name.to_string().into(),
            items: items.into_iter().flatten().collect(),
            disabled: false,
        }
    }

    fn prompt_menu(&self, a: &mut Vec<Action>) -> Menu {
        Self::menu(
            "Sinclair",
            vec![
                Some(MenuItem::action("About Sinclair", ShowAbout)),
                self.pick(a, "Check for Updates\u{2026}", Action::CheckUpdates),
                Some(MenuItem::separator()),
                self.pick(a, "Command Palette\u{2026}", Action::CommandPalette),
                self.pick(a, "Settings\u{2026}", Action::ToggleSettings),
                Some(MenuItem::separator()),
                self.pick(a, "Quit Sinclair", Action::Quit),
            ],
        )
    }

    fn shell_menu(&self, a: &mut Vec<Action>, cx: &App) -> Menu {
        let recording = self
            .focused_terminal(cx)
            .is_some_and(|v| v.read(cx).is_recording());
        Self::menu(
            "File",
            vec![
                self.pick(a, "New Window", Action::NewWindow),
                self.pick(a, "New Tab", Action::NewTab),
                self.pick(a, "New OS Tab\u{2026}", Action::NewContainerTab),
                Some(MenuItem::separator()),
                self.pick(a, "Notes", Action::Notes),
                Some(MenuItem::separator()),
                self.pick(a, "Save Buffer\u{2026}", Action::SaveBuffer),
                Some(MenuItem::separator()),
                Some(self.pick_checked(a, "Record Session", Action::ToggleRecording, recording)),
                self.pick(a, "Export Recording as GIF", Action::ExportRecording("gif".into())),
                self.pick(a, "Export Recording as MP4", Action::ExportRecording("mp4".into())),
                Some(MenuItem::separator()),
                self.pick(a, "Close", Action::CloseSurface),
                self.pick(a, "Close Tab", Action::CloseTab),
                self.pick(a, "Close Window", Action::CloseWindow),
                self.pick(a, "Close All Windows", Action::CloseAllWindows),
            ],
        )
    }

    fn edit_menu(&self, a: &mut Vec<Action>) -> Menu {
        Self::menu(
            "Edit",
            vec![
                self.pick(a, "Copy", Action::Copy),
                self.pick(a, "Paste", Action::Paste),
                self.pick(a, "Select All", Action::SelectAll),
                Some(MenuItem::separator()),
                self.pick(a, "Find\u{2026}", Action::ToggleSearch),
                self.pick(a, "Semantic Find", Action::ToggleSemanticSearch),
                self.pick(a, "Explain Output", Action::ExplainOutput),
                self.pick(a, "Compose Command", Action::ComposeCommand),
            ],
        )
    }

    /// View menu: font size plus the title/read-only group.
    fn view_menu(&self, a: &mut Vec<Action>, cx: &App) -> Menu {
        let read_only = self
            .focused_terminal(cx)
            .is_some_and(|v| v.read(cx).is_read_only());
        Self::menu(
            "View",
            vec![
                self.pick(a, "Reset Font Size", Action::ResetFontSize),
                self.pick(a, "Increase Font Size", Action::IncreaseFontSize(1.0)),
                self.pick(a, "Decrease Font Size", Action::DecreaseFontSize(1.0)),
                Some(MenuItem::separator()),
                self.pick(a, "Change Tab Title\u{2026}", Action::ChangeTabTitle),
                self.pick(a, "Change Terminal Title\u{2026}", Action::ChangeTerminalTitle),
                Some(self.pick_checked(a, "Terminal Read-only", Action::ToggleReadOnly, read_only)),
                Some(MenuItem::separator()),
                Some(MenuItem::submenu(self.sidebar_menu(a, SidebarSide::Left))),
                Some(MenuItem::submenu(self.sidebar_menu(a, SidebarSide::Right))),
                Some(MenuItem::separator()),
                self.pick(a, "Quick Terminal", Action::ToggleQuickTerminal),
            ],
        )
    }

    /// A drawer submenu: one checked item per panel, checked when that panel is
    /// the active one on this side. Selecting toggles via [`Action::Sidebar`].
    fn sidebar_menu(&self, a: &mut Vec<Action>, side: SidebarSide) -> Menu {
        let (name, active, prefix) = match side {
            SidebarSide::Left => ("Sidebar", self.left_panel, "left"),
            SidebarSide::Right => ("Right Sidebar", self.right_panel, "right"),
        };
        let items = SidebarPanel::ALL
            .into_iter()
            .map(|panel| {
                let payload = format!("{prefix}:{}", panel.id());
                Some(self.pick_checked(
                    a,
                    panel.label(),
                    Action::Sidebar(payload),
                    active == Some(panel),
                ))
            })
            .collect();
        Self::menu(name, items)
    }

    /// Workspace: split creation, navigation, sizing, saved tile presets, and
    /// broadcast input. Splitting is pane-tree (workspace) based, so it lives
    /// here rather than scattered across Shell/View/Window.
    fn workspace_menu(&self, a: &mut Vec<Action>, cx: &App) -> Menu {
        let broadcasting = cx.try_global::<Broadcast>().is_some_and(|b| b.0);
        let select_split = Self::menu(
            "Select Split",
            vec![
                self.pick(a, "Select Split Above", Action::GotoSplit(SplitFocus::Up)),
                self.pick(a, "Select Split Below", Action::GotoSplit(SplitFocus::Down)),
                self.pick(a, "Select Split Left", Action::GotoSplit(SplitFocus::Left)),
                self.pick(a, "Select Split Right", Action::GotoSplit(SplitFocus::Right)),
            ],
        );
        let resize_split = Self::menu(
            "Resize Split",
            vec![
                self.pick(a, "Equalize Splits", Action::EqualizeSplits),
                self.pick(a, "Move Divider Up", Action::ResizeSplit(ResizeDir::Up)),
                self.pick(a, "Move Divider Down", Action::ResizeSplit(ResizeDir::Down)),
                self.pick(a, "Move Divider Left", Action::ResizeSplit(ResizeDir::Left)),
                self.pick(a, "Move Divider Right", Action::ResizeSplit(ResizeDir::Right)),
            ],
        );
        let mut items = vec![
            self.pick(a, "Split Right", Action::NewSplit(SplitDirection::Right)),
            self.pick(a, "Split Left", Action::NewSplit(SplitDirection::Left)),
            self.pick(a, "Split Down", Action::NewSplit(SplitDirection::Down)),
            Some(MenuItem::separator()),
            self.pick(a, "Zoom Split", Action::ZoomSplit),
            self.pick(a, "Select Previous Split", Action::GotoSplit(SplitFocus::Previous)),
            self.pick(a, "Select Next Split", Action::GotoSplit(SplitFocus::Next)),
            Some(MenuItem::submenu(select_split)),
            Some(MenuItem::submenu(resize_split)),
            Some(MenuItem::separator()),
        ];
        for (id, label, _, _) in crate::tiles::presets() {
            items.push(self.pick(a, label, Action::Tile((*id).to_string())));
        }
        let custom = crate::tiles::list_custom();
        if !custom.is_empty() {
            items.push(Some(MenuItem::separator()));
            for name in custom {
                items.push(self.pick(a, &name, Action::Tile(name.clone())));
            }
        }
        items.push(Some(MenuItem::separator()));
        items.push(self.pick(a, "Save Current Layout\u{2026}", Action::SaveLayout));
        items.push(Some(MenuItem::separator()));
        items.push(Some(self.pick_checked(
            a,
            "Broadcast Input",
            Action::ToggleBroadcast,
            broadcasting,
        )));
        Self::menu("Workspace", items)
    }

    /// Window menu. Tab navigation is listed explicitly since Sinclair does not
    /// use the native macOS tabs that macOS would populate automatically.
    fn window_menu(&self, a: &mut Vec<Action>) -> Menu {
        Self::menu(
            "Window",
            vec![
                self.pick(a, "Minimize", Action::MinimizeWindow),
                self.pick(a, "Zoom", Action::ZoomWindow),
                self.pick(a, "Toggle Full Screen", Action::ToggleFullscreen),
                self.pick(a, "Show/Hide All Terminals", Action::HideAll),
                Some(MenuItem::separator()),
                self.pick(a, "Return To Default Size", Action::ReturnToDefaultSize),
                self.pick(a, "Use as Default", Action::UseAsDefault),
                Some(MenuItem::separator()),
                self.pick(a, "Previous Tab", Action::PreviousTab),
                self.pick(a, "Next Tab", Action::NextTab),
                Some(MenuItem::separator()),
                self.pick(a, "Bring All to Front", Action::BringAllToFront),
            ],
        )
    }
}
