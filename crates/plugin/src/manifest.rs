//! `plugin.toml` parsing.
//!
//! The parser intentionally supports a small TOML subset: root key/value
//! pairs and repeated `[[command]]` tables. That keeps plugin manifests easy
//! to hand-write without adding a broad dependency for this first API.

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
    /// plugin agent-callable: the tools appear in `prompt mcp`'s tool list.
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
    fn parse(value: &str) -> Option<Self> {
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
    fn parse(value: &str) -> Option<Self> {
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
    /// A path relative to the plugin directory, loaded via `file://`.
    Entry(String),
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
    fn parse(value: &str) -> Option<Self> {
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
    fn parse(value: &str) -> Option<Self> {
        match value {
            "pane" => Some(Self::Pane),
            "tab" => Some(Self::Tab),
            "splitright" | "split-right" => Some(Self::SplitRight),
            "splitdown" | "split-down" => Some(Self::SplitDown),
            _ => None,
        }
    }
}

#[derive(Default)]
struct RawPlugin {
    id: Option<String>,
    name: Option<String>,
    version: Option<String>,
    description: Option<String>,
    commands: Vec<RawCommand>,
    has_runtime: bool,
    runtime_command: Option<String>,
    runtime_type: Option<RuntimeKind>,
    runtime_wasm: Option<String>,
    has_panel: bool,
    panel_id: Option<String>,
    panel_title: Option<String>,
    panel_icon: Option<String>,
    has_webview: bool,
    webview_id: Option<String>,
    webview_title: Option<String>,
    webview_icon: Option<String>,
    webview_placement: Option<Placement>,
    webview_url: Option<String>,
    webview_entry: Option<String>,
    webview_boot: bool,
    triggers: Vec<RawTrigger>,
    tools: Vec<RawTool>,
    capabilities: Vec<String>,
}

#[derive(Default)]
struct RawTool {
    id: Option<String>,
    description: Option<String>,
    params: Vec<String>,
    line: usize,
}

#[derive(Default)]
struct RawCommand {
    id: Option<String>,
    title: Option<String>,
    run: Option<String>,
    mode: Option<CommandMode>,
    keybind: Option<String>,
    line: usize,
}

#[derive(Default)]
struct RawTrigger {
    on: Option<String>,
    when: Option<String>,
    run: Option<String>,
    target: Option<TriggerTarget>,
    notify: Option<String>,
    invoke: Option<String>,
    line: usize,
}

enum Section {
    Plugin,
    Command(usize),
    Runtime,
    Panel,
    Webview,
    Trigger(usize),
    Tool(usize),
}

pub fn parse(path: PathBuf, text: &str) -> (Option<Plugin>, Vec<Diagnostic>) {
    let mut raw = RawPlugin::default();
    let mut diags = Vec::new();
    let mut section = Section::Plugin;

    for (i, src) in text.lines().enumerate() {
        let line = i + 1;
        let trimmed = src.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed == "[[command]]" {
            raw.commands.push(RawCommand {
                line,
                ..RawCommand::default()
            });
            section = Section::Command(raw.commands.len() - 1);
            continue;
        }
        if trimmed == "[[trigger]]" {
            raw.triggers.push(RawTrigger {
                line,
                ..RawTrigger::default()
            });
            section = Section::Trigger(raw.triggers.len() - 1);
            continue;
        }
        if trimmed == "[[tool]]" {
            raw.tools.push(RawTool {
                line,
                ..RawTool::default()
            });
            section = Section::Tool(raw.tools.len() - 1);
            continue;
        }
        if trimmed == "[runtime]" {
            raw.has_runtime = true;
            section = Section::Runtime;
            continue;
        }
        if trimmed == "[panel]" {
            raw.has_panel = true;
            section = Section::Panel;
            continue;
        }
        if trimmed == "[webview]" {
            raw.has_webview = true;
            section = Section::Webview;
            continue;
        }
        if trimmed.starts_with('[') {
            diags.push(diag(&path, line, "unknown section"));
            continue;
        }
        let Some((key, val)) = trimmed.split_once('=') else {
            diags.push(diag(&path, line, "expected `key = value`"));
            continue;
        };
        let key = key.trim();
        let val = value(val.trim());
        if key.is_empty() {
            diags.push(diag(&path, line, "missing key before `=`"));
            continue;
        }
        match section {
            Section::Plugin => rootkey(&mut raw, key, &val, &path, line, &mut diags),
            Section::Command(index) => {
                commandkey(&mut raw.commands[index], key, &val, &path, line, &mut diags)
            }
            Section::Runtime => runtimekey(&mut raw, key, &val, &path, line, &mut diags),
            Section::Panel => panelkey(&mut raw, key, &val, &path, line, &mut diags),
            Section::Webview => webviewkey(&mut raw, key, &val, &path, line, &mut diags),
            Section::Trigger(index) => {
                triggerkey(&mut raw.triggers[index], key, &val, &path, line, &mut diags)
            }
            Section::Tool(index) => {
                toolkey(&mut raw.tools[index], key, &val, &path, line, &mut diags)
            }
        }
    }

    let plugin = build(raw, &path, &mut diags);
    (plugin, diags)
}

fn rootkey(
    raw: &mut RawPlugin,
    key: &str,
    val: &str,
    path: &std::path::Path,
    line: usize,
    diags: &mut Vec<Diagnostic>,
) {
    match key {
        "id" => raw.id = Some(val.to_string()),
        "name" => raw.name = Some(val.to_string()),
        "version" => raw.version = Some(val.to_string()),
        "description" => raw.description = Some(val.to_string()),
        "capability" => {
            if CAPABILITIES.contains(&val) {
                if !raw.capabilities.iter().any(|c| c == val) {
                    raw.capabilities.push(val.to_string());
                }
            } else {
                diags.push(diag(
                    path,
                    line,
                    &format!("unknown capability `{val}` (one of {})", CAPABILITIES.join(", ")),
                ));
            }
        }
        _ => diags.push(diag(path, line, &format!("unknown plugin key `{key}`"))),
    }
}

fn triggerkey(
    raw: &mut RawTrigger,
    key: &str,
    val: &str,
    path: &std::path::Path,
    line: usize,
    diags: &mut Vec<Diagnostic>,
) {
    match key {
        "on" => raw.on = Some(val.to_string()),
        "when" => raw.when = Some(val.to_string()),
        "run" => raw.run = Some(val.to_string()),
        "target" => match TriggerTarget::parse(val) {
            Some(t) => raw.target = Some(t),
            None => diags.push(diag(
                path,
                line,
                "trigger target must be `background`, `pane`, `tab`, `split_right`, or `split_down`",
            )),
        },
        "notify" => raw.notify = Some(val.to_string()),
        "invoke" => raw.invoke = Some(val.to_string()),
        _ => diags.push(diag(path, line, &format!("unknown trigger key `{key}`"))),
    }
}

fn commandkey(
    raw: &mut RawCommand,
    key: &str,
    val: &str,
    path: &std::path::Path,
    line: usize,
    diags: &mut Vec<Diagnostic>,
) {
    match key {
        "id" => raw.id = Some(val.to_string()),
        "title" => raw.title = Some(val.to_string()),
        "run" => raw.run = Some(val.to_string()),
        "mode" => match CommandMode::parse(val) {
            Some(mode) => raw.mode = Some(mode),
            None => diags.push(diag(path, line, "invalid command mode")),
        },
        "keybind" => raw.keybind = Some(val.to_string()),
        _ => diags.push(diag(path, line, &format!("unknown command key `{key}`"))),
    }
}

fn toolkey(
    raw: &mut RawTool,
    key: &str,
    val: &str,
    path: &std::path::Path,
    line: usize,
    diags: &mut Vec<Diagnostic>,
) {
    match key {
        "id" => raw.id = Some(val.to_string()),
        "description" => raw.description = Some(val.to_string()),
        "param" => raw.params.push(val.to_string()),
        _ => diags.push(diag(path, line, &format!("unknown tool key `{key}`"))),
    }
}

fn build(raw: RawPlugin, path: &std::path::Path, diags: &mut Vec<Diagnostic>) -> Option<Plugin> {
    let id = required(raw.id, "id", path, 0, diags)?;
    if !validid(&id) {
        diags.push(diag(
            path,
            0,
            "plugin id must use lowercase letters, numbers, `.` or `-`",
        ));
        return None;
    }
    let mut commands = Vec::new();
    for command in raw.commands {
        let Some(command) = buildcommand(command, path, diags) else {
            continue;
        };
        commands.push(command);
    }
    let name = raw.name.unwrap_or_else(|| id.clone());
    let runtime = if raw.has_runtime {
        let kind = raw.runtime_type.unwrap_or_default();
        let command = raw.runtime_command.filter(|s| !s.trim().is_empty());
        let wasm = raw.runtime_wasm.filter(|s| !s.trim().is_empty());
        match kind {
            RuntimeKind::Process => match command {
                Some(command) => Some(Runtime { kind, command, wasm: None }),
                None => {
                    diags.push(diag(path, 0, "[runtime] requires a `command`"));
                    None
                }
            },
            RuntimeKind::Wasm => match wasm {
                Some(wasm) => Some(Runtime {
                    kind,
                    command: command.unwrap_or_default(),
                    wasm: Some(wasm),
                }),
                None => {
                    diags.push(diag(path, 0, "a `wasm` runtime requires a `wasm` module path"));
                    None
                }
            },
        }
    } else {
        None
    };
    let panel = if raw.has_panel {
        let panel_id = raw.panel_id.filter(|s| !s.trim().is_empty());
        if let Some(ref pid) = panel_id {
            if !validid(pid) {
                diags.push(diag(
                    path,
                    0,
                    "panel id must use lowercase letters, numbers, `.` or `-`",
                ));
            }
        }
        Some(Panel {
            id: panel_id.unwrap_or_else(|| id.clone()),
            title: raw
                .panel_title
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| name.clone()),
            icon: raw
                .panel_icon
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| "\u{25c9}".to_string()),
        })
    } else {
        None
    };
    let webview = if raw.has_webview {
        build_webview(
            raw.webview_id,
            raw.webview_title,
            raw.webview_icon,
            raw.webview_placement,
            raw.webview_url,
            raw.webview_entry,
            raw.webview_boot,
            &id,
            &name,
            path,
            diags,
        )
    } else {
        None
    };
    let triggers = raw
        .triggers
        .into_iter()
        .filter_map(|t| build_trigger(t, path, diags))
        .collect();
    // Tools need a runtime to handle them; drop them (with a diagnostic) if none.
    let mut tools = Vec::new();
    if !raw.tools.is_empty() && runtime.is_none() {
        diags.push(diag(path, 0, "[[tool]] requires a [runtime] to handle it"));
    } else {
        for tool in raw.tools {
            if let Some(t) = build_tool(tool, path, diags) {
                tools.push(t);
            }
        }
    }
    Some(Plugin {
        name,
        version: raw.version.unwrap_or_else(|| "0.0.0".to_string()),
        description: raw.description.filter(|s| !s.trim().is_empty()),
        path: path
            .parent()
            .map(std::path::Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from(".")),
        id,
        commands,
        runtime,
        panel,
        webview,
        triggers,
        tools,
        capabilities: raw.capabilities,
    })
}

