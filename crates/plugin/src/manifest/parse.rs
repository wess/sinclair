//! `plugin.toml` parsing: deserialize the manifest with `serde`/`toml`, then a
//! `build` step validates the raw form into a [`Plugin`](super::model::Plugin),
//! collecting friendly [`Diagnostic`]s instead of aborting.
//!
//! The manifest is real TOML. Multi-valued fields are arrays or tables:
//! `capabilities = ["a", "b"]` and `[[tool.param]]` — not the repeated bare keys
//! the first prototype used (which are not valid TOML).

use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::model::*;

#[derive(Deserialize, Default)]
struct RawManifest {
    id: Option<String>,
    name: Option<String>,
    version: Option<String>,
    description: Option<String>,
    #[serde(default)]
    capabilities: Vec<String>,
    runtime: Option<RawRuntime>,
    panel: Option<RawPanel>,
    #[serde(default)]
    command: Vec<RawCommand>,
    #[serde(default)]
    trigger: Vec<RawTrigger>,
    #[serde(default)]
    tool: Vec<RawTool>,
}

#[derive(Deserialize)]
struct RawRuntime {
    #[serde(rename = "type")]
    kind: Option<String>,
    /// Retired. Still deserialized so a manifest carrying it gets a diagnostic
    /// naming the removal rather than a confusing downstream error.
    command: Option<String>,
    wasm: Option<String>,
}

#[derive(Deserialize)]
struct RawPanel {
    id: Option<String>,
    title: Option<String>,
    icon: Option<String>,
}

#[derive(Deserialize)]
struct RawCommand {
    id: Option<String>,
    title: Option<String>,
    run: Option<String>,
    mode: Option<String>,
    keybind: Option<String>,
}

#[derive(Deserialize)]
struct RawTrigger {
    on: Option<String>,
    when: Option<String>,
    run: Option<String>,
    target: Option<String>,
    notify: Option<String>,
    invoke: Option<String>,
}

#[derive(Deserialize)]
struct RawTool {
    id: Option<String>,
    description: Option<String>,
    #[serde(default)]
    param: Vec<RawParam>,
}

/// A `[[tool.param]]` table, or a legacy `"name | type | desc | required"`
/// pipe-string (accepted with a deprecation nudge).
#[derive(Deserialize)]
#[serde(untagged)]
enum RawParam {
    Pipe(String),
    Table {
        name: String,
        #[serde(rename = "type")]
        kind: Option<String>,
        #[serde(default)]
        description: String,
        #[serde(default)]
        required: bool,
    },
}

pub fn parse(path: PathBuf, text: &str) -> (Option<Plugin>, Vec<Diagnostic>) {
    let mut diags = Vec::new();
    let raw: RawManifest = match toml::from_str(text) {
        Ok(raw) => raw,
        Err(error) => {
            let mut message = format!("invalid plugin.toml: {error}");
            if error.to_string().contains("duplicate key") {
                message.push_str(
                    "\n  (multi-valued fields are arrays/tables now: \
                     `capabilities = [\"a\", \"b\"]`, `[[tool.param]]`)",
                );
            }
            diags.push(Diagnostic { path, line: 0, message });
            return (None, diags);
        }
    };
    let plugin = build(raw, &path, &mut diags);
    (plugin, diags)
}

fn build(raw: RawManifest, path: &Path, diags: &mut Vec<Diagnostic>) -> Option<Plugin> {
    let id = required(raw.id, "id", path, diags)?;
    if !validid(&id) {
        diags.push(diag(path, "plugin id must use lowercase letters, numbers, `.` or `-`"));
        return None;
    }
    let name = raw.name.filter(nonblank).unwrap_or_else(|| id.clone());

    let capabilities = build_capabilities(raw.capabilities, path, diags);
    let commands = raw
        .command
        .into_iter()
        .filter_map(|c| build_command(c, path, diags))
        .collect();
    let runtime = raw.runtime.and_then(|r| build_runtime(r, path, diags));
    let panel = raw.panel.map(|p| build_panel(p, &id, &name, path, diags));
    let triggers = raw
        .trigger
        .into_iter()
        .filter_map(|t| build_trigger(t, path, diags))
        .collect();
    // Tools need a runtime to handle them; drop them (with a diagnostic) if none.
    let tools = if !raw.tool.is_empty() && runtime.is_none() {
        diags.push(diag(path, "[[tool]] requires a [runtime] to handle it"));
        Vec::new()
    } else {
        raw.tool
            .into_iter()
            .filter_map(|t| build_tool(t, path, diags))
            .collect()
    };

    Some(Plugin {
        id,
        name,
        version: raw.version.filter(nonblank).unwrap_or_else(|| "0.0.0".to_string()),
        description: raw.description.filter(nonblank),
        path: path.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from(".")),
        commands,
        runtime,
        panel,
        triggers,
        tools,
        capabilities,
    })
}

