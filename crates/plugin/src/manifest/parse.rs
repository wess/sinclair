//! `plugin.toml` parsing: the line-oriented reader plus raw→validated `build`
//! step that turns manifest text into a [`Plugin`](super::model::Plugin).
//!
//! The parser intentionally supports a small TOML subset: root key/value pairs
//! and repeated `[[command]]`/`[[trigger]]`/`[[tool]]` tables. That keeps plugin
//! manifests easy to hand-write without a broad TOML dependency for this first
//! API.

use std::path::PathBuf;

use super::model::*;

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
