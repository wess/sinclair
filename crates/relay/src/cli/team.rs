//! Teams: a named layout plus a roster of members (name + role). Relay owns the
//! definition; Sinclair reads it (via `--json`) to open a tiled set of agents.
//! Resolved project → user → built-in, like roles.

use super::TeamCmd;
use anyhow::{anyhow, bail, Result};
use serde::Deserialize;
use serde_json::json;
use std::path::{Path, PathBuf};

const BUILTINS: &[(&str, &str)] = &[
    ("web", include_str!("../../teams/web.toml")),
    ("pair", include_str!("../../teams/pair.toml")),
];

/// Layout shapes Sinclair's tile engine understands.
const SHAPES: &[&str] = &["columns", "rows", "grid", "main-bottom", "main-right"];

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Project,
    User,
    Builtin,
}

impl Source {
    fn label(self) -> &'static str {
        match self {
            Source::Project => "project",
            Source::User => "user",
            Source::Builtin => "built-in",
        }
    }
}

#[derive(Deserialize, Default)]
struct TeamFile {
    #[serde(default)]
    layout: String,
    #[serde(default)]
    member: Vec<Member>,
}

#[derive(Deserialize, Clone)]
pub struct Member {
    pub name: String,
    pub role: Option<String>,
    pub agent: Option<String>,
}

pub struct Team {
    pub name: String,
    pub layout: String,
    pub members: Vec<Member>,
    pub source: Source,
}

fn parse(name: &str, text: &str, source: Source) -> Result<Team> {
    let f: TeamFile = toml::from_str(text).map_err(|e| anyhow!("team `{name}`: {e}"))?;
    if f.member.is_empty() {
        bail!("team `{name}` has no members");
    }
    let layout = if f.layout.trim().is_empty() {
        "columns".to_string()
    } else {
        f.layout.trim().to_string()
    };
    Ok(Team {
        name: name.to_string(),
        layout,
        members: f.member,
        source,
    })
}

fn valid(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'.')
}

fn project_dir() -> PathBuf {
    PathBuf::from(".relay").join("teams")
}

fn user_dir() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("relay").join("teams")
}

fn dir(user: bool) -> PathBuf {
    if user {
        user_dir()
    } else {
        project_dir()
    }
}

fn file_in(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("{name}.toml"))
}

fn builtin(name: &str) -> Option<&'static str> {
    BUILTINS.iter().find(|(n, _)| *n == name).map(|(_, t)| *t)
}

pub fn resolve(name: &str) -> Option<Team> {
    for (d, src) in [(project_dir(), Source::Project), (user_dir(), Source::User)] {
        if let Ok(text) = std::fs::read_to_string(file_in(&d, name)) {
            return parse(name, &text, src).ok();
        }
    }
    builtin(name).and_then(|text| parse(name, text, Source::Builtin).ok())
}

fn editor() -> String {
    for var in ["VISUAL", "EDITOR"] {
        if let Ok(v) = std::env::var(var) {
            if !v.trim().is_empty() {
                return v;
            }
        }
    }
    "vi".to_string()
}

pub fn run(action: TeamCmd) -> Result<()> {
    match action {
        TeamCmd::List { json } => list(json),
        TeamCmd::Info { name, json } => info(&name, json),
        TeamCmd::Create { name, user } => create(&name, user),
        TeamCmd::Edit { name, user } => edit(&name, user),
        TeamCmd::Delete { name, user } => delete(&name, user),
        TeamCmd::Save { user } => save(user),
    }
}

/// The JSON a host (Sinclair's team builder) pipes in on stdin to persist a team
/// without an editor.
#[derive(Deserialize)]
struct SaveSpec {
    name: String,
    #[serde(default)]
    layout: String,
    #[serde(default)]
    members: Vec<SaveMember>,
}

#[derive(Deserialize)]
struct SaveMember {
    name: String,
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    agent: Option<String>,
}

/// Render a team spec into a `.toml` file, non-interactively. Reads the spec as
/// JSON on stdin so an arbitrary roster fits without shell-quoting gymnastics.
fn save(user: bool) -> Result<()> {
    let mut raw = String::new();
    std::io::Read::read_to_string(&mut std::io::stdin(), &mut raw)?;
    let spec: SaveSpec = serde_json::from_str(&raw).map_err(|e| anyhow!("bad team JSON: {e}"))?;
    let name = spec.name.trim();
    if !valid(name) {
        bail!("team name must be lowercase letters, digits, `.` or `-`");
    }
    let members: Vec<SaveMember> = spec
        .members
        .into_iter()
        .filter(|m| !m.name.trim().is_empty())
        .collect();
    if members.is_empty() {
        bail!("team `{name}` needs at least one member");
    }
    let layout = spec.layout.trim();
    let layout = if SHAPES.contains(&layout) { layout } else { "columns" };
    let toml = render_toml(name, layout, &members);
    // Validate what we're about to write with the same parser `resolve` uses.
    parse(name, &toml, Source::User)?;
    let path = file_in(&dir(user), name);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, toml)?;
    println!("saved {}", path.display());
    Ok(())
}