/// Assemble a validated [`Tool`], or `None` (with diagnostics) when the id is
/// missing/invalid or the description is empty.
fn build_tool(raw: RawTool, path: &std::path::Path, diags: &mut Vec<Diagnostic>) -> Option<Tool> {
    let id = required(raw.id, "tool `id`", path, raw.line, diags)?;
    if !validid(&id) {
        diags.push(diag(
            path,
            raw.line,
            "tool id must use lowercase letters, numbers, `.` or `-`",
        ));
        return None;
    }
    let description = raw.description.filter(|s| !s.trim().is_empty()).unwrap_or_else(|| {
        diags.push(diag(path, raw.line, "tool has no `description`"));
        String::new()
    });
    let params = raw.params.iter().filter_map(|p| parse_param(p)).collect();
    Some(Tool {
        id,
        description,
        params,
    })
}

/// Parse a `param` line: `name | type | description | required`. Only the name
/// is required; type defaults to `string`, and the 4th field being `required`
/// or `true` marks it required.
fn parse_param(spec: &str) -> Option<ToolParam> {
    let mut parts = spec.split('|').map(str::trim);
    let name = parts.next().filter(|s| !s.is_empty())?.to_string();
    let kind = match parts.next().unwrap_or("") {
        "" | "string" | "str" => "string",
        "number" | "float" => "number",
        "integer" | "int" => "integer",
        "boolean" | "bool" => "boolean",
        _ => "string",
    }
    .to_string();
    let description = parts.next().unwrap_or("").to_string();
    let required = matches!(parts.next().unwrap_or(""), "required" | "true" | "yes");
    Some(ToolParam {
        name,
        kind,
        description,
        required,
    })
}

