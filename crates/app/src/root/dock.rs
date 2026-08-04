//! Side dock composition: which sections live on which side, in what order,
//! and which are expanded.
//!
//! Deliberately free of gpui — this is the part of the sidebar that has real
//! logic in it (resolving config tokens, revealing a section wherever the user
//! put it, moving one dock to the other), and keeping it as plain data over
//! plain functions is what makes it testable. `sidebar.rs` renders what this
//! module decides.
//!
//! Sections are addressed by **token**, never by index. `SidebarPanel::Plugin`
//! and `Webview` carry an index into a list that shifts whenever plugins are
//! loaded or removed, so persisting one would quietly rebind a saved slot to a
//! different plugin the next time the set changed.

/// Which side a dock lives on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SidebarSide {
    Left,
    Right,
}

impl SidebarSide {
    /// Index into [`Docks`].
    pub fn index(self) -> usize {
        match self {
            SidebarSide::Left => 0,
            SidebarSide::Right => 1,
        }
    }

    /// The prefix this side takes in an `Action::Sidebar` payload.
    pub fn token(self) -> &'static str {
        match self {
            SidebarSide::Left => "left",
            SidebarSide::Right => "right",
        }
    }

    pub fn parse(s: &str) -> Option<SidebarSide> {
        match s {
            "left" => Some(SidebarSide::Left),
            "right" => Some(SidebarSide::Right),
            _ => None,
        }
    }

    pub fn other(self) -> SidebarSide {
        match self {
            SidebarSide::Left => SidebarSide::Right,
            SidebarSide::Right => SidebarSide::Left,
        }
    }
}

/// The content of one dock section. Each maps to one rail icon and one
/// collapsible block in the stack.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SidebarPanel {
    /// Tree of tabs and their terminal panes.
    Terminals,
    /// At-a-glance activity: each tab's working / attention / idle state.
    Activity,
    /// Tile presets and saved custom layouts.
    Layouts,
    /// Live Relay server status and connections.
    Relay,
    /// Saved agent definitions you can launch.
    Agents,
    /// Installed plugins + the installable catalog.
    Plugins,
    /// Running containers (Docker/Podman): attach or spin up new OS tabs.
    Containers,
    /// This repository's git worktrees.
    Worktrees,
    /// The notes vault: recent notes and the tree.
    Notes,
    /// A plugin-contributed panel, by index into `plugin_panel_defs`.
    Plugin(usize),
    /// A plugin-contributed webview panel, by index into
    /// `plugin_webview_panel_defs`.
    Webview(usize),
}

impl SidebarPanel {
    /// Every built-in section, in catalog order. This is what the settings
    /// designer offers; it is *not* what any one dock contains.
    pub const ALL: [SidebarPanel; 9] = [
        SidebarPanel::Terminals,
        SidebarPanel::Activity,
        SidebarPanel::Layouts,
        SidebarPanel::Worktrees,
        SidebarPanel::Containers,
        SidebarPanel::Relay,
        SidebarPanel::Agents,
        SidebarPanel::Notes,
        SidebarPanel::Plugins,
    ];

