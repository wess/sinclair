//! Workspace root: ordered tabs of split panes, one shell per pane.
//!
//! Owns the `workspace::Tabs` model and a map from pane id to terminal
//! view entity. All tab/split mutations funnel through here; the panes
//! themselves only know their own session.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use config::{Action, Keybind, ResizeDir, SplitDirection, SplitFocus};
use futures::StreamExt;
use gpui::prelude::*;
use gpui::{
    div, px, size, AnyElement, App, Context, Entity, Focusable as _, KeyBinding, Menu, MenuItem,
    SharedString, Subscription, WeakEntity, Window,
};
use serde_json::{json, Value};
use terminal::Session;
use workspace::{Axis, Direction, PaneId, PaneIds, Rect, SplitId, Tabs};

use crate::bridge;
use crate::colors::{self, Colors};
use crate::keys;
use crate::metrics::{CellSize, Padding};
use crate::session;
use crate::splits::{self, Drag, SplitsElement};
use crate::view::{TerminalView, ViewEvent};

/// One keybind dispatch: the index into the workspace's resolved keybind
/// table. A single action type keeps every binding flowing through one
/// handler regardless of which config action it carries.
#[derive(Clone, PartialEq, Default, Debug, gpui::Action)]
#[action(namespace = prompt, no_json)]
pub struct RunBind(pub usize);

/// Open the documentation window. Its own action (rather than a `RunBind`) so
/// the Help menu item works without depending on a configured keybind.
#[derive(Clone, PartialEq, Default, Debug, gpui::Action)]
#[action(namespace = prompt, no_json)]
pub struct ShowDocs;

/// Open the About panel. Its own action so the application-menu item works
/// without depending on a configured keybind.
#[derive(Clone, PartialEq, Default, Debug, gpui::Action)]
#[action(namespace = prompt, no_json)]
pub struct ShowAbout;

/// Dispatch a menu item that has no keybind: the index into the workspace's
/// `menu_actions` table built alongside the native menu.
#[derive(Clone, PartialEq, Default, Debug, gpui::Action)]
#[action(namespace = prompt, no_json)]
pub struct MenuPick(pub usize);

/// App-global command-macro recorder. Recording is a single, app-wide mode
/// (one capture at a time), so it lives in a gpui global that the focused
/// pane's key handler feeds and the workspace toggles.
pub struct MacroRecorder(pub macros::Recorder);
impl gpui::Global for MacroRecorder {}

/// Whether typed input is mirrored to every pane in the active tab. A global
/// so the focused pane's `key_down` can check it without a workspace handle.
pub struct Broadcast(pub bool);
impl gpui::Global for Broadcast {}

/// App-global count of panes currently replaying a macro. Tracked in a global
/// (rather than per-workspace state) so a detached replay task can clear it
/// even after its pane is closed.
#[derive(Default)]
pub struct MacroReplays(pub usize);
impl gpui::Global for MacroReplays {}

/// Adjust the in-flight replay count by `delta` and repaint so every
/// workspace's indicator reflects the change.
pub fn replays_changed(cx: &mut App, delta: i32) {
    let count = cx.try_global::<MacroReplays>().map_or(0, |r| r.0);
    let next = (count as i64 + delta as i64).max(0) as usize;
    cx.set_global(MacroReplays(next));
    cx.refresh_windows();
}

/// Grid for a fresh pane until its first layout pass resizes it.
const SPAWN_COLS: usize = 80;
const SPAWN_ROWS: usize = 24;

/// Fraction a divider moves per "Resize Split" step.
const RESIZE_STEP: f32 = 0.05;

struct Pane {
    view: Entity<TerminalView>,
    _subscription: Subscription,
}

pub struct WorkspaceView {
    opts: config::Options,
    colors: Rc<Colors>,
    font: gpui::Font,
    font_size: gpui::Pixels,
    cell: CellSize,
    pad: Padding,
    tabs: Tabs,
    ids: PaneIds,
    panes: HashMap<PaneId, Pane>,
    /// Divider drag in progress, shared with the splits element.
    drag: Rc<RefCell<Option<Drag>>>,
    /// Resolved keybindings (defaults + user config); `RunBind` indexes here.
    keybinds: Vec<Keybind>,
    /// Loaded manifest plugins.
    plugins: Vec<plugin::Plugin>,
    /// Saved command macros, loaded from the macro directory.
    macros: Vec<macros::Macro>,
    /// Config actions for keybind-less menu items, indexed by [`MenuPick`].
    menu_actions: Vec<Action>,
    /// When set, the focused pane fills the tab (Window > Zoom Split).
    zoomed: bool,
    /// Configured font size, restored by `reset_font_size`.
    base_font_size: gpui::Pixels,
    /// Config-file watcher; kept alive so live reload keeps working.
    _watch: Option<config::WatchHandle>,
}

