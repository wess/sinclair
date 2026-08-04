//! Workspace root: a `guise::PaneGroup` (splits contain tabs,
//! each pane has its own tab bar) plus a map from the group's `ItemId`s to the
//! terminal/webview entities that back them.
//!
//! The group owns the tree, per-pane tab bars, dividers, and drag/drop; this
//! module owns the items (creating/destroying their real content), reacts to
//! the group's events, and drives layout mutations through the group's methods.

mod agents;
mod boot;
mod containers;
mod dialogs;
mod dispatch;
pub(crate) mod dock;
mod layout;
mod mcp;
mod menus;
mod panes;
mod persist;
mod pluginpanel;
mod quickopen;
mod render;
pub(crate) mod sandbox;
mod savebuffer;
mod sidebar;
mod tabmenu;
mod tabs;
mod triggers;
mod worktrees;

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

use config::{Action, Keybind, ResizeDir, SplitDirection, SplitFocus};
use futures::StreamExt;
use gpui::prelude::*;
use gpui::{
    div, px, size, AnyElement, App, Context, Entity, Focusable, KeyBinding, Menu, MenuItem,
    SharedString, Subscription, Window,
};
use guise::panegroup::{Direction, ItemId, ItemIds, PaneId, TearDrop};
use guise::SplitDirection as SplitAxis;
use guise::{PaneGroup, PaneGroupEvent};
use serde_json::{json, Value};
use terminal::Session;

use crate::colors::{self, Colors};
use crate::keys;
use crate::session;
use crate::view::{TerminalView, ViewEvent};
use libsinclair::bridge;
use libsinclair::metrics::{CellSize, Padding};

pub(crate) use boot::{
    commandspawn, loadmacros, loadplugins, palette_catalog, resolvekeys, write_config,
};
// The settings designer edits the same composition the workspace renders, so
// it builds docks through the same code rather than a parallel reading of the
// config keys.
pub(crate) use pluginpanel::{compose_docks, label_of, token_of};

/// One keybind dispatch: the index into the workspace's resolved keybind
/// table. A single action type keeps every binding flowing through one
/// handler regardless of which config action it carries.
#[derive(Clone, PartialEq, Default, Debug, gpui::Action)]
#[action(namespace = sinclair, no_json)]
pub struct RunBind(pub usize);

/// Open the documentation window. Its own action (rather than a `RunBind`) so
/// the Help menu item works without depending on a configured keybind.
#[derive(Clone, PartialEq, Default, Debug, gpui::Action)]
#[action(namespace = sinclair, no_json)]
pub struct ShowDocs;

/// Open the About panel. Its own action so the application-menu item works
/// without depending on a configured keybind.
#[derive(Clone, PartialEq, Default, Debug, gpui::Action)]
#[action(namespace = sinclair, no_json)]
pub struct ShowAbout;

/// Dispatch a menu item that has no keybind: the index into the workspace's
/// `menu_actions` table built alongside the native menu.
#[derive(Clone, PartialEq, Default, Debug, gpui::Action)]
#[action(namespace = sinclair, no_json)]
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
    /// Truthful liveness: parked on `wait` or active within the last minute.
    /// Computed by the server from a heartbeat + parked set, not a sticky flag.
    pub online: bool,
    /// Whether the agent has ever registered (false = a pending direct-message
    /// recipient the server pre-created; see relay issue #5).
    pub registered: bool,
    pub channels: i64,
    /// Epoch seconds of the agent's last activity (0 if never seen).
    pub last_seen: i64,
    /// The agent's last self-reported semantic state (`working`/`blocked`/
    /// `done`/`idle`, or a custom label); empty when none reported.
    pub status: String,
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

/// Map a config split direction to a `guise` split axis plus which side the new
/// pane takes (`first` = left/top).
pub(crate) fn split_dir(dir: SplitDirection) -> (SplitAxis, bool) {
    match dir {
        SplitDirection::Right => (SplitAxis::Horizontal, false),
        SplitDirection::Left => (SplitAxis::Horizontal, true),
        SplitDirection::Down => (SplitAxis::Vertical, false),
        SplitDirection::Up => (SplitAxis::Vertical, true),
    }
}

/// What a pane hosts: a terminal, or a plugin web view (in a tab). The shared
/// read surface below lets the workspace treat panes uniformly; terminal-only
/// operations match on the variant (or go through `WorkspaceView::onfocused`,
/// which only acts on `Terminal`).
pub(crate) enum PaneContent {
    Terminal(Entity<TerminalView>),
    Webview(Entity<crate::webview::SurfaceView>),
}