    /// Config/id token, round-tripping through [`SidebarPanel::from_id`]. The
    /// plugin variants have no stable id here — they carry an index, so their
    /// token is built by `panel_token_of`, which can reach the live plugin list.
    pub fn id(self) -> &'static str {
        match self {
            SidebarPanel::Terminals => "terminals",
            SidebarPanel::Activity => "activity",
            SidebarPanel::Layouts => "layouts",
            SidebarPanel::Containers => "containers",
            SidebarPanel::Relay => "relay",
            SidebarPanel::Agents => "agents",
            SidebarPanel::Plugins => "plugins",
            SidebarPanel::Worktrees => "worktrees",
            SidebarPanel::Notes => "notes",
            SidebarPanel::Plugin(_) => "plugin",
            SidebarPanel::Webview(_) => "webview",
        }
    }

    pub fn from_id(s: &str) -> Option<SidebarPanel> {
        SidebarPanel::ALL.into_iter().find(|p| p.id() == s)
    }

    /// Title shown in the section header.
    pub fn label(self) -> &'static str {
        match self {
            SidebarPanel::Terminals => "Terminals",
            SidebarPanel::Activity => "Activity",
            SidebarPanel::Layouts => "Layouts",
            SidebarPanel::Containers => "Containers",
            SidebarPanel::Relay => "Relay",
            SidebarPanel::Agents => "Agents",
            SidebarPanel::Plugins => "Plugins",
            SidebarPanel::Worktrees => "Worktrees",
            SidebarPanel::Notes => "Notes",
            SidebarPanel::Plugin(_) => "Plugin",
            SidebarPanel::Webview(_) => "Webview",
        }
    }

    /// The section header's label, pre-uppercased.
    ///
    /// A constant rather than `label().to_uppercase()` because the header is
    /// rebuilt for every visible section on every repaint, and the workspace
    /// repaints whenever a pane produces output — an allocation per section per
    /// line of build log is not a good trade for a string that never changes.
    pub fn label_upper(self) -> &'static str {
        match self {
            SidebarPanel::Terminals => "TERMINALS",
            SidebarPanel::Activity => "ACTIVITY",
            SidebarPanel::Layouts => "LAYOUTS",
            SidebarPanel::Containers => "CONTAINERS",
            SidebarPanel::Relay => "RELAY",
            SidebarPanel::Agents => "AGENTS",
            SidebarPanel::Plugins => "PLUGINS",
            SidebarPanel::Worktrees => "WORKTREES",
            SidebarPanel::Notes => "NOTES",
            // Plugin titles come from a manifest (or a live response), so they
            // are the one case that has to be uppercased at render time.
            SidebarPanel::Plugin(_) | SidebarPanel::Webview(_) => "",
        }
    }

    /// Rail glyph.
    pub fn icon(self) -> &'static str {
        match self {
            SidebarPanel::Terminals => "\u{25a3}",  // ▣ panes
            SidebarPanel::Activity => "\u{25c9}",   // ◉ activity
            SidebarPanel::Layouts => "\u{25f0}",    // ◰ tiles
            SidebarPanel::Containers => "\u{2756}", // ❖ containers
            SidebarPanel::Relay => "\u{21c4}",      // ⇄ connections
            SidebarPanel::Agents => "\u{25c8}",     // ◈ agents
            SidebarPanel::Plugins => "\u{29c9}",    // ⧉ plugins
            SidebarPanel::Worktrees => "\u{2442}",  // ⑂ branches
            SidebarPanel::Notes => "\u{25a5}",      // ▥ pages
            SidebarPanel::Plugin(_) => "\u{25c9}",  // ◉ plugin
            SidebarPanel::Webview(_) => "\u{25f1}", // ◱ webview
        }
    }
}

/// One section in a dock's stack.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct DockSection {
    pub panel: SidebarPanel,
    pub expanded: bool,
}

impl DockSection {
    pub fn new(panel: SidebarPanel, expanded: bool) -> Self {
        DockSection { panel, expanded }
    }
}

/// One side's dock: an ordered stack of sections, its width, and whether it is
/// showing at all.
#[derive(Clone, PartialEq, Debug)]
pub struct Dock {
    pub sections: Vec<DockSection>,
    pub width: f32,
    pub open: bool,
}

impl Dock {
    /// Position of `panel` in this dock, if it holds one.
    pub fn find(&self, panel: SidebarPanel) -> Option<usize> {
        self.sections.iter().position(|s| s.panel == panel)
    }

    pub fn holds(&self, panel: SidebarPanel) -> bool {
        self.find(panel).is_some()
    }

    /// Expanded sections share the dock's leftover height, so a dock where
    /// everything is collapsed renders as headers alone.
    pub fn any_expanded(&self) -> bool {
        self.sections.iter().any(|s| s.expanded)
    }
}

/// Both docks, indexed by [`SidebarSide::index`].
pub type Docks = [Dock; 2];

/// The built-in composition for a side, used when the user has never set one.
/// The whole point of the overhaul is that these two differ: file-ish and
/// structural things on the left, live agent state on the right.
pub fn defaults(side: SidebarSide) -> Vec<SidebarPanel> {
    match side {
        SidebarSide::Left => vec![
            SidebarPanel::Terminals,
            SidebarPanel::Worktrees,
            SidebarPanel::Layouts,
        ],
        SidebarSide::Right => vec![
            SidebarPanel::Activity,
            SidebarPanel::Agents,
            SidebarPanel::Relay,
        ],
    }
}

