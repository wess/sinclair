//! Reusable agent roles: a name + brief (+ optional defaults) that `launch`
//! injects into the harness. Resolved highest-priority-first from the project
//! (`./.relay/roles`), the user dir, then built-ins embedded in the binary.

use super::RoleCmd;
use anyhow::{anyhow, bail, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// Built-in role templates, embedded at build time.
const BUILTINS: &[(&str, &str)] = &[
    ("supervisor", include_str!("../../roles/supervisor.toml")),
    ("worker", include_str!("../../roles/worker.toml")),
    ("frontend", include_str!("../../roles/frontend.toml")),
    ("backend", include_str!("../../roles/backend.toml")),
    ("reviewer", include_str!("../../roles/reviewer.toml")),
    ("devops", include_str!("../../roles/devops.toml")),
    ("qa", include_str!("../../roles/qa.toml")),
];

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

/// The TOML schema of a role file. Unknown fields (e.g. `name`) are ignored.
#[derive(Deserialize, Default)]
struct RoleFile {
    #[serde(default)]
    channels: Vec<String>,
    agent: Option<String>,
    model: Option<String>,
    #[serde(default)]
    description: String,
    /// A human-driven role (a supervisor/lead). It launches interactively rather
    /// than parking on the `wait`-loop, so the human can steer it. See
    /// [`crate::cli::agent::harness_prompt`].
    #[serde(default)]
    driver: bool,
}

#[derive(Clone)]
pub struct Role {
    pub name: String,
    pub channels: Vec<String>,
    pub agent: Option<String>,
    pub model: Option<String>,
    pub description: String,
    pub driver: bool,
    pub source: Source,
}

fn parse(name: &str, text: &str, source: Source) -> Result<Role> {
    let f: RoleFile = toml::from_str(text).map_err(|e| anyhow!("role `{name}`: {e}"))?;
    Ok(Role {
        name: name.to_string(),
        channels: f.channels,
        agent: f.agent,
        model: f.model,
        description: f.description.trim().to_string(),
        driver: f.driver,
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
    PathBuf::from(".relay").join("roles")
}

fn user_dir() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("relay").join("roles")
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

/// Resolve a role by name, project → user → built-in.
pub fn resolve(name: &str) -> Option<Role> {
    let p = file_in(&project_dir(), name);
    if let Ok(text) = std::fs::read_to_string(&p) {
        return parse(name, &text, Source::Project).ok();
    }
    let u = file_in(&user_dir(), name);
    if let Ok(text) = std::fs::read_to_string(&u) {
        return parse(name, &text, Source::User).ok();
    }
    builtin(name).and_then(|text| parse(name, text, Source::Builtin).ok())
}

/// Editor command: $VISUAL, then $EDITOR, then `vi`.
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

pub fn run(action: RoleCmd) -> Result<()> {
    match action {
        RoleCmd::List { json } => list(json),
        RoleCmd::Info { name } => info(&name),
        RoleCmd::Create { name, user } => create(&name, user),
        RoleCmd::Edit { name, user } => edit(&name, user),
        RoleCmd::Delete { name, user } => delete(&name, user),
    }
}

fn list(as_json: bool) -> Result<()> {
    use std::collections::BTreeMap;
    let mut seen: BTreeMap<String, Source> = BTreeMap::new();
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
    if as_json {
        let arr: Vec<_> = seen
            .iter()
            .map(|(n, s)| serde_json::json!({ "name": n, "source": s.label() }))
            .collect();
        println!("{}", serde_json::to_string(&arr)?);
    } else {
        for (name, src) in seen {
            println!("  {name:<14} [{}]", src.label());
        }
    }
    Ok(())
}

fn info(name: &str) -> Result<()> {
    let role = resolve(name).ok_or_else(|| anyhow!("no role named `{name}`"))?;
    println!("name:    {}", role.name);
    println!("source:  {}", role.source.label());
    if !role.channels.is_empty() {
        println!("channels: {}", role.channels.join(", "));
    }
    if let Some(a) = &role.agent {
        println!("agent:   {a}");
    }
    if let Some(m) = &role.model {
        println!("model:   {m}");
    }
    println!("---\n{}", role.description);
    Ok(())
}

fn scaffold(name: &str) -> String {
    if let Some(text) = builtin(name) {
        return text.to_string();
    }
    format!(
        "name = \"{name}\"\n\
         # channels = [\"{name}\"]\n\
         # agent = \"claude\"\n\
         # model = \"\"\n\
         description = \"\"\"\n\
         Describe what the {name} role does, what it owns, and how it should\n\
         coordinate with the rest of the mesh.\n\
         \"\"\"\n"
    )
}

fn create(name: &str, user: bool) -> Result<()> {
    if !valid(name) {
        bail!("role name must be lowercase letters, digits, `.` or `-`");
    }
    let path = file_in(&dir(user), name);
    if path.exists() {
        bail!("role `{name}` already exists at {} — use `edit`", path.display());
    }
    open_editor(&path, scaffold(name))
}

fn edit(name: &str, user: bool) -> Result<()> {
    let target = file_in(&dir(user), name);
    let seed = std::fs::read_to_string(&target)
        .ok()
        .or_else(|| resolve(name).map(serialize))
        .unwrap_or_else(|| scaffold(name));
    open_editor(&target, seed)
}

fn delete(name: &str, user: bool) -> Result<()> {
    let path = file_in(&dir(user), name);
    if !path.exists() {
        if builtin(name).is_some() {
            bail!("`{name}` is a built-in role; create an override to change it");
        }
        bail!("no {} role named `{name}`", if user { "user" } else { "project" });
    }
    std::fs::remove_file(&path)?;
    println!("deleted {}", path.display());
    Ok(())
}

/// Re-emit a resolved role as a TOML file body (used to seed copy-on-write edits).
fn serialize(role: Role) -> String {
    let mut out = format!("name = \"{}\"\n", role.name);
    if !role.channels.is_empty() {
        let list = role
            .channels
            .iter()
            .map(|c| format!("\"{c}\""))
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!("channels = [{list}]\n"));
    }
    if let Some(a) = &role.agent {
        out.push_str(&format!("agent = \"{a}\"\n"));
    }
    if let Some(m) = &role.model {
        out.push_str(&format!("model = \"{m}\"\n"));
    }
    if role.driver {
        out.push_str("driver = true\n");
    }
    out.push_str(&format!("description = \"\"\"\n{}\n\"\"\"\n", role.description));
    out
}

/// Open `$EDITOR` on a temp seed, validate it parses, then move it into place.
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
    let name = target
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("role");
    if let Err(e) = parse(name, &text, Source::Project) {
        bail!("{e}\nleft your draft at {}", tmp.display());
    }
    std::fs::rename(&tmp, target)?;
    println!("saved {}", target.display());
    Ok(())
}