impl PaneContent {
    fn focus_handle(&self, cx: &App) -> gpui::FocusHandle {
        match self {
            PaneContent::Terminal(v) => v.focus_handle(cx),
            PaneContent::Webview(v) => v.focus_handle(cx),
        }
    }

    fn title(&self, cx: &App) -> String {
        match self {
            PaneContent::Terminal(v) => v.read(cx).title().to_string(),
            PaneContent::Webview(v) => v.read(cx).title(),
        }
    }

    /// True when the title was typed by the user rather than set by the
    /// program in the pane. An explicit name is never rewritten.
    fn has_title_override(&self, cx: &App) -> bool {
        match self {
            PaneContent::Terminal(v) => v.read(cx).has_title_override(),
            // A webview's title is its own; nothing shell-shaped to strip.
            PaneContent::Webview(_) => true,
        }
    }

    fn needs_attention(&self, cx: &App) -> bool {
        match self {
            PaneContent::Terminal(v) => v.read(cx).needs_attention(),
            PaneContent::Webview(_) => false,
        }
    }

    fn cwd(&self, cx: &App) -> Option<String> {
        match self {
            PaneContent::Terminal(v) => v.read(cx).cwd(),
            PaneContent::Webview(_) => None,
        }
    }

    fn cwd_path(&self, cx: &App) -> Option<std::path::PathBuf> {
        match self {
            PaneContent::Terminal(v) => v.read(cx).cwd_path(),
            PaneContent::Webview(_) => None,
        }
    }

    fn is_recording(&self, cx: &App) -> bool {
        match self {
            PaneContent::Terminal(v) => v.read(cx).is_recording(),
            PaneContent::Webview(_) => false,
        }
    }

    fn has_running_process(&self, cx: &App) -> bool {
        match self {
            PaneContent::Terminal(v) => v.read(cx).has_running_process(),
            PaneContent::Webview(_) => false,
        }
    }

    fn element(&self) -> AnyElement {
        match self {
            PaneContent::Terminal(v) => gpui::AnyView::from(v.clone())
                .cached(
                    gpui::StyleRefinement::default()
                        .flex()
                        .flex_col()
                        .w_full()
                        .h_full(),
                )
                .into_any_element(),
            PaneContent::Webview(v) => v.clone().into_any_element(),
        }
    }

    /// The terminal view, when this pane is a terminal (for terminal-only work).
    fn as_terminal(&self) -> Option<&Entity<TerminalView>> {
        match self {
            PaneContent::Terminal(v) => Some(v),
            PaneContent::Webview(_) => None,
        }
    }
}

/// One item held by the pane group: a terminal or webview surface, plus its
/// event bridge. Items are the unit that lives inside a `guise` pane's tab bar.
struct Item {
    content: PaneContent,
    /// The terminal event bridge; `None` for webview items (they emit none).
    _subscription: Option<Subscription>,
    /// A process-globally-unique token injected into the session's environment
    /// as `SINCLAIR_PANE`, so an agent's hooks can report state for exactly this
    /// pane (across all windows). `0` for items with no backing session.
    pane_token: u64,
    /// The agent's last self-reported semantic state, driving the tab/sidebar
    /// status dot. `None` when this pane isn't running a reporting agent.
    agent: Option<crate::agentstate::AgentState>,
    /// The agent's last-reported native session id, persisted for resume.
    agent_session: Option<String>,
    /// The command this pane was launched with (for agent panes), persisted so a
    /// restored session can relaunch — and resume — the agent.
    command: Option<String>,
}

/// The host-owned item map the `PaneGroup` reads through its render/title
/// callbacks. Shared as an `Rc<RefCell<..>>` so those callbacks (which run
/// during the group's render) can read it without borrowing `WorkspaceView`.
type Items = HashMap<ItemId, Item>;

// The dock composition model — which sections live on which side, in what
// order, and what a reveal does. Kept gpui-free in `dock.rs` so it can be
// tested; `sidebar.rs` renders whatever it decides.
pub(crate) use dock::{DockSection, Docks, SidebarPanel, SidebarSide};