/// Assemble a validated [`Trigger`], or `None` (with diagnostics) when the
/// event is unknown or the action is missing/ambiguous.
fn build_trigger(
    raw: RawTrigger,
    path: &std::path::Path,
    diags: &mut Vec<Diagnostic>,
) -> Option<Trigger> {
    let on = required(raw.on, "trigger `on` event", path, raw.line, diags)?;
    if !TRIGGER_EVENTS.contains(&on.as_str()) {
        diags.push(diag(
            path,
            raw.line,
            &format!("unknown trigger event `{on}` (one of {})", TRIGGER_EVENTS.join(", ")),
        ));
        return None;
    }
    let run = raw.run.filter(|s| !s.trim().is_empty());
    let notify = raw.notify.filter(|s| !s.trim().is_empty());
    let invoke = raw.invoke.filter(|s| !s.trim().is_empty());
    let count = [&run, &notify, &invoke].iter().filter(|o| o.is_some()).count();
    if count != 1 {
        diags.push(diag(
            path,
            raw.line,
            "a trigger needs exactly one action: `run`, `notify`, or `invoke`",
        ));
        return None;
    }
    let action = if let Some(text) = run {
        TriggerAction::Run {
            text,
            target: raw.target.unwrap_or_default(),
        }
    } else if let Some(text) = notify {
        TriggerAction::Notify { text }
    } else {
        TriggerAction::Invoke {
            method: invoke.expect("exactly one action"),
        }
    };
    Some(Trigger {
        on,
        when: raw.when.filter(|s| !s.trim().is_empty()),
        action,
    })
}

