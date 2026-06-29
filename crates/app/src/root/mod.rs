//! Workspace root: ordered tabs of split panes, one shell per pane.
//!
//! Owns the `workspace::Tabs` model and a map from pane id to terminal
//! view entity. All tab/split mutations funnel through here; the panes
//! themselves only know their own session.

mod boot;
mod dispatch;
mod layout;
mod mcp;
mod menus;
mod panes;
mod persist;
mod pluginpanel;
mod render;
mod sidebar;
mod tabs;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use config::{Action, Keybind, ResizeDir, SplitDirection, SplitFocus};
use futures::StreamExt;
use gpui::prelude::*;
use gpui::{
    anchored, deferred, div, point, px, size, AnyElement, App, Context, Entity,
    KeyBinding, Menu, MenuItem, MouseButton, MouseDownEvent, SharedString, Subscription,
    WeakEntity, Window,
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

pub(crate) use boot::{
    commandspawn, loadmacros, loadplugins, palette_catalog, resolvekeys, strip_user_host,
    write_config,
};

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

/// A live Relay connection (agent) from the status stream.
#[derive(Clone, Default)]
pub struct AgentConn {
    pub name: String,
    pub role: String,
    pub online: bool,
    pub channels: i64,
}

/// A live Relay worker (spawned agent process) from the status stream.
#[derive(Clone, Default)]
pub struct WorkerConn {
    pub name: String,
    pub role: String,
    pub status: String,
    pub restarts: i64,
}

/// App-global snapshot of live Relay status, fed by the `relay watch`
/// subprocess (see `relaywatch.rs`) and read by the Relay sidebar panel.
/// Process-wide (one relay daemon) so it lives in a global like the others.
#[derive(Clone, Default)]
pub struct RelayStatus {
    pub connected: bool,
    pub agents: Vec<AgentConn>,
    pub workers: Vec<WorkerConn>,
}
impl gpui::Global for RelayStatus {}

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

/// Which trailing tab-bar button dropdown is open, if any.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TabBarMenu {
    /// The `+` button's "New…" menu.
    New,
    /// The column button's "Split…" menu.
    Split,
}

/// Which side a drawer lives on.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SidebarSide {
    Left,
    Right,
}

/// The content shown in a side drawer. Each maps to one activity-bar icon.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SidebarPanel {
    /// Tree of tabs and their terminal panes.
    Terminals,
    /// Tile presets and saved custom layouts.
    Layouts,
    /// Live Relay server status and connections.
    Relay,
    /// Saved agent definitions you can launch.
    Agents,
    /// A plugin-contributed panel, by index into [`WorkspaceView::plugin_panel_defs`].
    Plugin(usize),
}

impl SidebarPanel {
    /// Panels in activity-bar order.
    pub const ALL: [SidebarPanel; 4] = [
        SidebarPanel::Terminals,
        SidebarPanel::Layouts,
        SidebarPanel::Relay,
        SidebarPanel::Agents,
    ];

    /// Config/id token, round-tripping through [`SidebarPanel::from_id`].
    pub fn id(self) -> &'static str {
        match self {
            SidebarPanel::Terminals => "terminals",
            SidebarPanel::Layouts => "layouts",
            SidebarPanel::Relay => "relay",
            SidebarPanel::Agents => "agents",
            SidebarPanel::Plugin(_) => "plugin",
        }
    }

    pub fn from_id(s: &str) -> Option<SidebarPanel> {
        SidebarPanel::ALL.into_iter().find(|p| p.id() == s)
    }

    /// Title shown in the panel header.
    pub fn label(self) -> &'static str {
        match self {
            SidebarPanel::Terminals => "Terminals",
            SidebarPanel::Layouts => "Layouts",
            SidebarPanel::Relay => "Relay",
            SidebarPanel::Agents => "Agents",
            SidebarPanel::Plugin(_) => "Plugin",
        }
    }

    /// Activity-bar glyph.
    pub fn icon(self) -> &'static str {
        match self {
            SidebarPanel::Terminals => "\u{25a3}", // ▣ panes
            SidebarPanel::Layouts => "\u{25f0}",   // ◰ tiles
            SidebarPanel::Relay => "\u{21c4}",     // ⇄ connections
            SidebarPanel::Agents => "\u{25c8}",    // ◈ agents
            SidebarPanel::Plugin(_) => "\u{25c9}", // ◉ plugin
        }
    }
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
    /// When true, the tab-overflow `…` dropdown is open.
    tab_overflow: bool,
    /// Which trailing tab-bar button dropdown is open (`+` / split), if any.
    trailing_menu: Option<TabBarMenu>,
    /// Active panel in the left drawer; `None` = left drawer hidden.
    left_panel: Option<SidebarPanel>,
    /// Active panel in the right drawer; `None` = right drawer hidden.
    right_panel: Option<SidebarPanel>,
    /// Last block-tree response per plugin panel id, refreshed on open/action.
    plugin_panels: HashMap<String, crate::pluginhost::Response>,
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
            tabs: Tabs::new(PaneIds::new().next()),
            ids: PaneIds::new(),
            panes: HashMap::new(),
            drag: Rc::new(RefCell::new(None)),
            keybinds,
            plugins,
            macros: loadmacros(),
            menu_actions: Vec::new(),
            zoomed: false,
            tab_overflow: false,
            trailing_menu: None,
            left_panel: None,
            right_panel: None,
            plugin_panels: HashMap::new(),
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
        crate::relaywatch::start(&this.opts, cx);
        if this.opts.session_restore {
            this.try_restore(window, cx);
        }
        this
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
        crate::guisetheme::install(&self.colors, cx);
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
            clipboard_write: self.opts.clipboard_write,
        };
        for pane in self.panes.values() {
            pane.view
                .update(cx, |view, cx| view.set_appearance(&appearance, cx));
        }
    }

    /// Re-measure the cell box for the current font size and republish.
    pub(crate) fn setfontsize(&mut self, size: gpui::Pixels, cx: &mut Context<Self>) {
        let size = px(f32::from(size).max(1.0));
        if size == self.font_size {
            return;
        }
        self.font_size = size;
        self.cell = crate::metrics::measure(cx.text_system(), &self.font, self.font_size);
        self.pushappearance(cx);
        cx.notify();
    }
}