pub struct WorkspaceView {
    opts: config::Options,
    colors: Rc<Colors>,
    font: gpui::Font,
    font_size: gpui::Pixels,
    cell: CellSize,
    pad: Padding,
    /// The reusable `guise` pane group: the whole window's tree of tabbed
    /// splits. Owns layout, per-pane tab bars, dividers, and drag/drop.
    group: Entity<PaneGroup>,
    /// Host-owned items keyed by the group's `ItemId`s. Shared with the group's
    /// render/title callbacks (see [`Items`]).
    items: Rc<RefCell<Items>>,
    /// Monotonic allocator for [`ItemId`]s.
    item_ids: ItemIds,
    /// Keeps the group's event subscription alive.
    _group_sub: Subscription,
    /// Resolved keybindings (defaults + user config); `RunBind` indexes here.
    keybinds: Vec<Keybind>,
    /// Loaded manifest plugins.
    plugins: Vec<plugin::Plugin>,
    /// Saved command macros, loaded from the macro directory.
    macros: Vec<macros::Macro>,
    /// Config actions for keybind-less menu items, indexed by [`MenuPick`].
    menu_actions: Vec<Action>,
    /// The two side docks, indexed by [`SidebarSide::index`]. Each is an
    /// ordered stack of collapsible sections composed from `sidebar-left` /
    /// `sidebar-right`, so the sides are genuinely different columns rather
    /// than the same panel list twice.
    docks: Docks,
    /// The window's last windowed bounds, refreshed on every frame. gpui has no
    /// bounds observer, but a move or resize calls `bounds_changed`, which
    /// refreshes — so render is where this can be seen. Saved into the session
    /// so the window reopens where you left it.
    last_bounds: Option<gpui::Bounds<gpui::Pixels>>,
    /// Worktrees of the current repo, cached when that section expands. `None`
    /// until first resolved; the outer/inner options separate "not looked yet"
    /// from "looked and this isn't a repo".
    worktrees: Option<Result<Vec<crate::worktree::Worktree>, String>>,
    /// Notes vaults, cached when the Notes section expands.
    notes_recent: Vec<crate::notes::Vault>,
    /// Last block-tree response per plugin panel id, refreshed on open/action.
    plugin_panels: HashMap<String, crate::panelui::Response>,

    /// GUI-side WASM runtime for `wasm` plugin panels, created on first use.
    gui_wasm: Option<crate::guiwasm::GuiWasm>,
    /// Installable catalog plugin names, fetched lazily when the Plugins panel
    /// opens; `None` until the first fetch.
    catalog: Option<Vec<String>>,
    /// Status line for the Plugins panel (last fetch/install result).
    catalog_status: Option<String>,
    /// True while a catalog fetch is in flight (off-thread).
    catalog_loading: bool,
    /// The guise Spotlight quick-open overlay (cmd+P), rebuilt each open.
    spotlight: Option<Entity<guise::Spotlight>>,
    /// Live `tab-title-show-host`, shared with the pane group's tab-label
    /// closure so a config reload reaches it without rebuilding the group.
    show_host: Rc<std::cell::Cell<bool>>,
    /// The open tab context menu (right-click on a tab). Rebuilt per open, so
    /// its entries can close over the tab that was clicked.
    tab_menu: Option<Entity<guise::ContextMenu>>,
    /// The active in-window dialog (rename), as a guise Modal
    /// overlay. `None` when no dialog is open.
    modal: Option<gpui::AnyView>,
    /// Cached running containers for the Containers panel, refreshed when the
    /// panel opens or on its refresh action (running `docker ps` is I/O, so it
    /// is never done during render).
    containers: Vec<container::Running>,
    /// The container engine resolved alongside [`Self::refresh_containers`]
    /// (resolving stats `$PATH`, so never during render). `None` until the
    /// Containers panel first opens.
    engine: Option<Option<container::Engine>>,
    /// Cached menu/panel data that needs blocking reads (the relay `team list`
    /// subprocess, agents.json, the layouts dir). Refreshed off-thread by
    /// [`Self::refresh_menu_data`]; `setmenus` and the sidebar render from here.
    menu_teams: Vec<String>,
    menu_agent_defs: Vec<crate::relay::AgentDef>,
    menu_custom_tiles: Vec<String>,
    /// Why the most recent shell spawn failed. Shown as an in-window error when
    /// the window has no live items (e.g. a bad `shell =` at startup), instead
    /// of exiting the app.
    spawn_error: Option<String>,
    /// Map of container id → the item attached to it, so re-selecting a
    /// running container focuses its existing tab instead of opening a second.
    container_tabs: HashMap<String, ItemId>,
    /// Items whose on-the-fly (run-fresh) container should be force-removed when
    /// the item closes (value is the container name). Only ephemeral containers
    /// — `container-persist = false` — are tracked here; persistent ones are
    /// left running.
    kill_on_close: HashMap<ItemId, String>,
    /// This project's shared sandbox, once it has been brought up. One
    /// container serves every pane and every agent, so it is resolved once per
    /// window and cached here.
    sandbox: Option<crate::sandbox::Ready>,
    /// Panes currently attached to the sandbox. The container outlives any one
    /// of them; the count is what decides when it may be retired.
    sandbox_panes: HashSet<ItemId>,
    /// Live progress or the last failure, shown in the AI menu and the
    /// Containers panel. `None` when the sandbox is simply idle.
    sandbox_status: Option<String>,
    /// Advisories from the last resolve (a devcontainer that stops on close, a
    /// mount that did not parse), surfaced in the Containers panel.
    sandbox_notes: Vec<String>,
    /// True while a resolve is in flight, so a second click does not start a
    /// second build.
    sandbox_busy: bool,
    /// Configured font size, restored by `reset_font_size`.
    base_font_size: gpui::Pixels,
    /// Enabled agent providers that passed their reachability probe, gating the
    /// AI menu's quick-launch items. `None` until the first off-thread probe
    /// finishes — treated as "unknown", so every enabled provider shows meanwhile.
    verified_agents: Option<HashSet<String>>,
    /// Whether the OS appearance is currently dark (drives `theme-light`/`-dark`).
    dark: bool,
    /// Keeps the OS-appearance observer alive.
    _appearance: Option<gpui::Subscription>,
    /// The transparency currently applied to the platform window, synced in
    /// `render` so a live `background-opacity` change flips the window between
    /// opaque and transparent.
    bg_transparent: bool,
    /// Set when this window was opened to hold a Relay team, naming it. Titles
    /// the window and keeps the window out of session save/restore — the panes
    /// are agents, reproducible from the team spec, and replaying their
    /// `relay launch` on the next start would re-register a roster the daemon
    /// may still be running.
    team: Option<String>,
}