fn runtimekey(
    raw: &mut RawPlugin,
    key: &str,
    val: &str,
    path: &std::path::Path,
    line: usize,
    diags: &mut Vec<Diagnostic>,
) {
    match key {
        "command" => raw.runtime_command = Some(val.to_string()),
        "wasm" => raw.runtime_wasm = Some(val.to_string()),
        "type" => match RuntimeKind::parse(val) {
            Some(k) => raw.runtime_type = Some(k),
            None => diags.push(diag(path, line, "runtime type must be `process` or `wasm`")),
        },
        _ => diags.push(diag(path, line, &format!("unknown runtime key `{key}`"))),
    }
}

fn panelkey(
    raw: &mut RawPlugin,
    key: &str,
    val: &str,
    path: &std::path::Path,
    line: usize,
    diags: &mut Vec<Diagnostic>,
) {
    match key {
        "id" => raw.panel_id = Some(val.to_string()),
        "title" => raw.panel_title = Some(val.to_string()),
        "icon" => raw.panel_icon = Some(val.to_string()),
        _ => diags.push(diag(path, line, &format!("unknown panel key `{key}`"))),
    }
}

fn webviewkey(
    raw: &mut RawPlugin,
    key: &str,
    val: &str,
    path: &std::path::Path,
    line: usize,
    diags: &mut Vec<Diagnostic>,
) {
    match key {
        "id" => raw.webview_id = Some(val.to_string()),
        "title" => raw.webview_title = Some(val.to_string()),
        "icon" => raw.webview_icon = Some(val.to_string()),
        "placement" => match Placement::parse(val) {
            Some(p) => raw.webview_placement = Some(p),
            None => diags.push(diag(
                path,
                line,
                "webview placement must be `panel`, `tab`, or `window`",
            )),
        },
        "url" => raw.webview_url = Some(val.to_string()),
        "entry" => raw.webview_entry = Some(val.to_string()),
        "boot" => raw.webview_boot = matches!(val, "true" | "1" | "yes"),
        _ => diags.push(diag(path, line, &format!("unknown webview key `{key}`"))),
    }
}