/// Build one dock from its configured tokens.
///
/// `resolve` turns a token into a panel; it returns `None` for a token that no
/// longer names anything (an uninstalled plugin). Those are skipped here and
/// **left alone in the config file**, so reinstalling the plugin puts it back in
/// the same slot rather than appending it to the end.
///
/// An empty `tokens` means "unset", which is what gets the built-in
/// composition — distinct from a dock the user has deliberately emptied, which
/// is why the settings page writes a sentinel rather than an empty list.
pub fn compose(
    side: SidebarSide,
    tokens: &[String],
    collapsed: &[String],
    width: f32,
    resolve: impl Fn(&str) -> Option<SidebarPanel>,
    token_of: impl Fn(SidebarPanel) -> String,
) -> Dock {
    let panels: Vec<SidebarPanel> = if tokens.is_empty() {
        defaults(side)
    } else {
        tokens
            .iter()
            .filter(|t| t.as_str() != EMPTY)
            .filter_map(|t| resolve(t))
            .collect()
    };
    let sections = panels
        .into_iter()
        .map(|panel| DockSection::new(panel, !collapsed.iter().any(|c| *c == token_of(panel))))
        .collect();
    Dock {
        sections,
        width,
        // Both docks start hidden, as they always have; composition only
        // decides what is in one once it is opened.
        open: false,
    }
}

/// Written by the designer for a dock the user emptied on purpose, so that an
/// empty list can keep meaning "never configured, use the defaults".
pub const EMPTY: &str = "none";

/// What [`reveal`] decided to do, so the caller can run the matching side
/// effects (fetch a catalog, refresh containers, hide a webview…).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Revealed {
    /// The side the section ended up on.
    pub side: SidebarSide,
    /// Whether it is expanded now.
    pub expanded: bool,
    /// True when the section was not in either dock and got added to `side`
    /// for this session only.
    pub added: bool,
}

/// Reveal `panel`: expand it wherever it already lives, opening that dock.
/// Re-revealing an already-expanded section collapses it, which is what makes a
/// keybinding a toggle.
///
/// `prefer` is the side named in the action payload. It only decides where an
/// *unplaced* section goes — a section the user has moved to the other dock is
/// revealed there, so `sidebar:left:containers` keeps working after Containers
/// has been moved right.
pub fn reveal(docks: &mut Docks, prefer: SidebarSide, panel: SidebarPanel) -> Revealed {
    for side in [prefer, prefer.other()] {
        let dock = &mut docks[side.index()];
        if let Some(i) = dock.find(panel) {
            // A closed dock means "show me this", not "toggle it off" — opening
            // and expanding is the only useful reading of a keypress there.
            let expanded = if dock.open {
                !dock.sections[i].expanded
            } else {
                true
            };
            dock.sections[i].expanded = expanded;
            dock.open = true;
            return Revealed {
                side,
                expanded,
                added: false,
            };
        }
    }
    // Unplaced: add it to the preferred side for this session. Not persisted —
    // the settings page owns composition, and a keybinding should not silently
    // rewrite it.
    let dock = &mut docks[prefer.index()];
    dock.sections.push(DockSection::new(panel, true));
    dock.open = true;
    Revealed {
        side: prefer,
        expanded: true,
        added: true,
    }
}

/// Toggle the section at a known position, returning what it now holds and
/// whether it is expanded.
///
/// The click path for the rail and the section headers. They drew a specific
/// section and already know its index, so going through [`reveal`] would mean
/// building a token string, parsing it back into a side and a token, and
/// re-resolving that token against the plugin list — three allocations and a
/// linear scan to rediscover something the caller never stopped knowing.
/// [`reveal`] stays for config actions and keybinds, where a string genuinely
/// is the input.
pub fn toggle_at(
    docks: &mut Docks,
    side: SidebarSide,
    index: usize,
) -> Option<(SidebarPanel, bool)> {
    let dock = &mut docks[side.index()];
    let section = dock.sections.get_mut(index)?;
    // A closed dock means "show me this", same as `reveal`.
    let expanded = if dock.open { !section.expanded } else { true };
    section.expanded = expanded;
    let panel = section.panel;
    dock.open = true;
    Some((panel, expanded))
}

