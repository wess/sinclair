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

enum Section {
    Plugin,
    Command(usize),
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
        _ => diags.push(diag(path, line, &format!("unknown plugin key `{key}`"))),
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
    Some(Plugin {
        name: raw.name.unwrap_or_else(|| id.clone()),
        version: raw.version.unwrap_or_else(|| "0.0.0".to_string()),
        description: raw.description.filter(|s| !s.trim().is_empty()),
        path: path
            .parent()
            .map(std::path::Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from(".")),
        id,
        commands,
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