/// Build the team TOML, quoting values and omitting empty role/agent.
fn render_toml(name: &str, layout: &str, members: &[SaveMember]) -> String {
    let q = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    let mut out = format!("name = \"{}\"\nlayout = \"{}\"\n", q(name), q(layout));
    for m in members {
        out.push_str(&format!("\n[[member]]\nname = \"{}\"\n", q(m.name.trim())));
        if let Some(role) = m.role.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            out.push_str(&format!("role = \"{}\"\n", q(role)));
        }
        if let Some(agent) = m.agent.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            out.push_str(&format!("agent = \"{}\"\n", q(agent)));
        }
    }
    out
}

fn all_names() -> std::collections::BTreeMap<String, Source> {
    let mut seen = std::collections::BTreeMap::new();
    for (n, _) in BUILTINS {
        seen.insert((*n).to_string(), Source::Builtin);
    }
    for (d, src) in [(user_dir(), Source::User), (project_dir(), Source::Project)] {
        if let Ok(entries) = std::fs::read_dir(&d) {
            for e in entries.flatten() {
                let path = e.path();
                if path.extension().and_then(|x| x.to_str()) == Some("toml") {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        seen.insert(stem.to_string(), src);
                    }
                }
            }
        }
    }
    seen
}

fn list(as_json: bool) -> Result<()> {
    let names = all_names();
    if as_json {
        let arr: Vec<_> = names
            .iter()
            .map(|(n, s)| json!({ "name": n, "source": s.label() }))
            .collect();
        println!("{}", serde_json::to_string(&arr)?);
    } else {
        for (name, src) in names {
            println!("  {name:<14} [{}]", src.label());
        }
    }
    Ok(())
}

fn info(name: &str, as_json: bool) -> Result<()> {
    let team = resolve(name).ok_or_else(|| anyhow!("no team named `{name}`"))?;
    if as_json {
        let members: Vec<_> = team
            .members
            .iter()
            .map(|m| json!({ "name": m.name, "role": m.role, "agent": m.agent }))
            .collect();
        println!(
            "{}",
            serde_json::to_string(&json!({
                "name": team.name,
                "layout": team.layout,
                "members": members,
            }))?
        );
    } else {
        println!("name:   {}", team.name);
        println!("source: {}", team.source.label());
        println!("layout: {}", team.layout);
        println!("members:");
        for m in &team.members {
            let role = m.role.as_deref().unwrap_or("worker");
            println!("  {:<12} {}", m.name, role);
        }
    }
    Ok(())
}

fn scaffold(name: &str) -> String {
    if let Some(text) = builtin(name) {
        return text.to_string();
    }
    format!(
        "name = \"{name}\"\n\
         layout = \"main-bottom\"   # {}\n\n\
         [[member]]\n\
         name = \"lead\"\n\
         role = \"supervisor\"\n\n\
         [[member]]\n\
         name = \"worker1\"\n\
         role = \"worker\"\n",
        SHAPES.join(" | ")
    )
}

fn create(name: &str, user: bool) -> Result<()> {
    if !valid(name) {
        bail!("team name must be lowercase letters, digits, `.` or `-`");
    }
    let path = file_in(&dir(user), name);
    if path.exists() {
        bail!("team `{name}` already exists at {} — use `edit`", path.display());
    }
    open_editor(&path, scaffold(name))
}

fn edit(name: &str, user: bool) -> Result<()> {
    let target = file_in(&dir(user), name);
    let seed = std::fs::read_to_string(&target)
        .ok()
        .or_else(|| builtin(name).map(str::to_string))
        .unwrap_or_else(|| scaffold(name));
    open_editor(&target, seed)
}

fn delete(name: &str, user: bool) -> Result<()> {
    let path = file_in(&dir(user), name);
    if !path.exists() {
        if builtin(name).is_some() {
            bail!("`{name}` is a built-in team; create an override to change it");
        }
        bail!("no {} team named `{name}`", if user { "user" } else { "project" });
    }
    std::fs::remove_file(&path)?;
    println!("deleted {}", path.display());
    Ok(())
}

fn open_editor(target: &PathBuf, seed: String) -> Result<()> {
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = target.with_extension("toml.tmp");
    std::fs::write(&tmp, seed)?;
    let status = std::process::Command::new(editor()).arg(&tmp).status()?;
    if !status.success() {
        let _ = std::fs::remove_file(&tmp);
        bail!("editor exited without saving");
    }
    let text = std::fs::read_to_string(&tmp)?;
    let name = target.file_stem().and_then(|s| s.to_str()).unwrap_or("team");
    if let Err(e) = parse(name, &text, Source::Project) {
        bail!("{e}\nleft your draft at {}", tmp.display());
    }
    std::fs::rename(&tmp, target)?;
    println!("saved {}", target.display());
    Ok(())
}
