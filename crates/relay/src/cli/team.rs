//! Teams: a named layout plus a roster of members (name + role). Relay owns the
//! definition; Sinclair reads it (via `--json`) to open a tiled set of agents.
//! Resolved project → user → built-in, like roles — see [`super::layered`].

use super::layered::{self, Source};
use super::TeamCmd;
use anyhow::{anyhow, bail, Result};
use serde::Deserialize;
use serde_json::json;

const KIND: &str = "teams";

const BUILTINS: &[(&str, &str)] = &[
    ("web", include_str!("../../teams/web.toml")),
    ("pair", include_str!("../../teams/pair.toml")),
];

/// Layout shapes Sinclair's tile engine understands.
const SHAPES: &[&str] = &["columns", "rows", "grid", "main-bottom", "main-right"];

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

pub fn resolve(name: &str) -> Option<Team> {
    for (d, src) in [
        (layered::project_dir(KIND), Source::Project),
        (layered::user_dir(KIND), Source::User),
    ] {
        if let Ok(text) = std::fs::read_to_string(layered::file_in(&d, name)) {
            return parse(name, &text, src).ok();
        }
    }
    layered::builtin(BUILTINS, name).and_then(|text| parse(name, text, Source::Builtin).ok())
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
    if !layered::valid(name) {
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
    let layout = if SHAPES.contains(&layout) {
        layout
    } else {
        "columns"
    };
    let toml = render_toml(name, layout, &members);
    // Validate what we're about to write with the same parser `resolve` uses.
    parse(name, &toml, Source::User)?;
    let path = layered::file_in(&layered::dir(KIND, user), name);
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

fn list(as_json: bool) -> Result<()> {
    let names = layered::scan(KIND, BUILTINS);
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
    if let Some(text) = layered::builtin(BUILTINS, name) {
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

fn check(name: &str, text: &str) -> Result<()> {
    parse(name, text, Source::Project).map(|_| ())
}

fn create(name: &str, user: bool) -> Result<()> {
    if !layered::valid(name) {
        bail!("team name must be lowercase letters, digits, `.` or `-`");
    }
    let path = layered::file_in(&layered::dir(KIND, user), name);
    if path.exists() {
        bail!(
            "team `{name}` already exists at {} — use `edit`",
            path.display()
        );
    }
    layered::open_editor(&path, scaffold(name), &check)
}

fn edit(name: &str, user: bool) -> Result<()> {
    let target = layered::file_in(&layered::dir(KIND, user), name);
    // Seed from the fully resolved team (project → user → built-in), the same
    // copy-on-write `role edit` does, so editing an overridden team starts from
    // what is actually in effect rather than the pristine built-in.
    let seed = std::fs::read_to_string(&target)
        .ok()
        .or_else(|| resolve(name).map(serialize))
        .unwrap_or_else(|| scaffold(name));
    layered::open_editor(&target, seed, &check)
}

fn delete(name: &str, user: bool) -> Result<()> {
    let path = layered::file_in(&layered::dir(KIND, user), name);
    if !path.exists() {
        if layered::builtin(BUILTINS, name).is_some() {
            bail!("`{name}` is a built-in team; create an override to change it");
        }
        bail!(
            "no {} team named `{name}`",
            if user { "user" } else { "project" }
        );
    }
    std::fs::remove_file(&path)?;
    println!("deleted {}", path.display());
    Ok(())
}

/// Re-emit a resolved team as a TOML file body (seeds copy-on-write edits).
fn serialize(team: Team) -> String {
    let members: Vec<SaveMember> = team
        .members
        .into_iter()
        .map(|m| SaveMember {
            name: m.name,
            role: m.role,
            agent: m.agent,
        })
        .collect();
    render_toml(&team.name, &team.layout, &members)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `edit` seed must round-trip: a resolved team serialized back to TOML
    /// has to parse to the same roster and layout.
    #[test]
    fn serialize_round_trips_a_builtin() {
        let text = layered::builtin(BUILTINS, "web").unwrap();
        let team = parse("web", text, Source::Builtin).unwrap();
        let layout = team.layout.clone();
        let roster: Vec<(String, Option<String>)> = team
            .members
            .iter()
            .map(|m| (m.name.clone(), m.role.clone()))
            .collect();
        let re = parse("web", &serialize(team), Source::Project).unwrap();
        assert_eq!(re.layout, layout);
        let re_roster: Vec<(String, Option<String>)> = re
            .members
            .iter()
            .map(|m| (m.name.clone(), m.role.clone()))
            .collect();
        assert_eq!(re_roster, roster);
    }
}