impl WorkspaceView {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        opts: config::Options,
        colors: Rc<Colors>,
        font: gpui::Font,
        font_size: gpui::Pixels,
        cell: CellSize,
        pad: Padding,
        cols: usize,
        rows: usize,
        cwd: Option<std::path::PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let plugins = loadplugins(&opts);
        let (keybinds, diags) = resolvekeys(&opts, &plugins);
        for d in &diags {
            eprintln!("prompt: {}: {}", d.key, d.message);
        }
        // One app-wide recorder, shared across windows. Guard so opening a
        // second window never clobbers a capture in progress.
        if cx.try_global::<MacroRecorder>().is_none() {
            cx.set_global(MacroRecorder(macros::Recorder::new()));
        }
        let mut this = Self {
            base_font_size: font_size,
            opts,
            colors,
            font,
            font_size,
            cell,
            pad,
            // Temporary: replaced right below with the first real pane.
            tabs: Tabs::new(PaneIds::new().next()),
            ids: PaneIds::new(),
            panes: HashMap::new(),
            drag: Rc::new(RefCell::new(None)),
            keybinds,
            plugins,
            macros: loadmacros(),
            menu_actions: Vec::new(),
            zoomed: false,
            _watch: None,
        };
        this.applykeybinds(cx);
        this.setmenus(cx);
        let options = session::options(&this.opts, cols, rows, cwd);
        let Some(id) = this.spawn(options, window, cx) else {
            std::process::exit(1);
        };
        this.tabs = Tabs::new(id);
        this.focusactive(window, cx);
        this.startwatch(window, cx);
        crate::relay::on_launch(&this.opts);
        if this.opts.session_restore {
            this.try_restore(window, cx);
        }
        this
    }

    /// (Re)bind every resolved keybind to a [`RunBind`] carrying its table
    /// index. Triggers with no gpui spelling are skipped. Called at startup
    /// and after a live reload.
    fn applykeybinds(&self, cx: &mut Context<Self>) {
        cx.clear_key_bindings();
        let mut bindings = Vec::new();
        for (i, kb) in self.keybinds.iter().enumerate() {
            let Some(ks) = keys::keystroke(kb.mods, &kb.key) else {
                continue;
            };
            if gpui::Keystroke::parse(&ks).is_err() {
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
    fn setmenus(&mut self, cx: &mut Context<Self>) {
        // Actions for menu items that have no keybind, indexed by `MenuPick`.
        let mut actions: Vec<Action> = Vec::new();
        let mut menus = vec![
            self.prompt_menu(&mut actions),
            self.shell_menu(&mut actions, cx),
            self.edit_menu(&mut actions),
            self.view_menu(&mut actions, cx),
            self.workspace_menu(&mut actions, cx),
            self.window_menu(&mut actions),
        ];
        if self.opts.ai_enabled {
            menus.push(self.ai_menu(&mut actions));
        }
        // macOS inserts the "Search" field at the top of any menu named
        // "Help" automatically; "Documents" opens the docs window.
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
    fn refresh_menus_soon(&self, window: &mut Window, cx: &mut Context<Self>) {
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

    fn ai_menu(&self, a: &mut Vec<Action>) -> Menu {
        let mut items: Vec<Option<MenuItem>> = Vec::new();
        if crate::relay::enabled(&self.opts) {
            items.push(Some(MenuItem::submenu(self.agents_submenu(a))));
            items.push(Some(MenuItem::submenu(self.relay_submenu(a))));
            items.push(self.pick(a, "Open Feed", Action::RelayFeed));
            let teams = crate::relay::team_list();
            if !teams.is_empty() {
                let mut t: Vec<Option<MenuItem>> = Vec::new();
                for name in teams {
                    t.push(self.pick(a, &name, Action::OpenTeam(name.clone())));
                }
                items.push(Some(MenuItem::separator()));
                items.push(Some(MenuItem::submenu(Self::menu("Teams", t))));
            }
        } else {
            items.push(self.pick(a, "Enable Relay in Settings\u{2026}", Action::ToggleSettings));
        }
        Self::menu("AI", items)
    }

    fn agents_submenu(&self, a: &mut Vec<Action>) -> Menu {
        let mut items: Vec<Option<MenuItem>> = vec![self.pick(a, "Define Agent\u{2026}", Action::RelayLaunch)];
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
                self.pick(a, "Start Server", Action::RelayStart),
                self.pick(a, "Stop Server", Action::RelayStop),
                self.pick(a, "Restart Server", Action::RelayRestart),
                Some(MenuItem::separator()),
                self.pick(a, "View Logs", Action::RelayLog),
            ],
        )
    }

    /// A greyed-out, non-interactive informational menu row.
    fn status_item(label: &str) -> MenuItem {
        MenuItem::Action {
            name: label.to_string().into(),
            // Disabled, so this index is never dispatched; out-of-range is a
            // no-op in `menupick` regardless.
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
            "Prompt",
            vec![
                Some(MenuItem::action("About Prompt", ShowAbout)),
                Some(MenuItem::separator()),
                self.pick(a, "Command Palette\u{2026}", Action::CommandPalette),
                self.pick(a, "Settings\u{2026}", Action::ToggleSettings),
                Some(MenuItem::separator()),
                self.pick(a, "Quit Prompt", Action::Quit),
            ],
        )
    }

    fn shell_menu(&self, a: &mut Vec<Action>, cx: &App) -> Menu {
        let recording = self
            .panes
            .get(&self.tabs.focused())
            .is_some_and(|p| p.view.read(cx).is_recording());
        Self::menu(
            "File",
            vec![
                self.pick(a, "New Window", Action::NewWindow),
                self.pick(a, "New Tab", Action::NewTab),
                Some(MenuItem::separator()),
                Some(self.pick_checked(a, "Record Session", Action::ToggleRecording, recording)),
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
            .panes
            .get(&self.tabs.focused())
            .is_some_and(|p| p.view.read(cx).is_read_only());
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
                self.pick(a, "Quick Terminal", Action::ToggleQuickTerminal),
            ],
        )
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
        // Tile presets (formerly View > Tiles), inlined for quicker reach.
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

    /// Window menu. Tab navigation is listed explicitly since Prompt does not
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

    /// Watch the config file and reload appearance on every edit.
    fn startwatch(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((handle, mut changes)) = crate::reload::watch() else {
            return;
        };
        self._watch = Some(handle);
        let weak = cx.weak_entity();
        window
            .spawn(cx, async move |cx| {
                while changes.next().await.is_some() {
                    if weak.update(cx, |this, cx| this.reload(cx)).is_err() {
                        break;
                    }
                }
            })
            .detach();
    }

    /// Re-read the config file and apply everything that can change at
    /// runtime: theme/colors, font family/size, padding, cursor style,
    /// copy-on-select. Shell, scrollback and window size only affect new
    /// sessions or need a restart.
    fn reload(&mut self, cx: &mut Context<Self>) {
        let (opts, diagnostics) = config::load();
        for d in &diagnostics {
            eprintln!("prompt: config line {}: {} ({})", d.line, d.message, d.key);
        }
        self.colors = Rc::new(colors::from_config(&opts));
        self.font = crate::font::build(&opts);
        self.font_size = px(opts.font_size.max(1.0));
        self.cell = crate::metrics::measure(cx.text_system(), &self.font, self.font_size);
        self.pad = Padding {
            x: opts.window_padding_x as f32,
            y: opts.window_padding_y as f32,
        };
        self.base_font_size = self.font_size;
        self.opts = opts;
        self.plugins = loadplugins(&self.opts);
        self.macros = loadmacros();
        let (keybinds, diags) = resolvekeys(&self.opts, &self.plugins);
        for d in &diags {
            eprintln!("prompt: {}: {}", d.key, d.message);
        }
        self.keybinds = keybinds;
        self.applykeybinds(cx);
        self.setmenus(cx);
        self.pushappearance(cx);
        crate::relay::on_reload(&self.opts);
        cx.notify();
    }

    /// Push the current appearance to every pane.
    fn pushappearance(&self, cx: &mut Context<Self>) {
        let appearance = crate::view::Appearance {
            colors: self.colors.clone(),
            font: self.font.clone(),
            font_size: self.font_size,
            cell: self.cell,
            pad: self.pad,
            cursor_default: self.opts.cursor_style,
            copy_on_select: self.opts.copy_on_select,
            option_as_alt: self.opts.macos_option_as_alt,
            paste_protection: self.opts.clipboard_paste_protection,
        };
        for pane in self.panes.values() {
            pane.view
                .update(cx, |view, cx| view.set_appearance(&appearance, cx));
        }
    }

    /// Re-measure the cell box for the current font size and republish.
    fn setfontsize(&mut self, size: gpui::Pixels, cx: &mut Context<Self>) {
        let size = px(f32::from(size).max(1.0));
        if size == self.font_size {
            return;
        }
        self.font_size = size;
        self.cell = crate::metrics::measure(cx.text_system(), &self.font, self.font_size);
        self.pushappearance(cx);
        cx.notify();
    }

    /// Spawn a session, wrap it in a pane view, wire its event bridge and
    /// subscription, and register it. `None` if the shell failed to spawn.
    fn spawn(
        &mut self,
        options: terminal::SessionOptions,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<PaneId> {
        let (session, events) = match Session::spawn(options) {
            Ok(pair) => pair,
            Err(error) => {
                eprintln!("prompt: failed to spawn shell: {error}");
                return None;
            }
        };
        let session = Arc::new(session);
        let fallback = session::shellname(self.opts.shell.as_deref());
        let view = cx.new(|cx| {
            TerminalView::new(
                session,
                self.colors.clone(),
                self.font.clone(),
                self.font_size,
                self.cell,
                self.pad,
                self.opts.cursor_style,
                self.opts.copy_on_select,
                self.opts.macos_option_as_alt,
                self.opts.clipboard_paste_protection,
                fallback,
                window,
                cx,
            )
        });

        // Pump session events into the pane view on the foreground.
        let weak = view.downgrade();
        let mut events = bridge::forward(events);
        window
            .spawn(cx, async move |cx| {
                while let Some(event) = events.next().await {
                    if weak.update(cx, |view, cx| view.apply(event, cx)).is_err() {
                        break;
                    }
                }
            })
            .detach();

        let id = self.ids.next();
        let subscription = cx.subscribe_in(
            &view,
            window,
            move |this: &mut Self, _view, event: &ViewEvent, window, cx| {
                this.paneevent(id, event, window, cx);
            },
        );
        self.panes.insert(
            id,
            Pane {
                view,
                _subscription: subscription,
            },
        );
        Some(id)
    }

    /// Spawn a pane inheriting the focused pane's working directory.
    fn spawnpane(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<PaneId> {
        let inherit = self
            .panes
            .get(&self.tabs.focused())
            .and_then(|pane| pane.view.read(cx).cwd())
            .and_then(|osc| session::cwdpath(&osc));
        let options = session::options(&self.opts, SPAWN_COLS, SPAWN_ROWS, inherit);
        self.spawn(options, window, cx)
    }

    fn paneevent(
        &mut self,
        pane: PaneId,
        event: &ViewEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            ViewEvent::Title => {
                if pane == self.tabs.focused() {
                    self.settitle(window, cx);
                }
                cx.notify(); // tab labels
            }
            ViewEvent::Exited => self.closepane(pane, window, cx),
            ViewEvent::Input(bytes) => self.broadcast(pane, bytes, cx),
            ViewEvent::Action(action) => {
                // A right-click menu pick targets the pane it was opened in.
                self.tabs.focus(pane);
                self.focusactive(window, cx);
                self.dispatch(action.clone(), window, cx);
            }
            // A pane raised/cleared its attention flag; repaint the tab strip.
            ViewEvent::Attention => cx.notify(),
        }
    }

    /// Mirror `bytes` (already encoded by the source pane) to every other pane
    /// in the active tab. The source already wrote them to its own pty.
    fn broadcast(&mut self, source: PaneId, bytes: &[u8], cx: &mut Context<Self>) {
        for id in self.tabs.active().tree.panes() {
            if id == source {
                continue;
            }
            if let Some(pane) = self.panes.get(&id) {
                pane.view.update(cx, |view, cx| view.send_text(bytes, cx));
            }
        }
    }

    /// Toggle broadcast input. Repaints panes (for the indicator) and rebuilds
    /// menus (for the checkmark).
    fn toggle_broadcast(&mut self, cx: &mut Context<Self>) {
        let on = cx.try_global::<Broadcast>().is_some_and(|b| b.0);
        cx.set_global(Broadcast(!on));
        self.setmenus(cx);
        for pane in self.panes.values() {
            pane.view.update(cx, |_v, cx| cx.notify());
        }
        cx.notify();
    }

    /// Close one pane: collapse its split, or close its tab when it is the
    /// last pane there, or quit when it is the last pane of the last tab.
    fn closepane(&mut self, pane: PaneId, window: &mut Window, cx: &mut Context<Self>) {
        self.zoomed = false;
        let Some(index) = self.tabindex(pane) else {
            return;
        };
        let lastpane = self.tabs.get(index).expect("tab").tree.panes().len() == 1;
        if lastpane && self.tabs.len() == 1 {
            self.close_window(window, cx);
            return;
        }
        if lastpane {
            self.tabs.close_tab(index);
        } else {
            // Mutations go through the active tab; visit and restore.
            let previous = self.tabs.active_index();
            self.tabs.activate(index);
            let next = (self.tabs.focused() == pane)
                .then(|| workspace::next(&self.tabs.active().tree, pane))
                .flatten();
            self.tabs.active_mut().tree.remove(pane);
            if let Some(next) = next {
                self.tabs.focus(next);
            }
            self.tabs.activate(previous);
        }
        self.panes.remove(&pane);
        self.focusactive(window, cx);
        cx.notify();
    }

    /// Close a whole tab (tab-bar close glyph), dropping all its panes.
    pub fn closetab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tab) = self.tabs.get(index) else {
            return;
        };
        let removed = tab.tree.panes();
        if self.tabs.len() == 1 {
            self.close_window(window, cx);
            return;
        }
        self.tabs.close_tab(index);
        for pane in removed {
            self.panes.remove(&pane);
        }
        self.focusactive(window, cx);
        cx.notify();
    }

    /// Close just this window. The app keeps running while other windows are
    /// open; only when this is the last window do we honor
    /// `quit-after-last-window-closed` (macOS keeps the app alive otherwise).
    fn close_window(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.save_state(cx);
        let last_window = cx.windows().len() <= 1;
        if last_window && self.opts.quit_after_last_window_closed {
            cx.quit();
        } else {
            window.remove_window();
        }
    }

    pub fn activatetab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.activate(index) {
            self.zoomed = false;
            self.focusactive(window, cx);
            cx.notify();
        }
    }

    pub fn focuspane(&mut self, pane: PaneId, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.focus(pane) {
            self.focusactive(window, cx);
            cx.notify();
        }
    }

    /// Set a divider's ratio in the active tab (divider drag).
    pub fn setratio(&mut self, split: SplitId, ratio: f32, cx: &mut Context<Self>) {
        if self.tabs.active_mut().tree.set_ratio(split, ratio) {
            cx.notify();
        }
    }

    /// Reset every divider in the active tab to an even split.
    fn equalizesplits(&mut self, cx: &mut Context<Self>) {
        let dividers = self.tabs.active().tree.list_dividers();
        if dividers.is_empty() {
            return;
        }
        let tree = &mut self.tabs.active_mut().tree;
        for (split, _) in dividers {
            tree.set_ratio(split, 0.5);
        }
        cx.notify();
    }

    /// Nudge the divider adjacent to the focused pane in a direction.
    fn resizesplit(&mut self, dir: ResizeDir, cx: &mut Context<Self>) {
        let (axis, delta) = match dir {
            ResizeDir::Left => (Axis::Horizontal, -RESIZE_STEP),
            ResizeDir::Right => (Axis::Horizontal, RESIZE_STEP),
            ResizeDir::Up => (Axis::Vertical, -RESIZE_STEP),
            ResizeDir::Down => (Axis::Vertical, RESIZE_STEP),
        };
        let focused = self.tabs.focused();
        let tree = &mut self.tabs.active_mut().tree;
        let Some(split) = tree.nearest_split(focused, axis) else {
            return;
        };
        if let Some(current) = tree.ratio(split) {
            tree.set_ratio(split, current + delta);
            cx.notify();
        }
    }

    /// Resize the window back to the configured default cell grid.
    fn returntodefaultsize(&self, window: &mut Window) {
        let cols = if self.opts.window_width > 0 {
            self.opts.window_width as usize
        } else {
            SPAWN_COLS
        };
        let rows = if self.opts.window_height > 0 {
            self.opts.window_height as usize
        } else {
            SPAWN_ROWS
        };
        let (width, height) = crate::metrics::pixel_size(cols, rows, self.pad, self.cell);
        window.resize(size(px(width), px(height)));
    }

    /// Persist the focused pane's current cell grid as the default size.
    fn useasdefault(&mut self, cx: &mut Context<Self>) {
        let Some((cols, rows)) = self
            .panes
            .get(&self.tabs.focused())
            .map(|p| p.view.read(cx).grid_size())
        else {
            return;
        };
        write_config("window-width", &cols.to_string());
        write_config("window-height", &rows.to_string());
    }

    /// Open the rename window for the active tab or the focused pane.
    fn changetitle(&mut self, tab: bool, window: &mut Window, cx: &mut Context<Self>) {
        let root = cx.weak_entity();
        if tab {
            let index = self.tabs.active_index();
            let initial = self.tabs.active().title.clone().unwrap_or_default();
            crate::rename::open(window, root, crate::rename::Target::Tab(index), initial, cx);
        } else {
            let pane = self.tabs.focused();
            let initial = self
                .panes
                .get(&pane)
                .map(|p| p.view.read(cx).title().to_string())
                .unwrap_or_default();
            crate::rename::open(window, root, crate::rename::Target::Pane(pane), initial, cx);
        }
    }

    /// Set the active-tab label override (called back from the rename window).
    pub fn rename_tab(&mut self, index: usize, title: &str, cx: &mut Context<Self>) {
        let trimmed = title.trim();
        let value = (!trimmed.is_empty()).then(|| trimmed.to_string());
        if self.tabs.set_title(index, value) {
            cx.notify();
        }
    }

    /// Set a pane's title override (called back from the rename window).
    pub fn rename_pane(&mut self, pane: PaneId, title: &str, cx: &mut Context<Self>) {
        if let Some(p) = self.panes.get(&pane) {
            p.view
                .update(cx, |view, cx| view.set_title_override(title, cx));
        }
    }

    /// Split the focused pane. `first` places the new pane before the
    /// existing one (left/up) instead of after it (right/down).
    fn split(&mut self, axis: Axis, first: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.zoomed = false;
        let target = self.tabs.focused();
        let Some(id) = self.spawnpane(window, cx) else {
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

    fn focusdir(&mut self, direction: Direction, window: &mut Window, cx: &mut Context<Self>) {
        // Directional nav only needs relative geometry, so the viewport
        // rect is close enough without the exact splits bounds.
        let viewport = window.viewport_size();
        let rect = Rect::new(
            0.0,
            0.0,
            f32::from(viewport.width).max(1.0),
            f32::from(viewport.height).max(1.0),
        );
        let layout = workspace::compute_layout(&self.tabs.active().tree, rect, splits::DIVIDER);
        if let Some(next) = workspace::neighbor(&layout, self.tabs.focused(), direction) {
            self.focuspane(next, window, cx);
        }
    }

    /// Move window focus to the active tab's focused pane and retitle.
    fn focusactive(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(pane) = self.panes.get(&self.tabs.focused()) {
            window.focus(&pane.view.focus_handle(cx), cx);
        }
        self.settitle(window, cx);
    }

    fn settitle(&self, window: &mut Window, cx: &App) {
        let title = self
            .panes
            .get(&self.tabs.focused())
            .map(|pane| pane.view.read(cx).title().to_string())
            .unwrap_or_else(|| "prompt".to_string());
        window.set_window_title(&title);
    }

    fn tabindex(&self, pane: PaneId) -> Option<usize> {
        (0..self.tabs.len()).find(|i| self.tabs.get(*i).is_some_and(|t| t.tree.contains(pane)))
    }

    /// One label per tab: its override when set, else its focused pane's title.
    fn titles(&self, cx: &App) -> Vec<String> {
        (0..self.tabs.len())
            .map(|i| {
                let tab = self.tabs.get(i).expect("tab index");
                if let Some(title) = &tab.title {
                    return title.clone();
                }
                self.panes
                    .get(&tab.focused)
                    .map(|pane| pane.view.read(cx).title().to_string())
                    .unwrap_or_default()
            })
            .collect()
    }

    /// Per-tab strip data: the label and whether any pane in the tab has a
    /// pending notification.
    fn tab_infos(&self, cx: &App) -> Vec<crate::tabbar::TabInfo> {
        (0..self.tabs.len())
            .map(|i| {
                let tab = self.tabs.get(i).expect("tab index");
                let focused = self.panes.get(&tab.focused).map(|p| p.view.read(cx));
                let title = match &tab.title {
                    Some(t) => t.clone(),
                    None => focused
                        .as_ref()
                        .map(|v| {
                            if self.opts.tab_title_show_host {
                                v.title().to_string()
                            } else {
                                strip_user_host(v.title()).to_string()
                            }
                        })
                        .unwrap_or_default(),
                };
                let attention = tab.tree.panes().iter().any(|id| {
                    self.panes
                        .get(id)
                        .is_some_and(|p| p.view.read(cx).needs_attention())
                });
                crate::tabbar::TabInfo { title, attention }
            })
            .collect()
    }

    pub fn newtab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.zoomed = false;
        if let Some(id) = self.spawnpane(window, cx) {
            self.tabs.new_tab(id);
            self.focusactive(window, cx);
            cx.notify();
        }
    }

    /// Open another top-level window, cloning this window's current
    /// appearance so the new one matches without re-reading config.
    fn newwindow(&self, cx: &mut Context<Self>) {
        // Open the new window in the focused pane's directory, like new tabs.
        let cwd = self
            .panes
            .get(&self.tabs.focused())
            .and_then(|pane| pane.view.read(cx).cwd())
            .and_then(|osc| session::cwdpath(&osc));
        crate::open_window(
            self.opts.clone(),
            self.colors.clone(),
            self.font.clone(),
            self.font_size,
            self.cell,
            self.pad,
            cwd,
            cx,
        );
    }

    /// Cycle focus to the previous/next pane in the active tab's layout.
    fn cyclesplit(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        let focused = self.tabs.focused();
        let tree = &self.tabs.active().tree;
        let next = if forward {
            workspace::next(tree, focused)
        } else {
            workspace::prev(tree, focused)
        };
        if let Some(next) = next {
            self.focuspane(next, window, cx);
        }
    }

    /// Activate a 1-based tab index; negative counts from the end.
    fn gototab(&mut self, n: i32, window: &mut Window, cx: &mut Context<Self>) {
        let len = self.tabs.len() as i32;
        let index = if n < 0 { len + n } else { n - 1 };
        if (0..len).contains(&index) {
            self.activatetab(index as usize, window, cx);
        }
    }

    /// Reorder the active tab by a signed delta, clamped to the ends.
    fn movetab(&mut self, delta: i32, cx: &mut Context<Self>) {
        let from = self.tabs.active_index();
        let len = self.tabs.len() as i32;
        let to = (from as i32 + delta).clamp(0, len - 1) as usize;
        if self.tabs.move_tab(from, to) {
            cx.notify();
        }
    }

    /// Run something on the focused pane's view.
    fn onfocused(
        &self,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut TerminalView, &mut Context<TerminalView>),
    ) {
        if let Some(pane) = self.panes.get(&self.tabs.focused()) {
            pane.view.update(cx, |view, cx| f(view, cx));
        }
    }

    /// Dispatch handler shared by every keybinding.
    fn runbind(&mut self, action: &RunBind, window: &mut Window, cx: &mut Context<Self>) {
        let Some(kb) = self.keybinds.get(action.0) else {
            return;
        };
        self.dispatch(kb.action.clone(), window, cx);
    }

    /// Open the documentation window (Help menu).
    fn showdocs(&mut self, _: &ShowDocs, window: &mut Window, cx: &mut Context<Self>) {
        crate::help::open(window, cx);
    }

    /// Open the About panel (application menu).
    fn showabout(&mut self, _: &ShowAbout, window: &mut Window, cx: &mut Context<Self>) {
        crate::about::open(window, cx);
    }

    /// Dispatch a keybind-less menu item via its `menu_actions` index.
    fn menupick(&mut self, action: &MenuPick, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(config_action) = self.menu_actions.get(action.0).cloned() {
            self.dispatch(config_action, window, cx);
        }
    }

    /// Run an action from outside the keymap (the command palette).
    pub fn run_action(&mut self, action: Action, window: &mut Window, cx: &mut Context<Self>) {
        self.dispatch(action, window, cx);
    }

    /// Open the command palette over the curated action catalog, each entry
    /// labeled and tagged with its current keybind.
    fn open_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(handle) = window.window_handle().downcast::<WorkspaceView>() else {
            return;
        };
        let items = palette_catalog()
            .into_iter()
            .map(|(label, action)| {
                let shortcut = self
                    .keybinds
                    .iter()
                    .find(|k| k.action == action)
                    .and_then(|k| keys::shortcut_glyphs(k.mods, &k.key));
                crate::palette::Item {
                    label: label.to_string(),
                    shortcut,
                    action,
                }
            })
            .collect();
        crate::palette::open(window, handle, items, cx);
    }

    /// Carry out one config action.
    fn dispatch(&mut self, action: Action, window: &mut Window, cx: &mut Context<Self>) {
        match action {
            Action::NewWindow => self.newwindow(cx),
            Action::NewTab => self.newtab(window, cx),
            Action::CloseSurface => self.closepane(self.tabs.focused(), window, cx),
            Action::CloseTab => self.closetab(self.tabs.active_index(), window, cx),
            Action::CloseWindow => self.close_window(window, cx),
            Action::CloseAllWindows => {
                for handle in cx.windows() {
                    handle
                        .update(cx, |_, window, _| window.remove_window())
                        .ok();
                }
            }
            Action::NewSplit(dir) => {
                let (axis, first) = match dir {
                    SplitDirection::Right => (Axis::Horizontal, false),
                    SplitDirection::Left => (Axis::Horizontal, true),
                    SplitDirection::Down => (Axis::Vertical, false),
                    SplitDirection::Up => (Axis::Vertical, true),
                };
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
            Action::ZoomSplit => {
                self.zoomed = !self.zoomed;
                cx.notify();
            }
            Action::EqualizeSplits => self.equalizesplits(cx),
            Action::ResizeSplit(dir) => self.resizesplit(dir, cx),
            Action::GotoTab(n) => self.gototab(n, window, cx),
            Action::PreviousTab => {
                self.tabs.activate_prev();
                self.focusactive(window, cx);
                cx.notify();
            }
            Action::NextTab => {
                self.tabs.activate_next();
                self.focusactive(window, cx);
                cx.notify();
            }
            Action::MoveTab(delta) => self.movetab(delta, cx),
            Action::Copy => self.onfocused(cx, |v, cx| v.copy_selection(cx)),
            Action::Paste => self.onfocused(cx, |v, cx| v.paste_clipboard(cx)),
            Action::SelectAll => self.onfocused(cx, |v, cx| v.select_all(cx)),
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
                    eprintln!("prompt: {error}");
                }
            }
            Action::CommandPalette => self.open_palette(window, cx),
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
                // Rebuild menus so the read-only checkmark reflects the change.
                self.setmenus(cx);
            }
            Action::ToggleBroadcast => self.toggle_broadcast(cx),
            Action::ToggleRecording => {
                self.onfocused(cx, |v, cx| v.toggle_recording(cx));
                // Refresh the Shell menu checkmark.
                self.setmenus(cx);
            }
            Action::ToggleQuickTerminal => crate::quick::toggle(cx),
            Action::RelayFeed => {
                self.splitcommand(&crate::relay::feed_command(), Axis::Vertical, false, window, cx)
            }
            Action::RelayLaunch => {
                let providers = crate::relay::enabled_agents(&self.opts);
                let roles = crate::relay::role_list();
                if let Some(handle) = window.window_handle().downcast::<WorkspaceView>() {
                    crate::newagent::open(window, handle, providers, roles, cx);
                }
            }
            Action::RelayLog => {
                self.splitcommand(&crate::relay::log_command(), Axis::Vertical, false, window, cx)
            }
            Action::RelayStart => {
                crate::relay::start(&self.opts);
                self.refresh_menus_soon(window, cx);
            }
            Action::RelayStop => {
                crate::relay::stop();
                self.refresh_menus_soon(window, cx);
            }
            Action::RelayRestart => {
                crate::relay::restart(&self.opts);
                self.refresh_menus_soon(window, cx);
            }
            Action::Tile(id) => self.apply_tile(&id, window, cx),
            Action::SaveLayout => self.open_save_layout(window, cx),
            Action::OpenTeam(name) => self.open_team(&name, window, cx),
            Action::AgentDef(name) => {
                crate::relay::ensure_running(&self.opts);
                if let Some(cmd) = crate::relay::launch_saved_command(&name) {
                    self.splitcommand(&cmd, Axis::Horizontal, false, window, cx);
                }
            }
            Action::Quit => self.request_quit(window, cx),
            Action::Unbound => {}
        }
    }

    fn runplugin(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some((_plugin, command)) = plugin::command(&self.plugins, id) else {
            eprintln!("prompt: missing plugin command `{id}`");
            return;
        };
        let command = command.clone();
        match command.mode {
            plugin::CommandMode::Pane => {
                self.onfocused(cx, |view, cx| view.run_command(&command.run, cx));
            }
            plugin::CommandMode::Tab => {
                if let Some(id) = self.spawncommand(&command.run, window, cx) {
                    self.tabs.new_tab(id);
                    self.focusactive(window, cx);
                    cx.notify();
                }
            }
            plugin::CommandMode::SplitRight => {
                self.splitcommand(&command.run, Axis::Horizontal, false, window, cx);
            }
            plugin::CommandMode::SplitDown => {
                self.splitcommand(&command.run, Axis::Vertical, false, window, cx);
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
            eprintln!("prompt: macro recording started; run commands, then trigger macro_record again to save");
            cx.notify();
            return;
        }
        let commands = cx.update_global::<MacroRecorder, _>(|rec, _| rec.0.finish());
        cx.notify();
        if commands.is_empty() {
            eprintln!("prompt: macro recording stopped: nothing captured");
            return;
        }
        let root = cx.weak_entity();
        crate::rename::open_macro(window, root, commands, cx);
    }

    /// Persist a recorded macro under `name` (coerced to a safe id), then make
    /// it immediately available. Invoked by the rename modal on commit.
    pub fn save_macro(&mut self, name: &str, commands: Vec<String>, cx: &mut Context<Self>) {
        let Some(name) = macros::sanitize_name(name) else {
            eprintln!("prompt: macro name `{name}` has no usable characters");
            return;
        };
        let Some(dir) = macros::defaultdir() else {
            eprintln!("prompt: no config directory for macros");
            return;
        };
        match macros::save(&dir, &macros::Macro::new(name.clone(), commands)) {
            Ok(()) => {
                self.macros = loadmacros();
                eprintln!("prompt: saved macro `{name}` (bind it with `keybind = ...=macro:{name}`)");
            }
            Err(error) => eprintln!("prompt: failed to save macro: {error}"),
        }
        cx.notify();
    }

    /// Replay a saved macro into the focused pane.
    fn replay_macro(&mut self, name: &str, cx: &mut Context<Self>) -> Result<(), String> {
        let commands = self
            .macros
            .iter()
            .find(|m| m.name == name)
            .map(|m| m.commands.clone())
            .ok_or_else(|| format!("no macro named `{name}`"))?;
        self.onfocused(cx, |view, cx| view.run_macro(commands, cx));
        Ok(())
    }

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

    fn spawncommand(
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

    fn splitcommand(
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
    fn spawn_pane(
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
    fn apply_layout(
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
        // The tab is the workspace; label it so it reads "web", not the shell.
        if let Some(t) = title {
            let idx = self.tabs.active_index();
            self.rename_tab(idx, t, cx);
        }
        self.focusactive(window, cx);
        cx.notify();
    }

    /// Spawn a pane rooted at `cwd` (or the configured default when `None`).
    fn spawn_cwd(
        &mut self,
        cwd: Option<std::path::PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<PaneId> {
        let options = session::options(&self.opts, SPAWN_COLS, SPAWN_ROWS, cwd);
        self.spawn(options, window, cx)
    }

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
    fn try_restore(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
        // The restored tabs were appended after the launch default at index 0.
        self.closetab(0, window, cx);
        let active = state.active.min(self.tabs.len().saturating_sub(1));
        self.activatetab(active, window, cx);
    }

    /// Quit, but warn first when a process is still running in a pane and
    /// `confirm-quit` is on. The native dialog runs async; we quit only if the
    /// user confirms.
    fn request_quit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
    fn save_state(&self, cx: &App) {
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
    fn realize_into(
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
    fn apply_tile(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
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
    fn open_team(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
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
            .map(|(m, role)| Some(crate::relay::launch_member(m, role)))
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
    fn open_save_layout(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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

impl Render for WorkspaceView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tree = self.tabs.active().tree.clone();
        let focused = self.tabs.focused();
        let multi = tree.panes().len() > 1;
        let children: Vec<(PaneId, AnyElement)> = tree
            .panes()
            .into_iter()
            .filter_map(|id| {
                self.panes
                    .get(&id)
                    .map(|pane| (id, pane.view.clone().into_any_element()))
            })
            .collect();
        let mut dividercolor = colors::hsla(self.colors.fg);
        dividercolor.a = 0.2;
        // Non-focused panes recede by a wash of the background; how far is the
        // `unfocused-split-opacity` setting (opacity o ⇒ 1-o of background).
        let mut dimcolor = colors::hsla(self.colors.bg);
        dimcolor.a = (1.0 - self.opts.unfocused_split_opacity).clamp(0.0, 1.0);
        let root: WeakEntity<Self> = cx.weak_entity();
        let splitselement = SplitsElement::new(
            tree,
            focused,
            children,
            dividercolor,
            dimcolor,
            self.drag.clone(),
            root,
        );

        let mut base = div()
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(colors::rgba(self.colors.bg))
            .key_context("Workspace")
            .on_action(cx.listener(Self::runbind))
            .on_action(cx.listener(Self::showdocs))
            .on_action(cx.listener(Self::showabout))
            .on_action(cx.listener(Self::menupick));

        // The custom titlebar replaces the native one and folds the tabs in,
        // so it is always present (the window opens with no native chrome).
        let tab_infos = self.tab_infos(cx);
        base = base.child(crate::titlebar::bar(
            &tab_infos,
            self.tabs.active_index(),
            &self.colors,
            &self.font,
            self.font_size,
            window,
            cx,
        ));

        // Zoom Split: when active with more than one pane, the focused pane
        // fills the area in place of the split layout.
        let content: AnyElement = if self.zoomed && multi {
            match self.panes.get(&focused) {
                Some(pane) => pane.view.clone().into_any_element(),
                None => splitselement.into_any_element(),
            }
        } else {
            splitselement.into_any_element()
        };
        base = base.child(div().w_full().flex_1().min_h(px(0.0)).child(content));

        // Floating indicator while a macro is recording or replaying.
        let recording = cx
            .try_global::<MacroRecorder>()
            .is_some_and(|rec| rec.0.is_active());
        let replaying = cx.try_global::<MacroReplays>().is_some_and(|r| r.0 > 0);
        if let Some(pill) = macro_pill(recording, replaying, &self.colors) {
            base = base.child(pill);
        }

        // Floating indicator while broadcast input is on — typed keys reach
        // every pane in the tab, so make that unmissable.
        if cx.try_global::<Broadcast>().is_some_and(|b| b.0) {
            base = base.child(broadcast_pill(&self.colors));
        }

        // Floating indicator while any pane is recording a cast.
        if self
            .panes
            .values()
            .any(|p| p.view.read(cx).is_recording())
        {
            base = base.child(recording_pill(&self.colors));
        }

        // Client-side decorations (Linux) need app-drawn resize edges.
        #[cfg(target_os = "linux")]
        if matches!(window.window_decorations(), gpui::Decorations::Client { .. }) {
            base = base.child(crate::titlebar::resize_handles());
        }

        base
    }
}

/// Upsert one `key = value` line into the user's config file, creating it if
/// needed. Shared shape with the settings panel's writer.
fn write_config(key: &str, value: &str) {
    let Some(path) = config::default_path() else {
        return;
    };
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let updated = config::upsert(&text, key, value);
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(&path, updated);
}

fn loadplugins(opts: &config::Options) -> Vec<plugin::Plugin> {
    let (plugins, diags) = plugin::load(&opts.plugin);
    for d in &diags {
        if d.line == 0 {
            eprintln!("prompt: plugin {}: {}", d.path.display(), d.message);
        } else {
            eprintln!(
                "prompt: plugin {} line {}: {}",
                d.path.display(),
                d.line,
                d.message
            );
        }
    }
    plugins
}

/// Strip a leading `user@host:` prefix from a shell-set title, leaving just the
/// path. Shells default the terminal title to `\u@\h: \w`, which wraps to two
/// lines in a tab and reads poorly; the host is noise in a tab label.
fn strip_user_host(title: &str) -> &str {
    if let Some((head, rest)) = title.split_once(':') {
        if head.contains('@') && !head.contains([' ', '/']) {
            return rest.trim_start();
        }
    }
    title
}

/// The curated set of actions the command palette offers, with display
/// labels. Ordered roughly by how often they're reached.
fn palette_catalog() -> Vec<(&'static str, Action)> {
    vec![
        ("New Window", Action::NewWindow),
        ("New Tab", Action::NewTab),
        ("Close Pane", Action::CloseSurface),
        ("Close Tab", Action::CloseTab),
        ("Close Window", Action::CloseWindow),
        ("Split Right", Action::NewSplit(SplitDirection::Right)),
        ("Split Left", Action::NewSplit(SplitDirection::Left)),
        ("Split Down", Action::NewSplit(SplitDirection::Down)),
        ("Zoom Split", Action::ZoomSplit),
        ("Equalize Splits", Action::EqualizeSplits),
        ("Select Split Up", Action::GotoSplit(SplitFocus::Up)),
        ("Select Split Down", Action::GotoSplit(SplitFocus::Down)),
        ("Select Split Left", Action::GotoSplit(SplitFocus::Left)),
        ("Select Split Right", Action::GotoSplit(SplitFocus::Right)),
        ("Broadcast Input", Action::ToggleBroadcast),
        ("Save Current Layout", Action::SaveLayout),
        ("Previous Tab", Action::PreviousTab),
        ("Next Tab", Action::NextTab),
        ("Copy", Action::Copy),
        ("Paste", Action::Paste),
        ("Select All", Action::SelectAll),
        ("Find", Action::ToggleSearch),
        ("Semantic Find", Action::ToggleSemanticSearch),
        ("Explain Output", Action::ExplainOutput),
        ("Compose Command", Action::ComposeCommand),
        ("Clear Screen", Action::ClearScreen),
        ("Jump to Previous Prompt", Action::JumpToPrompt(-1)),
        ("Jump to Next Prompt", Action::JumpToPrompt(1)),
        ("Increase Font Size", Action::IncreaseFontSize(1.0)),
        ("Decrease Font Size", Action::DecreaseFontSize(1.0)),
        ("Reset Font Size", Action::ResetFontSize),
        ("Change Tab Title", Action::ChangeTabTitle),
        ("Change Terminal Title", Action::ChangeTerminalTitle),
        ("Terminal Read-only", Action::ToggleReadOnly),
        ("Toggle Full Screen", Action::ToggleFullscreen),
        ("Quick Terminal", Action::ToggleQuickTerminal),
        ("Record Macro", Action::MacroRecord),
        ("Record Session (cast)", Action::ToggleRecording),
        ("Settings", Action::ToggleSettings),
        ("Documentation", Action::ShowHelp),
        ("Relay: Launch Agent", Action::RelayLaunch),
        ("Relay: Open Feed", Action::RelayFeed),
        ("Relay: View Logs", Action::RelayLog),
        ("Relay: Start Server", Action::RelayStart),
        ("Relay: Stop Server", Action::RelayStop),
        ("Relay: Restart Server", Action::RelayRestart),
        ("Quit", Action::Quit),
    ]
}

/// A floating pill shown while a cast recording is capturing, stacked below
/// the macro/broadcast pills so the three never collide.
fn recording_pill(palette: &Colors) -> AnyElement {
    let accent = theme::Rgb::new(255, 69, 58);
    let mut bg = colors::hsla(palette.bg);
    bg.a = 0.9;
    let mut border = colors::hsla(accent);
    border.a = 0.5;
    div()
        .absolute()
        .top(px(56.0))
        .right(px(8.0))
        .flex()
        .items_center()
        .gap_1()
        .px_2()
        .py(px(2.0))
        .rounded(px(6.0))
        .bg(bg)
        .border_1()
        .border_color(border)
        .text_size(px(11.0))
        .text_color(colors::hsla(accent))
        .child(SharedString::from("\u{25cf}"))
        .child(SharedString::from("REC"))
        .into_any_element()
}

/// A floating pill warning that broadcast input is active, placed beside the
/// macro pill (one notch lower so they never collide).
fn broadcast_pill(palette: &Colors) -> AnyElement {
    let accent = theme::Rgb::new(255, 196, 0);
    let mut bg = colors::hsla(palette.bg);
    bg.a = 0.9;
    let mut border = colors::hsla(accent);
    border.a = 0.5;
    div()
        .absolute()
        .top(px(32.0))
        .right(px(8.0))
        .flex()
        .items_center()
        .gap_1()
        .px_2()
        .py(px(2.0))
        .rounded(px(6.0))
        .bg(bg)
        .border_1()
        .border_color(border)
        .text_size(px(11.0))
        .text_color(colors::hsla(accent))
        .child(SharedString::from("\u{1f4e1}"))
        .child(SharedString::from("BROADCAST"))
        .into_any_element()
}

fn macro_pill(recording: bool, replaying: bool, palette: &Colors) -> Option<AnyElement> {
    if !recording && !replaying {
        return None;
    }
    let (glyph, label, accent) = if recording {
        ("\u{25cf}", "REC", theme::Rgb::new(230, 80, 80))
    } else {
        ("\u{25b6}", "REPLAY", theme::Rgb::new(120, 190, 250))
    };
    let mut bg = colors::hsla(palette.bg);
    bg.a = 0.9;
    let mut border = colors::hsla(palette.fg);
    border.a = 0.18;
    Some(
        div()
            .absolute()
            .top(px(8.0))
            .right(px(8.0))
            .flex()
            .items_center()
            .gap_1()
            .px_2()
            .py(px(2.0))
            .rounded(px(6.0))
            .bg(bg)
            .border_1()
            .border_color(border)
            .text_size(px(11.0))
            .text_color(colors::hsla(accent))
            .child(SharedString::from(glyph))
            .child(SharedString::from(label))
            .into_any_element(),
    )
}

/// Load saved macros from the default macro directory (empty if unconfigured
/// or absent).
fn loadmacros() -> Vec<macros::Macro> {
    macros::defaultdir()
        .map(|dir| macros::load(&dir))
        .unwrap_or_default()
}

fn resolvekeys(
    opts: &config::Options,
    plugins: &[plugin::Plugin],
) -> (Vec<Keybind>, Vec<config::Diagnostic>) {
    let mut raw = plugin::keybinds(plugins);
    raw.extend(opts.keybind.iter().cloned());
    let (mut binds, diags) = config::resolve(&raw);
    // Menu-only actions that ship without a default shortcut: give each a
    // binding-less entry so it gets a dispatch index for the menu. The empty
    // key is unspellable, so `applykeybinds` never binds it to a keystroke.
    // Skip any the user has already bound, so their shortcut shows instead.
    for action in [Action::NewSplit(SplitDirection::Left)] {
        if !binds.iter().any(|b| b.action == action) {
            binds.push(Keybind {
                mods: config::Mods::default(),
                key: String::new(),
                action,
            });
        }
    }
    (binds, diags)
}

fn commandspawn(opts: &config::Options, command: &str) -> pty::SpawnOptions {
    let shell = opts
        .shell
        .as_ref()
        .and_then(|shell| shell.split_whitespace().next())
        .filter(|shell| !shell.is_empty())
        .map(str::to_string)
        .unwrap_or_else(pty::default_shell);
    // Run the command through an interactive login shell so it inherits the exact
    // same environment as a normal pane — we make no assumptions about where the
    // user's startup files put things. GUI launches inherit only macOS's minimal
    // PATH; `-l`+`-i` makes the shell source its login *and* interactive rc (e.g.
    // ~/.zshrc), which is where PATH additions like `claude`'s dir usually live.
    // Without it, `relay launch` can't exec the agent binary. Flags are passed
    // separately rather than fused (`-ilc`) for portability across shells.
    pty::SpawnOptions::command(vec![
        shell,
        "-i".to_string(),
        "-l".to_string(),
        "-c".to_string(),
        command.to_string(),
    ])
}
