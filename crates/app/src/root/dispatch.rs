use super::*;
use gpui::prelude::*;

impl WorkspaceView {
    /// Run something on the focused pane's terminal view. No-op when the focused
    /// pane is a web view — this single guard makes every terminal-only action
    /// (copy, paste, search, scroll, recording, …) skip webview panes.
    pub(crate) fn onfocused(
        &self,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut TerminalView, &mut Context<TerminalView>),
    ) {
        if let Some(v) = self.focused_terminal(cx) {
            v.update(cx, |view, cx| f(view, cx));
        }
    }

    /// Dispatch handler shared by every keybinding.
    pub(crate) fn runbind(&mut self, action: &RunBind, window: &mut Window, cx: &mut Context<Self>) {
        let Some(kb) = self.keybinds.get(action.0) else {
            return;
        };
        self.dispatch(kb.action.clone(), window, cx);
    }

    /// Open the documentation window (Help menu).
    pub(crate) fn showdocs(&mut self, _: &ShowDocs, window: &mut Window, cx: &mut Context<Self>) {
        crate::help::open(window, cx);
    }

    /// Open the About panel (application menu).
    pub(crate) fn showabout(&mut self, _: &ShowAbout, window: &mut Window, cx: &mut Context<Self>) {
        crate::about::open(window, cx);
    }

    /// Dispatch a keybind-less menu item via its `menu_actions` index.
    pub(crate) fn menupick(&mut self, action: &MenuPick, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(config_action) = self.menu_actions.get(action.0).cloned() {
            self.dispatch(config_action, window, cx);
        }
    }

    /// Run an action from outside the keymap (the command palette).
    pub fn run_action(&mut self, action: Action, window: &mut Window, cx: &mut Context<Self>) {
        self.dispatch(action, window, cx);
    }

    /// Carry out one config action.
    pub(crate) fn dispatch(&mut self, action: Action, window: &mut Window, cx: &mut Context<Self>) {
        match action {
            Action::NewWindow => self.newwindow(cx),
            Action::NewTab => self.newtab(window, cx),
            Action::NewContainerTab => crate::ospicker::open(window, cx),
            Action::CloseSurface | Action::CloseTab => {
                let item = self.active_item(cx);
                self.close_item(item, window, cx);
            }
            Action::CloseWindow => self.close_window(window, cx),
            Action::CloseAllWindows => {
                for handle in cx.windows() {
                    handle
                        .update(cx, |_, window, _| window.remove_window())
                        .ok();
                }
            }
            Action::NewSplit(dir) => {
                let (axis, first) = split_dir(dir);
                self.split(axis, first, window, cx);
            }
            Action::GotoSplit(focus) => match focus {
                SplitFocus::Previous => self.cyclesplit(false, window, cx),
                SplitFocus::Next => self.cyclesplit(true, window, cx),
                SplitFocus::Up => self.focusdir(Direction::Up, window, cx),
                SplitFocus::Down => self.focusdir(Direction::Down, window, cx),
                SplitFocus::Left => self.focusdir(Direction::Left, window, cx),
                SplitFocus::Right => self.focusdir(Direction::Right, window, cx),
            },
            Action::ZoomSplit => self.group.update(cx, |g, cx| g.toggle_zoom(cx)),
            Action::EqualizeSplits => self.equalizesplits(cx),
            Action::ResizeSplit(dir) => self.resizesplit(dir, cx),
            Action::GotoTab(n) => self.gototab(n, window, cx),
            Action::PreviousTab => {
                self.group.update(cx, |g, cx| g.activate_prev(cx));
                self.focusactive(window, cx);
                cx.notify();
            }
            Action::NextTab => {
                self.group.update(cx, |g, cx| g.activate_next(cx));
                self.focusactive(window, cx);
                cx.notify();
            }
            Action::MoveTab(delta) => self.movetab(delta, cx),
            Action::Copy => self.onfocused(cx, |v, cx| v.copy_selection(cx)),
            Action::CopyCommandOutput => self.onfocused(cx, |v, cx| v.copy_command_output(cx)),
            Action::Hints => self.onfocused(cx, |v, cx| v.open_hints(cx)),
            Action::CopyMode => self.onfocused(cx, |v, cx| v.toggle_copy_mode(cx)),
            Action::ClipboardHistory => self.open_clipboard_history(window, cx),
            Action::UnicodePicker => self.open_unicode_picker(window, cx),
            Action::Snippets => self.open_snippets(window, cx),
            Action::SearchAll => self.open_global_search(window, cx),
            Action::Profiles => self.open_profiles(window, cx),
            Action::Annotate => self.open_annotate(window, cx),
            Action::Paste => self.onfocused(cx, |v, cx| v.paste_clipboard(cx)),
            Action::SelectAll => self.onfocused(cx, |v, cx| v.select_all(cx)),
            Action::AdjustSelection(dir) => self.onfocused(cx, |v, cx| v.adjust_selection(dir, cx)),
            Action::SendText(bytes) => self.onfocused(cx, |v, cx| v.send_text(&bytes, cx)),
            Action::IncreaseFontSize(amount) => {
                self.setfontsize(px(f32::from(self.font_size) + amount), cx)
            }
            Action::DecreaseFontSize(amount) => {
                self.setfontsize(px(f32::from(self.font_size) - amount), cx)
            }
            Action::ResetFontSize => self.setfontsize(self.base_font_size, cx),
            Action::ScrollPageUp => self.onfocused(cx, |v, cx| v.scroll_pages(1, cx)),
            Action::ScrollPageDown => self.onfocused(cx, |v, cx| v.scroll_pages(-1, cx)),
            Action::ScrollToTop => self.onfocused(cx, |v, cx| v.scroll_to_top(cx)),
            Action::ScrollToBottom => self.onfocused(cx, |v, cx| v.scroll_to_live(cx)),
            Action::JumpToPrompt(delta) => self.onfocused(cx, |v, cx| v.jump_prompt(delta, cx)),
            Action::ClearScreen => self.onfocused(cx, |v, cx| v.clear_screen(cx)),
            Action::ToggleSearch => self.onfocused(cx, |v, cx| v.toggle_search(cx)),
            Action::ToggleSemanticSearch => {
                self.onfocused(cx, |v, cx| v.toggle_semantic_search(cx))
            }
            Action::ExplainOutput => self.onfocused(cx, |v, cx| v.explain_output(cx)),
            Action::ComposeCommand => self.onfocused(cx, |v, cx| v.compose_command(cx)),
            Action::PluginCommand(id) => self.runplugin(&id, window, cx),
            Action::MacroRecord => self.togglerecord(window, cx),
            Action::MacroReplay(name) => {
                if let Err(error) = self.replay_macro(&name, cx) {
                    eprintln!("sinclair: {error}");
                }
            }
            Action::CommandPalette => self.open_palette(window, cx),
            Action::Notes => self.open_notes(window, cx),
            Action::QuickOpen => self.open_quickopen(window, cx),
            Action::ToggleSettings => crate::settings::open(window, cx),
            Action::ShowHelp => crate::help::open(window, cx),
            Action::ToggleFullscreen => window.toggle_fullscreen(),
            Action::MinimizeWindow => window.minimize_window(),
            Action::ZoomWindow => window.zoom_window(),
            Action::HideAll => cx.hide(),
            Action::BringAllToFront => cx.activate(true),
            Action::ReturnToDefaultSize => self.returntodefaultsize(window),
            Action::UseAsDefault => self.useasdefault(cx),
            Action::ChangeTabTitle => self.changetitle(true, window, cx),
            Action::ChangeTerminalTitle => self.changetitle(false, window, cx),
            Action::ToggleReadOnly => {
                self.onfocused(cx, |v, cx| {
                    v.toggle_read_only(cx);
                });
                self.setmenus(cx);
            }
            Action::ToggleBroadcast => self.toggle_broadcast(cx),
            Action::ToggleRecording => {
                self.onfocused(cx, |v, cx| v.toggle_recording(cx));
                self.setmenus(cx);
            }
            Action::ExportRecording(fmt) => {
                self.onfocused(cx, |v, cx| v.export_recording(&fmt, cx))
            }
            Action::SaveBuffer => self.save_buffer(cx),
            Action::ToggleQuickTerminal => crate::quick::toggle(cx),
            Action::RelayFeed => {
                self.splitcommand(&crate::relay::feed_command(), SplitAxis::Vertical, false, window, cx)
            }
            Action::RelayLaunch => crate::agentpicker::open(window, cx),
            Action::RelayLog => {
                self.splitcommand(&crate::relay::log_command(), SplitAxis::Vertical, false, window, cx)
            }
            Action::RelayStart => {
                crate::relay::start(&self.opts);
                crate::relaywatch::start(&self.opts, cx);
                self.refresh_menus_soon(window, cx);
            }
            Action::RelayStop => {
                crate::relay::stop();
                self.refresh_menus_soon(window, cx);
            }
            Action::RelayRestart => {
                crate::relay::restart(&self.opts);
                crate::relaywatch::start(&self.opts, cx);
                self.refresh_menus_soon(window, cx);
            }
            Action::WorktreeCreate(spec) => {
                if let Err(e) = self.worktree_create(&spec, window, cx) {
                    eprintln!("sinclair: worktree create failed: {e}");
                }
            }
            Action::WorktreeOpen(path) => {
                if let Err(e) = self.worktree_open(&path, window, cx) {
                    eprintln!("sinclair: worktree open failed: {e}");
                }
            }
            Action::WorktreeRemove(path) => {
                if let Err(e) = self.worktree_remove(&path, window, cx) {
                    eprintln!("sinclair: worktree remove failed: {e}");
                }
            }
            Action::Tile(id) => self.apply_tile(&id, window, cx),
            Action::SaveLayout => self.open_save_layout(window, cx),
            Action::Sidebar(payload) => self.toggle_sidebar(&payload, cx),
            Action::OpenTeam(name) => self.open_team(&name, window, cx),
            Action::BuildTeam => {
                crate::relay::ensure_running(&self.opts);
                let cwd = self.focused_cwd_path(cx);
                crate::teambuilder::open(window, cwd, cx);
            }
            Action::CheckUpdates => crate::updateui::check_now(cx),
            Action::AgentDef(name) => {
                crate::relay::ensure_running(&self.opts);
                if let Some(cmd) = crate::relay::launch_saved_command(&self.opts, &name) {
                    self.splitcommand(&cmd, SplitAxis::Horizontal, false, window, cx);
                }
            }
            Action::LaunchAgent(provider) => {
                crate::relay::ensure_running(&self.opts);
                let cmd = crate::relay::quick_launch_command(&self.opts, &provider);
                self.splitcommand(&cmd, SplitAxis::Horizontal, false, window, cx);
            }
            Action::OpenWebview(id) => self.open_webview(&id, window, cx),
            Action::ManagePlugins => crate::pluginmanager::open(window, cx),
            Action::Quit => self.request_quit(window, cx),
            Action::Unbound => {}
        }
    }

    fn runplugin(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some((_plugin, command)) = plugin::command(&self.plugins, id) else {
            eprintln!("sinclair: missing plugin command `{id}`");
            return;
        };
        let command = command.clone();
        match command.mode {
            plugin::CommandMode::Pane => {
                self.onfocused(cx, |view, cx| view.run_command(&command.run, cx));
            }
            plugin::CommandMode::Tab => {
                if let Some(id) = self.spawncommand(&command.run, window, cx) {
                    self.group.update(cx, |g, cx| g.add_to_focused(id, cx));
                    self.focusactive(window, cx);
                    cx.notify();
                }
            }
            plugin::CommandMode::SplitRight => {
                self.splitcommand(&command.run, SplitAxis::Horizontal, false, window, cx);
            }
            plugin::CommandMode::SplitDown => {
                self.splitcommand(&command.run, SplitAxis::Vertical, false, window, cx);
            }
        }
    }

    /// Toggle command-macro recording. Starting arms the global recorder;
    /// stopping captures the typed commands and opens the rename modal to name
    /// and save them (an empty capture is discarded).
    fn togglerecord(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let active = cx
            .try_global::<MacroRecorder>()
            .is_some_and(|rec| rec.0.is_active());
        if !active {
            cx.update_global::<MacroRecorder, _>(|rec, _| rec.0.start());
            eprintln!("sinclair: macro recording started; run commands, then trigger macro_record again to save");
            cx.notify();
            return;
        }
        let commands = cx.update_global::<MacroRecorder, _>(|rec, _| rec.0.finish());
        cx.notify();
        if commands.is_empty() {
            eprintln!("sinclair: macro recording stopped: nothing captured");
            return;
        }
        self.open_rename(crate::rename::Target::Macro(commands), String::new(), window, cx);
    }

    /// Persist a recorded macro under `name` (coerced to a safe id), then make
    /// it immediately available. Invoked by the rename modal on commit.
    pub fn save_macro(&mut self, name: &str, commands: Vec<String>, cx: &mut Context<Self>) {
        let Some(name) = macros::sanitize_name(name) else {
            eprintln!("sinclair: macro name `{name}` has no usable characters");
            return;
        };
        let Some(dir) = macros::defaultdir() else {
            eprintln!("sinclair: no config directory for macros");
            return;
        };
        match macros::save(&dir, &macros::Macro::new(name.clone(), commands)) {
            Ok(()) => {
                self.macros = loadmacros();
                eprintln!("sinclair: saved macro `{name}` (bind it with `keybind = ...=macro:{name}`)");
            }
            Err(error) => eprintln!("sinclair: failed to save macro: {error}"),
        }
        cx.notify();
    }

    /// Replay a saved macro into the focused pane.
    pub(crate) fn replay_macro(&mut self, name: &str, cx: &mut Context<Self>) -> Result<(), String> {
        let commands = self
            .macros
            .iter()
            .find(|m| m.name == name)
            .map(|m| m.commands.clone())
            .ok_or_else(|| format!("no macro named `{name}`"))?;
        self.onfocused(cx, |view, cx| view.run_macro(commands, cx));
        Ok(())
    }
}