fn build_capabilities(raw: Vec<String>, path: &Path, diags: &mut Vec<Diagnostic>) -> Vec<String> {
    let mut caps = Vec::new();
    for cap in raw {
        if !CAPABILITIES.contains(&cap.as_str()) {
            diags.push(diag(
                path,
                &format!("unknown capability `{cap}` (one of {})", CAPABILITIES.join(", ")),
            ));
        } else if !caps.contains(&cap) {
            caps.push(cap);
        }
    }
    caps
}

fn build_runtime(raw: RawRuntime, path: &Path, diags: &mut Vec<Diagnostic>) -> Option<Runtime> {
    // `type` is now always wasm. Name the removal rather than ignoring the key,
    // so a manifest written against the old subprocess runtime says why it
    // stopped working instead of failing later with "no wasm module".
    if let Some(kind) = raw.kind.as_deref().filter(|k| !k.is_empty()) {
        if !matches!(kind, "wasm" | "wasm32" | "webassembly") {
            diags.push(diag(
                path,
                "`[runtime] type` must be `wasm` — subprocess plugin runtimes were \
                 removed; see docs/pluginauthoring.md for porting",
            ));
            return None;
        }
    }
    if raw.command.as_deref().is_some_and(|c| !c.trim().is_empty()) {
        diags.push(diag(
            path,
            "`[runtime] command` is no longer supported — a plugin is a WASM \
             component; set `wasm = \"plugin.wasm\"` instead",
        ));
        return None;
    }
    let Some(wasm) = raw.wasm.filter(nonblank) else {
        diags.push(diag(path, "[runtime] requires a `wasm` module path"));
        return None;
    };
    if !contained(&wasm) {
        diags.push(diag(
            path,
            "the `wasm` module path must stay inside the plugin folder \
             (no absolute paths or `..`)",
        ));
        return None;
    }
    Some(Runtime { wasm })
}

fn build_panel(raw: RawPanel, id: &str, name: &str, path: &Path, diags: &mut Vec<Diagnostic>) -> Panel {
    let pid = raw.id.filter(nonblank).unwrap_or_else(|| id.to_string());
    if !validid(&pid) {
        diags.push(diag(path, "panel id must use lowercase letters, numbers, `.` or `-`"));
    }
    Panel {
        id: pid,
        title: raw.title.filter(nonblank).unwrap_or_else(|| name.to_string()),
        icon: raw.icon.filter(nonblank).unwrap_or_else(|| "\u{25c9}".to_string()),
    }
}

fn build_command(raw: RawCommand, path: &Path, diags: &mut Vec<Diagnostic>) -> Option<Command> {
    let id = required(raw.id, "command id", path, diags)?;
    if !validid(&id) {
        diags.push(diag(path, "command id must use lowercase letters, numbers, `.` or `-`"));
        return None;
    }
    let run = required(raw.run, "command run", path, diags)?;
    let mode = raw
        .mode
        .as_deref()
        .map(|m| CommandMode::parse(m).unwrap_or_else(|| {
            diags.push(diag(path, "invalid command mode"));
            CommandMode::default()
        }))
        .unwrap_or_default();
    Some(Command {
        title: raw.title.filter(nonblank).unwrap_or_else(|| id.clone()),
        id,
        run,
        mode,
        keybind: raw.keybind.filter(nonblank),
    })
}