/// Assemble a validated [`Webview`] from the raw `[webview]` keys, or `None`
/// (with diagnostics) when the source is missing or ambiguous.
#[allow(clippy::too_many_arguments)]
fn build_webview(
    raw_id: Option<String>,
    raw_title: Option<String>,
    raw_icon: Option<String>,
    placement: Option<Placement>,
    raw_url: Option<String>,
    raw_entry: Option<String>,
    boot: bool,
    id_default: &str,
    name_default: &str,
    path: &std::path::Path,
    diags: &mut Vec<Diagnostic>,
) -> Option<Webview> {
    let id = raw_id
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| id_default.to_string());
    if !validid(&id) {
        diags.push(diag(
            path,
            0,
            "webview id must use lowercase letters, numbers, `.` or `-`",
        ));
        return None;
    }
    let url = raw_url.filter(|s| !s.trim().is_empty());
    let entry = raw_entry.filter(|s| !s.trim().is_empty());
    let source = match (url, entry) {
        (Some(u), None) => WebviewSource::Url(u),
        (None, Some(e)) => WebviewSource::Entry(e),
        (Some(_), Some(_)) => {
            diags.push(diag(
                path,
                0,
                "[webview] needs exactly one of `url` or `entry`, not both",
            ));
            return None;
        }
        (None, None) => {
            diags.push(diag(path, 0, "[webview] requires a `url` or `entry`"));
            return None;
        }
    };
    Some(Webview {
        id,
        title: raw_title
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| name_default.to_string()),
        icon: raw_icon
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "\u{25f1}".to_string()),
        placement: placement.unwrap_or_default(),
        source,
        boot,
    })
}

fn buildcommand(
    raw: RawCommand,
    path: &std::path::Path,
    diags: &mut Vec<Diagnostic>,
) -> Option<Command> {
    let id = required(raw.id, "command id", path, raw.line, diags)?;
    if !validid(&id) {
        diags.push(diag(
            path,
            raw.line,
            "command id must use lowercase letters, numbers, `.` or `-`",
        ));
        return None;
    }
    let run = required(raw.run, "command run", path, raw.line, diags)?;
    if run.trim().is_empty() {
        diags.push(diag(path, raw.line, "command run cannot be empty"));
        return None;
    }
    Some(Command {
        title: raw.title.unwrap_or_else(|| id.clone()),
        id,
        run,
        mode: raw.mode.unwrap_or_default(),
        keybind: raw.keybind.filter(|s| !s.trim().is_empty()),
    })
}

fn required(
    value: Option<String>,
    name: &str,
    path: &std::path::Path,
    line: usize,
    diags: &mut Vec<Diagnostic>,
) -> Option<String> {
    match value.filter(|s| !s.trim().is_empty()) {
        Some(value) => Some(value),
        None => {
            diags.push(diag(path, line, &format!("missing {name}")));
            None
        }
    }
}

fn validid(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.' || b == b'-')
}

fn value(s: &str) -> String {
    let s = s.trim();
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        unescape(&s[1..s.len() - 1])
    } else {
        s.to_string()
    }
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

fn diag(path: &std::path::Path, line: usize, message: &str) -> Diagnostic {
    Diagnostic {
        path: path.to_path_buf(),
        line,
        message: message.to_string(),
    }
}

#[cfg(test)]
#[path = "../tests/manifest.rs"]
mod tests;