/// A team to realize in a freshly opened window. The roster becomes the
/// window's whole pane group — one member per pane, nothing else in it — so a
/// team reads as its own divided workspace rather than a tab folded into
/// whatever you were already doing.
pub(crate) struct TeamOpen {
    /// Team name, shown in the window title.
    pub name: String,
    /// The roster resolved into panes. `panes.commands[0]` runs in the window's
    /// first pane instead of the default shell; the rest split off it.
    pub panes: crate::relay::TeamPanes,
}

/// Whether a gpui window appearance is one of the dark variants.
pub(crate) fn is_dark(a: gpui::WindowAppearance) -> bool {
    matches!(
        a,
        gpui::WindowAppearance::Dark | gpui::WindowAppearance::VibrantDark
    )
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
        // A torn-off tab re-homed here as the first item (else spawn a shell).
        adopt: Option<PaneContent>,
        // A Relay team to fill this window with, instead of a single shell.
        team: Option<TeamOpen>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let plugins = loadplugins(&opts);
        // Composed before `self` exists, from settings plus the loaded plugin
        // set — both docks start closed, exactly as the old drawers did.
        let docks = pluginpanel::compose_docks(&opts, &plugins);
        let (keybinds, diags) = resolvekeys(&opts, &plugins);
        for d in &diags {
            eprintln!("sinclair: {}: {}", d.key, d.message);
        }
        if cx.try_global::<MacroRecorder>().is_none() {
            cx.set_global(MacroRecorder(macros::Recorder::new()));
        }
        let items: Rc<RefCell<Items>> = Rc::new(RefCell::new(HashMap::new()));
        let mut item_ids = ItemIds::new();
        // The group needs a first item, but spawning one needs `self`. Build a
        // throwaway group over a placeholder id, assemble `this`, spawn the real
        // first terminal, then rebuild the group around it below.
        let placeholder = item_ids.next();
        let show_host = Rc::new(std::cell::Cell::new(opts.tab_title_show_host));
        let group = Self::build_group(placeholder, items.clone(), show_host.clone(), cx);
        let group_sub = cx.subscribe_in(
            &group,
            window,
            |this, _g, ev: &PaneGroupEvent, window, cx| {
                this.on_group_event(ev.clone(), window, cx);
            },
        );
        let mut this = Self {
            base_font_size: font_size,
            opts,
            colors,
            font,
            font_size,
            cell,
            pad,
            group,
            items,
            item_ids,
            _group_sub: group_sub,
            keybinds,
            plugins,
            macros: loadmacros(),
            menu_actions: Vec::new(),
            docks,
            last_bounds: None,
            worktrees: None,
            notes_recent: Vec::new(),
            plugin_panels: HashMap::new(),
            gui_wasm: None,
            catalog: None,
            catalog_status: None,
            catalog_loading: false,
            spotlight: None,
            show_host,
            tab_menu: None,
            modal: None,
            containers: Vec::new(),
            engine: None,
            menu_teams: Vec::new(),
            menu_agent_defs: Vec::new(),
            menu_custom_tiles: Vec::new(),
            spawn_error: None,
            container_tabs: HashMap::new(),
            kill_on_close: HashMap::new(),
            sandbox: None,
            sandbox_panes: HashSet::new(),
            sandbox_status: None,
            sandbox_notes: Vec::new(),
            sandbox_busy: false,
            verified_agents: None,
            dark: is_dark(window.appearance()),
            _appearance: None,
            // main.rs always opens the window transparent-capable.
            bg_transparent: true,
            team: team.as_ref().map(|t| t.name.clone()),
        };
        // Follow the OS light/dark appearance when `theme-light`/`theme-dark` are set.
        this._appearance = Some(cx.observe_window_appearance(window, |view, window, cx| {
            let dark = is_dark(window.appearance());
            if dark != view.dark {
                view.dark = dark;
                if view.opts.auto_theme() {
                    view.apply_theme(cx);
                }
            }
        }));
        this.applykeybinds(cx);
        this.setmenus(cx);
        let first = match adopt {
            // A torn-off tab: re-home its live terminal/webview as the first item.
            Some(content) => this.register_item(content, window, cx),
            // A team window opens *on* its first member rather than spawning a
            // shell and adding the team beside it, so no pane in the window
            // holds anything but an agent. The lead takes the cwd explicitly:
            // there is no focused pane to inherit one from yet, and every other
            // member then inherits it from the lead. It is the directory each
            // role resolves its project `.relay/roles` against.
            None if team.is_some() => {
                let lead = team
                    .as_ref()
                    .and_then(|t| t.panes.commands.first().cloned())
                    .flatten();
                match lead.and_then(|c| this.spawn_command_cwd(&c, cwd, window, cx)) {
                    Some(id) => id,
                    None => placeholder,
                }
            }
            None => {
                let options = session::options(&this.opts, cols, rows, cwd);
                match this.spawn(options, window, cx) {
                    Some(id) => id,
                    // A bad `shell =` must not kill the app (every window shares
                    // this path via NewWindow). Keep the placeholder id — it has
                    // no backing item, so the window renders the spawn-error view
                    // (see `render`) until a spawn succeeds again.
                    None => placeholder,
                }
            }
        };
        // Rebuild the group around the real first item (the placeholder held none).
        this.group = Self::build_group(first, this.items.clone(), this.show_host.clone(), cx);
        this._group_sub = cx.subscribe_in(
            &this.group,
            window,
            |this, _g, ev: &PaneGroupEvent, window, cx| {
                this.on_group_event(ev.clone(), window, cx);
            },
        );
        this.focusactive(window, cx);
        crate::relay::on_launch(&this.opts);
        crate::relaywatch::start(&this.opts, cx);
        this.refresh_agent_verification(cx);
        this.refresh_menu_data(cx);
        // Split the rest of the roster off the lead's pane. Done after the group
        // is rebuilt so the splits land in the real tree, not the placeholder's.
        // Every pane is titled after its member: the tab would otherwise read
        // `zsh`, since the launch command runs under a shell.
        if let Some(t) = team.filter(|_| this.spawn_error.is_none()) {
            let host = this.group.read(cx).focused_pane();
            if let Some(lead) = t.panes.names.first() {
                this.rename_item(first, lead, cx);
            }
            this.realize_into(
                &t.panes.layout,
                host,
                0,
                &t.panes.commands,
                &t.panes.names,
                window,
                cx,
            );
            // Land on the lead, not on whichever pane was split last: the lead
            // is the member that stays interactive for the human, so it is the
            // one you start typing to.
            this.activate_item(first, window, cx);
        }
        // Restoring re-spawns shells; with the first spawn already failed there
        // is nothing to restore into (and closing the placeholder would close
        // the window before the error is ever seen). A team window is already
        // full of the roster it was opened for, and never restores.
        if this.opts.session_restore && this.spawn_error.is_none() && this.team.is_none() {
            this.try_restore(window, cx);
        }
        this
    }

    /// Build a `PaneGroup` over `first`, wiring its per-item content and title
    /// callbacks to read the shared `items` map. The callbacks run during the
    /// group's render (nested inside the workspace's), so they read `items`
    /// directly rather than the `WorkspaceView` entity (which would double-borrow).
    fn build_group(
        first: ItemId,
        items: Rc<RefCell<Items>>,
        show_host: Rc<std::cell::Cell<bool>>,
        cx: &mut Context<Self>,
    ) -> Entity<PaneGroup> {
        // The group doubles as the window titlebar: reserve the top-left inset
        // for the macOS traffic lights (Linux draws its own controls on the
        // right, so reserve there instead).
        let (leading, trailing) = if cfg!(target_os = "macos") {
            (crate::titlebar::TRAFFIC_LIGHT_INSET, 0.0)
        } else {
            (8.0, 120.0)
        };
        cx.new(|cx| {
            PaneGroup::new(first, cx)
                .titlebar(leading, trailing)
                .tab_height(34.0)
                .on_render_item({
                    let items = items.clone();
                    move |id, _w, _cx| {
                        items
                            .borrow()
                            .get(&id)
                            .map(|it| it.content.element())
                            .unwrap_or_else(|| div().into_any_element())
                    }
                })
                .on_item_title({
                    let items = items.clone();
                    let show_host = show_host.clone();
                    move |id, cx| {
                        let items = items.borrow();
                        let Some(item) = items.get(&id) else {
                            return SharedString::default();
                        };
                        let title = item.content.title(cx);
                        // Shells write `user@host:path`; most people want the
                        // path. A title the user typed is left exactly as typed.
                        if show_host.get() || item.content.has_title_override(cx) {
                            SharedString::from(title)
                        } else {
                            SharedString::from(crate::tabbar::strip_host(&title).to_string())
                        }
                    }
                })
                .on_item_dot({
                    let items = items.clone();
                    move |id, _cx| {
                        items
                            .borrow()
                            .get(&id)
                            .and_then(|it| it.agent)
                            .map(|st| colors::hsla(st.color()))
                    }
                })
        })
    }

    /// Handle an event emitted by the pane group.
    fn on_group_event(&mut self, ev: PaneGroupEvent, window: &mut Window, cx: &mut Context<Self>) {
        match ev {
            PaneGroupEvent::NewRequested(pane) => {
                if let Some(item) = self.spawn_default(window, cx) {
                    self.group.update(cx, |g, cx| g.add_item(pane, item, cx));
                    self.focusactive(window, cx);
                    cx.notify();
                }
            }
            PaneGroupEvent::CloseRequested(item) => self.close_item(item, window, cx),
            PaneGroupEvent::Activated(_) | PaneGroupEvent::FocusChanged(_) => {
                self.focusactive(window, cx);
                cx.notify();
            }
            PaneGroupEvent::TearOff { item, drop } => {
                self.tear_off_to_window(item, drop, window, cx)
            }
            PaneGroupEvent::ContextMenu { item, position } => {
                self.open_tab_menu(item, position, window, cx)
            }
        }
    }

    /// The focused pane's active item id.
    pub(crate) fn active_item(&self, cx: &App) -> ItemId {
        self.group.read(cx).active_item()
    }

    /// The focused item's terminal view, if it is a terminal (not a webview).
    pub(crate) fn focused_terminal(&self, cx: &App) -> Option<Entity<TerminalView>> {
        let item = self.group.read(cx).active_item();
        self.items
            .borrow()
            .get(&item)
            .and_then(|it| it.content.as_terminal().cloned())
    }

    /// The focused item's working directory (resolved from its OSC-reported cwd).
    pub(crate) fn focused_cwd_path(&self, cx: &App) -> Option<std::path::PathBuf> {
        let item = self.group.read(cx).active_item();
        self.items
            .borrow()
            .get(&item)
            .and_then(|it| it.content.cwd(cx))
            .and_then(|osc| session::cwdpath(&osc))
    }

    /// Focus the focused pane's active item and retitle the window.
    pub(crate) fn focusactive(&self, window: &mut Window, cx: &mut Context<Self>) {
        let item = self.group.read(cx).active_item();
        let handle = self
            .items
            .borrow()
            .get(&item)
            .map(|it| it.content.focus_handle(cx));
        if let Some(handle) = handle {
            window.focus(&handle, cx);
        }
        // A switched-away webview item must hide its native surface.
        self.reconcile_webview_visibility(cx);
        self.settitle(window, cx);
    }

    /// Set the window title from the focused item. A team window leads with the
    /// team name, so its window is identifiable no matter which member has
    /// focus.
    pub(crate) fn settitle(&self, window: &mut Window, cx: &App) {
        let item = self.group.read(cx).active_item();
        let title = self
            .items
            .borrow()
            .get(&item)
            .map(|it| it.content.title(cx))
            .unwrap_or_else(|| "sinclair".to_string());
        match &self.team {
            Some(team) => window.set_window_title(&format!("{team} \u{2014} {title}")),
            None => window.set_window_title(&title),
        }
    }

    /// Activate `item` in its pane and focus it.
    pub(crate) fn activate_item(
        &mut self,
        item: ItemId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let pane = self.group.read(cx).pane_of(item);
        if let Some(pane) = pane {
            self.group.update(cx, |g, cx| g.activate(pane, item, cx));
            self.focusactive(window, cx);
            cx.notify();
        }
    }

    /// Remove an item's content and drop it from the group. Closes the window
    /// when it was the last item.
    pub(crate) fn close_item(&mut self, item: ItemId, window: &mut Window, cx: &mut Context<Self>) {
        if self.group.read(cx).items().len() <= 1 {
            self.close_window(window, cx);
            return;
        }
        self.on_item_closed(item, cx);
        // Dropping the Item drops the TerminalView (and its pty/subscription).
        self.items.borrow_mut().remove(&item);
        self.group.update(cx, |g, cx| g.close_item(item, cx));
        self.focusactive(window, cx);
        cx.notify();
    }

    /// Re-home a torn-off item into a new window. The group already detached it
    /// (guise `tear_off`); take its live content out of `items` — dropping the
    /// old event subscription, but keeping the `TerminalView` entity and its pty
    /// alive (the event pump is app-scoped) — and open a new window adopting it.
    fn tear_off_to_window(
        &mut self,
        item: ItemId,
        drop: Option<TearDrop>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(Item { content, .. }) = self.items.borrow_mut().remove(&item) else {
            return;
        };
        let source = window.bounds();
        // The tab's release is reported in window coordinates, and a window's
        // bounds are read against its own display, so both stay in that
        // display's frame.
        let display = window.display(cx).map(|d| d.bounds());
        let place = display.map(|display| match drop {
            // A drag: open where the pointer let go, so the window arrives under
            // the ghost the user was already carrying.
            Some(drop) => {
                crate::dropped(source.origin + drop.position, drop.grab, source.size, display)
            }
            // No gesture to honor (a menu tear): match the window the tab came
            // from and step off it, so the new window keeps its size and doesn't
            // land squarely behind its source.
            None => crate::cascade(source, display),
        });
        crate::open_window(
            self.opts.clone(),
            self.colors.clone(),
            self.font.clone(),
            self.font_size,
            self.cell,
            self.pad,
            None,
            Some(content),
            place,
            None,
            cx,
        );
        cx.notify();
    }

    /// Re-read the config file and apply everything that can change at
    /// runtime: theme/colors, font family/size, padding, cursor style,
    /// copy-on-select. Shell, scrollback and window size only affect new
    /// sessions or need a restart.
    ///
    /// This is the single reload path (the settings window only writes the
    /// file), so it fires on every settings toggle and slider step. Each
    /// heavier subsystem is diffed against the current options and skipped
    /// when its keys did not change: plugin rescans, font re-measures, and
    /// agent re-verification never run for an unrelated key.
    pub(crate) fn reload(&mut self, cx: &mut Context<Self>) {
        let (opts, diagnostics) = config::load();
        for d in &diagnostics {
            eprintln!(
                "sinclair: settings line {}: {} ({})",
                d.line, d.message, d.key
            );
        }
        let font_changed = opts.font_family != self.opts.font_family
            || opts.font_feature != self.opts.font_feature
            || opts.font_style != self.opts.font_style
            || (opts.font_size - self.opts.font_size).abs() > f32::EPSILON
            || opts.adjust_cell_width != self.opts.adjust_cell_width
            || opts.adjust_cell_height != self.opts.adjust_cell_height;
        let plugins_changed = opts.plugin != self.opts.plugin;
        let agents_changed = opts.ai_enabled != self.opts.ai_enabled
            || opts.agent_claude != self.opts.agent_claude
            || opts.agent_codex != self.opts.agent_codex
            || opts.agent_ollama != self.opts.agent_ollama
            || opts.agent_gemini != self.opts.agent_gemini
            || opts.agent_claude_path != self.opts.agent_claude_path
            || opts.agent_codex_path != self.opts.agent_codex_path
            || opts.agent_gemini_path != self.opts.agent_gemini_path
            || opts.agent_custom != self.opts.agent_custom;
        crate::redact::install(&opts.redact, cx);
        crate::badge::install(&opts.badge, cx);
        crate::view::install_timestamps(opts.timestamps, cx);
        crate::trigger::install(&opts.trigger, cx);
        self.colors = Rc::new(colors::from_config(&opts, self.dark));
        crate::guisetheme::install(&self.colors, cx);
        if font_changed {
            self.font = crate::font::build(&opts);
            self.font_size = px(opts.font_size.max(1.0));
            self.cell = libsinclair::metrics::measure(cx.text_system(), &self.font, self.font_size);
        }
        self.pad = Padding {
            x: opts.window_padding_x as f32,
            y: opts.window_padding_y as f32,
        };
        self.base_font_size = px(opts.font_size.max(1.0));
        self.opts = opts;
        self.show_host.set(self.opts.tab_title_show_host);
        if plugins_changed {
            self.plugins = loadplugins(&self.opts);
            // Drop the resident instances with them. A plugin that was disabled
            // or uninstalled otherwise keeps its instance — and its guest
            // memory — alive for as long as the window, and one whose module
            // changed on disk would keep serving the old code. They are rebuilt
            // lazily on the next panel render.
            self.gui_wasm = None;
            self.plugin_panels.clear();
        }
        self.macros = loadmacros();
        let (keybinds, diags) = resolvekeys(&self.opts, &self.plugins);
        for d in &diags {
            eprintln!("sinclair: {}: {}", d.key, d.message);
        }
        self.keybinds = keybinds;
        self.applykeybinds(cx);
        if agents_changed {
            // Re-probe providers: paths or toggles changed. Clear the cache
            // first so a newly enabled tool shows immediately, then prune.
            self.verified_agents = None;
            self.refresh_agent_verification(cx);
        }
        self.setmenus(cx);
        self.pushappearance(cx);
        // Reconciling the relay daemon probes it (file read + TCP connect);
        // keep that off the UI thread.
        let relay_opts = self.opts.clone();
        std::thread::spawn(move || crate::relay::on_reload(&relay_opts));
        self.refresh_menu_data(cx);
        cx.notify();
    }

    /// Rebuild colors for the current OS appearance and push them everywhere.
    /// Used by the appearance observer for live light/dark switching.
    fn apply_theme(&mut self, cx: &mut Context<Self>) {
        self.colors = Rc::new(colors::from_config(&self.opts, self.dark));
        crate::guisetheme::install(&self.colors, cx);
        self.pushappearance(cx);
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
            smart_select: self.opts.smart_select,
            middle_click_paste: self.opts.middle_click_paste,
            option_as_alt: self.opts.macos_option_as_alt,
            paste_protection: self.opts.clipboard_paste_protection,
            clipboard_write: self.opts.clipboard_write,
            suggest: crate::suggest::SuggestConfig::from_opts(&self.opts),
            unfocused_split_opacity: self.opts.unfocused_split_opacity,
        };
        let terminals: Vec<_> = self
            .items
            .borrow()
            .values()
            .filter_map(|it| it.content.as_terminal().cloned())
            .collect();
        for v in terminals {
            v.update(cx, |view, cx| view.set_appearance(&appearance, cx));
        }
    }

    /// Whether the live config exposes this terminal's MCP tool surface.
    /// Read by the IPC bridge on every dispatched op.
    pub fn mcp_enabled(&self) -> bool {
        self.opts.mcp_server_enabled
    }

    /// Re-measure the cell box for the current font size and republish.
    pub(crate) fn setfontsize(&mut self, size: gpui::Pixels, cx: &mut Context<Self>) {
        let size = px(f32::from(size).max(1.0));
        if size == self.font_size {
            return;
        }
        self.font_size = size;
        self.cell = libsinclair::metrics::measure(cx.text_system(), &self.font, self.font_size);
        self.pushappearance(cx);
        cx.notify();
    }
}