fn build_trigger(raw: RawTrigger, path: &Path, diags: &mut Vec<Diagnostic>) -> Option<Trigger> {
    let on = required(raw.on, "trigger `on` event", path, diags)?;
    if !TRIGGER_EVENTS.contains(&on.as_str()) {
        diags.push(diag(
            path,
            &format!("unknown trigger event `{on}` (one of {})", TRIGGER_EVENTS.join(", ")),
        ));
        return None;
    }
    let run = raw.run.filter(nonblank);
    let notify = raw.notify.filter(nonblank);
    let invoke = raw.invoke.filter(nonblank);
    if [&run, &notify, &invoke].iter().filter(|o| o.is_some()).count() != 1 {
        diags.push(diag(path, "a trigger needs exactly one action: `run`, `notify`, or `invoke`"));
        return None;
    }
    let target = raw
        .target
        .as_deref()
        .and_then(|t| {
            TriggerTarget::parse(t).or_else(|| {
                diags.push(diag(path, "invalid trigger target"));
                None
            })
        })
        .unwrap_or_default();
    let action = if let Some(text) = run {
        TriggerAction::Run { text, target }
    } else if let Some(text) = notify {
        TriggerAction::Notify { text }
    } else {
        TriggerAction::Invoke { method: invoke.expect("exactly one action") }
    };
    Some(Trigger { on, when: raw.when.filter(nonblank), action })
}

fn build_tool(raw: RawTool, path: &Path, diags: &mut Vec<Diagnostic>) -> Option<Tool> {
    let id = required(raw.id, "tool `id`", path, diags)?;
    if !validid(&id) {
        diags.push(diag(path, "tool id must use lowercase letters, numbers, `.` or `-`"));
        return None;
    }
    let description = raw.description.filter(nonblank).unwrap_or_else(|| {
        diags.push(diag(path, "tool has no `description`"));
        String::new()
    });
    let params = raw.param.into_iter().filter_map(param).collect();
    Some(Tool { id, description, params })
}

/// Convert a raw param (table or legacy pipe-string) into a [`ToolParam`].
fn param(raw: RawParam) -> Option<ToolParam> {
    match raw {
        RawParam::Pipe(spec) => parse_pipe_param(&spec),
        RawParam::Table { name, kind, description, required } => Some(ToolParam {
            name,
            kind: normalize_kind(kind.as_deref().unwrap_or("")),
            description,
            required,
        }),
    }
}

/// Legacy `name | type | description | required` form. Only the name is required.
fn parse_pipe_param(spec: &str) -> Option<ToolParam> {
    let mut parts = spec.split('|').map(str::trim);
    let name = parts.next().filter(|s| !s.is_empty())?.to_string();
    let kind = normalize_kind(parts.next().unwrap_or(""));
    let description = parts.next().unwrap_or("").to_string();
    let required = matches!(parts.next().unwrap_or(""), "required" | "true" | "yes");
    Some(ToolParam { name, kind, description, required })
}

fn normalize_kind(kind: &str) -> String {
    match kind {
        "number" | "float" => "number",
        "integer" | "int" => "integer",
        "boolean" | "bool" => "boolean",
        _ => "string",
    }
    .to_string()
}

fn required(value: Option<String>, name: &str, path: &Path, diags: &mut Vec<Diagnostic>) -> Option<String> {
    match value.filter(nonblank) {
        Some(value) => Some(value),
        None => {
            diags.push(diag(path, &format!("missing {name}")));
            None
        }
    }
}

/// Predicate for `Option::<String>::filter`, which hands the closure a `&String`.
#[allow(clippy::ptr_arg)]
fn nonblank(s: &String) -> bool {
    !s.trim().is_empty()
}

fn validid(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.' || b == b'-')
}

/// A manifest file path the host will join onto the plugin dir: it must be
/// relative and free of `..`, so it can't escape the plugin's own folder.
fn contained(path: &str) -> bool {
    use std::path::Component;
    Path::new(path).components().all(|c| {
        !matches!(c, Component::RootDir | Component::Prefix(_) | Component::ParentDir)
    })
}

fn diag(path: &Path, message: &str) -> Diagnostic {
    Diagnostic { path: path.to_path_buf(), line: 0, message: message.to_string() }
}
