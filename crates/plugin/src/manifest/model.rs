//! The parsed `plugin.toml` data model: the public types the host consumes and
//! the fixed vocabularies (`CAPABILITIES`, `TRIGGER_EVENTS`). No parsing logic
//! lives here — see `parse.rs`.

use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub path: PathBuf,
    pub line: usize,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plugin {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub path: PathBuf,
    pub commands: Vec<Command>,
    /// `[runtime]`: an executable the app invokes (over JSON on stdin/stdout)
    /// to render panels and handle actions. Present makes this an IPC plugin.
    pub runtime: Option<Runtime>,
    /// `[panel]`: a side-drawer panel this plugin contributes.
    pub panel: Option<Panel>,
    /// `[webview]`: a native web-view surface this plugin contributes.
    pub webview: Option<Webview>,
    /// `[[trigger]]`: event hooks that run an action when something happens.
    pub triggers: Vec<Trigger>,
    /// `[[tool]]`: tools this plugin exposes to MCP clients (AI agents). Each is
    /// handled by the `[runtime]` via a `tool` request. This is what makes a
    /// plugin agent-callable: the tools appear in `sinclair mcp`'s tool list.
    pub tools: Vec<Tool>,
    /// `capability = "…"`: what the plugin declares it accesses (from
    /// [`CAPABILITIES`]). Advisory today — surfaced at install so users see a
    /// plugin's reach — and the vocabulary the sandboxed runtime will enforce.
    pub capabilities: Vec<String>,
}

/// The capabilities a plugin may declare (`capability = "…"`). A process
/// `[runtime]` runs with full user privileges regardless; these describe intent
/// for the user and are the gate list the WASM runtime enforces.
pub const CAPABILITIES: &[&str] = &[
    "commands",   // run shell commands / terminal directives
    "screen",     // read terminal output (read_screen)
    "network",    // make network requests
    "filesystem", // read or write files
    "clipboard",  // read or write the clipboard
    "notify",     // post desktop notifications
];

/// `[[tool]]` — a tool a plugin exposes to AI agents over MCP. When an agent
/// calls it, the app invokes the plugin's `[runtime]` with a `tool` request
/// (`method` = the tool id, `params` = the arguments) and returns its `result`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tool {
    /// Stable id; the MCP tool name is `<plugin-id>_<id>`.
    pub id: String,
    /// Description shown to the agent (the MCP `description`).
    pub description: String,
    /// Declared parameters, assembled into the MCP `inputSchema` by the host.
    pub params: Vec<ToolParam>,
}

/// One argument of a `[[tool]]`, declared as
/// `param = "name | type | description | required"` (type/description/required
/// optional; type defaults to `string`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolParam {
    pub name: String,
    /// JSON Schema type: `string` | `number` | `integer` | `boolean`.
    pub kind: String,
    pub description: String,
    pub required: bool,
}

/// `[runtime]` — how to launch the plugin's function host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Runtime {
    /// Which host runs the plugin.
    pub kind: RuntimeKind,
    /// For a `process` runtime: the command to spawn (split on whitespace).
    pub command: String,
    /// For a `wasm` runtime: the `.wasm` module path, relative to the plugin.
    pub wasm: Option<String>,
    /// A `process` runtime that is a long-lived stdio server (reads newline JSON
    /// requests, writes newline JSON responses in a loop) rather than one-shot.
    /// The host keeps it warm instead of spawning per event. Ignored for `wasm`
    /// (always resident).
    pub persistent: bool,
}

/// The kind of `[runtime]` host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RuntimeKind {
    /// A subprocess spoken to over JSON on stdin/stdout. Full user privileges;
    /// needs whatever interpreter its `command` names (bun, node, …).
    #[default]
    Process,
    /// A WebAssembly module run in-process. No runtime dependency; sandboxed to
    /// its declared [`capabilities`](Plugin::capabilities). The execution engine
    /// is in progress — see `docs/plugins-wasm.md`; declaring it is supported so
    /// plugins and the host can adopt the surface incrementally.
    Wasm,
}

impl RuntimeKind {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "process" | "" => Some(Self::Process),
            "wasm" | "wasm32" | "webassembly" => Some(Self::Wasm),
            _ => None,
        }
    }
}

/// `[panel]` — a contributed side-drawer panel rendered from the plugin's
/// block-tree responses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Panel {
    /// Stable id used in render/action requests and the activity-bar payload.
    pub id: String,
    /// Header/title shown for the panel.
    pub title: String,
    /// Single-glyph activity-bar icon.
    pub icon: String,
}