/// Toggle a whole dock open/closed. Opening one that has no expanded section
/// expands the first, so a dock never opens showing nothing but headers.
pub fn toggle_side(docks: &mut Docks, side: SidebarSide) {
    let dock = &mut docks[side.index()];
    dock.open = !dock.open;
    if dock.open && !dock.any_expanded() {
        if let Some(first) = dock.sections.first_mut() {
            first.expanded = true;
        }
    }
}

/// Move the section at `from_index` on `from` into `to` at `to_index`
/// (clamped). A no-op if the index is out of range. Used by the designer.
pub fn move_to(
    docks: &mut Docks,
    from: SidebarSide,
    from_index: usize,
    to: SidebarSide,
    to_index: usize,
) {
    if from_index >= docks[from.index()].sections.len() {
        return;
    }
    let section = docks[from.index()].sections.remove(from_index);
    let dest = &mut docks[to.index()];
    let at = to_index.min(dest.sections.len());
    dest.sections.insert(at, section);
}

/// Move a section within its own dock by `delta` (-1 up, +1 down), clamped.
pub fn reorder(docks: &mut Docks, side: SidebarSide, index: usize, delta: isize) {
    let sections = &mut docks[side.index()].sections;
    if index >= sections.len() {
        return;
    }
    let target = index as isize + delta;
    if target < 0 || target as usize >= sections.len() {
        return;
    }
    sections.swap(index, target as usize);
}

/// Drop a section from whichever dock holds it.
pub fn remove(docks: &mut Docks, panel: SidebarPanel) {
    for dock in docks.iter_mut() {
        if let Some(i) = dock.find(panel) {
            dock.sections.remove(i);
            return;
        }
    }
}

/// Add `panel` to the end of `side`, if it is not already placed somewhere.
pub fn add(docks: &mut Docks, side: SidebarSide, panel: SidebarPanel) {
    if docks.iter().any(|d| d.holds(panel)) {
        return;
    }
    docks[side.index()]
        .sections
        .push(DockSection::new(panel, true));
}

/// Built-in sections not currently placed in either dock — the designer's
/// "available" pool. Plugin-contributed sections are not offered here; they
/// appear automatically (see `plugin_defaults`).
pub fn available(docks: &Docks) -> Vec<SidebarPanel> {
    SidebarPanel::ALL
        .into_iter()
        .filter(|p| !docks.iter().any(|d| d.holds(*p)))
        .collect()
}

/// Append any plugin-contributed section that the config has not placed. A
/// freshly installed plugin should show up without the user having to go and
/// configure it; the right dock is where `panel_pluginlist`'s "Open panel" row
/// has always sent them.
pub fn place_unplaced(docks: &mut Docks, contributed: &[SidebarPanel]) {
    for panel in contributed {
        if !docks.iter().any(|d| d.holds(*panel)) {
            docks[SidebarSide::Right.index()]
                .sections
                .push(DockSection::new(*panel, false));
        }
    }
}

/// The tokens to persist for one dock. An emptied dock writes [`EMPTY`] so it
/// stays empty across a reload instead of falling back to the defaults.
pub fn tokens_of(dock: &Dock, token_of: impl Fn(SidebarPanel) -> String) -> Vec<String> {
    if dock.sections.is_empty() {
        return vec![EMPTY.to_string()];
    }
    dock.sections
        .iter()
        .map(|s| token_of(s.panel))
        .collect()
}

/// The tokens of every collapsed section across both docks, for
/// `sidebar-collapsed`.
pub fn collapsed_tokens(docks: &Docks, token_of: impl Fn(SidebarPanel) -> String) -> Vec<String> {
    docks
        .iter()
        .flat_map(|d| d.sections.iter())
        .filter(|s| !s.expanded)
        .map(|s| token_of(s.panel))
        .collect()
}

#[cfg(test)]
#[path = "../../tests/dock.rs"]
mod tests;