/// `[webview]` — a native web-view surface a plugin contributes. It hosts an
/// arbitrary HTML/JS UI that talks to the app over the webview message bridge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Webview {
    /// Stable id used in open requests and the activity-bar/tab token.
    pub id: String,
    /// Header/tab/title shown for the surface.
    pub title: String,
    /// Single-glyph activity-bar / tab icon.
    pub icon: String,
    /// Where the surface is shown.
    pub placement: Placement,
    /// Where the page content comes from.
    pub source: WebviewSource,
    /// When true (and the plugin has a `[runtime]`), the app invokes the
    /// runtime's `boot` method before loading, and navigates to the URL it
    /// returns (`{ port }` or `{ url }`). Lets a plugin start a local server and
    /// hand back its address — the page then loads from a real `http` origin.
    pub boot: bool,
}

/// Where a `[webview]` surface is shown. `tab` is parsed now but hosted in a
/// later phase; until then the host falls back to a window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Placement {
    /// A side-drawer panel (default).
    #[default]
    Panel,
    /// A tab / split pane.
    Tab,
    /// A standalone window.
    Window,
}

impl Placement {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "panel" => Some(Self::Panel),
            "tab" => Some(Self::Tab),
            "window" => Some(Self::Window),
            _ => None,
        }
    }

    /// The config token, round-tripping through [`Placement::parse`].
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Panel => "panel",
            Self::Tab => "tab",
            Self::Window => "window",
        }
    }
}

/// Where a `[webview]` loads its content from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WebviewSource {
    /// A remote or absolute URL, loaded as-is.
    Url(String),
    /// A path relative to the plugin directory, served over the internal origin.
    Entry(String),
    /// A host-managed sidecar: the host runs this command as a local server
    /// (allocating a port + token, health-checking, reaping on close) and loads
    /// the page from its `http` origin. Generalizes the built-in Notes server so
    /// any plugin — including Notes itself, now a plugin — can ship a backend.
    Service(String),
}

/// The event names a `[[trigger]]` may hook. Kept in one place so the manifest
/// parser and the host agree.
pub const TRIGGER_EVENTS: &[&str] = &[
    "bell",
    "title_changed",
    "notify",
    "exit",
    "command_finished",
    "dir_changed",
    "worktree_created",
    "worktree_removed",
];

/// `[[trigger]]` — run an action when a terminal event fires. The plugin
/// reacts to things happening rather than being opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trigger {
    /// The event to hook; one of [`TRIGGER_EVENTS`].
    pub on: String,
    /// Optional event-specific filter (e.g. `nonzero` for exits, or a substring
    /// for title/notify). Interpreted by the host.
    pub when: Option<String>,
    /// What to do when the trigger fires.
    pub action: TriggerAction,
}

/// What a [`Trigger`] does when it fires.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TriggerAction {
    /// Run a shell command at `target`.
    Run { text: String, target: TriggerTarget },
    /// Post a desktop notification with this body.
    Notify { text: String },
    /// Call the plugin's `[runtime]` with the event payload (method name).
    Invoke { method: String },
}

/// Where a trigger's `run` command executes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TriggerTarget {
    /// A detached background process (default) — no terminal UI.
    #[default]
    Background,
    /// Typed into the focused pane.
    Pane,
    /// A new tab.
    Tab,
    /// A right split.
    SplitRight,
    /// A down split.
    SplitDown,
}

impl TriggerTarget {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "background" | "bg" => Some(Self::Background),
            "pane" => Some(Self::Pane),
            "tab" => Some(Self::Tab),
            "splitright" | "split-right" | "split_right" => Some(Self::SplitRight),
            "splitdown" | "split-down" | "split_down" => Some(Self::SplitDown),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    pub id: String,
    pub title: String,
    pub run: String,
    pub mode: CommandMode,
    pub keybind: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CommandMode {
    /// Write the command into the focused shell and press enter.
    #[default]
    Pane,
    /// Run the command in a new tab.
    Tab,
    /// Run the command in a right split.
    SplitRight,
    /// Run the command in a down split.
    SplitDown,
}

impl CommandMode {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "pane" => Some(Self::Pane),
            "tab" => Some(Self::Tab),
            "splitright" | "split-right" => Some(Self::SplitRight),
            "splitdown" | "split-down" => Some(Self::SplitDown),
            _ => None,
        }
    }
}
