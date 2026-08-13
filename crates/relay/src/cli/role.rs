//! Reusable agent roles: a name + brief (+ optional defaults) that `launch`
//! injects into the harness. Resolved highest-priority-first from the project
//! layer (the state dir's `roles/`), the user dir, then built-ins embedded in
//! the binary — see [`super::layered`].

use super::layered::{self, Source};
use super::RoleCmd;
use anyhow::{anyhow, bail, Result};
use serde::Deserialize;

const KIND: &str = "roles";

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
    /// Tool-access allow-list for agents in this role, passed to the agent CLI
    /// (`claude --allowedTools`). Each entry is a Claude Code tool rule, e.g.
    /// `"Read"`, `"Edit"`, `"Bash(git commit:*)"`, `"mcp__context7__*"`. Empty =
    /// no pre-grant (the agent falls back to your settings.json permissions).
    #[serde(default)]
    tools: Vec<String>,
}

#[derive(Clone)]
pub struct Role {
    pub name: String,
    pub channels: Vec<String>,
    pub agent: Option<String>,
    pub model: Option<String>,
    pub description: String,
    pub driver: bool,
    pub tools: Vec<String>,
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
        tools: f.tools,
        source,
    })
}

/// Resolve a role by name, project → user → built-in, with the project layer
/// in the ambient working directory (CLI use).
pub fn resolve(name: &str) -> Option<Role> {
    resolve_in(None, name)
}

/// Resolve with an explicit project root — the worker's cwd, passed through
/// `build::Options::role_root`. The daemon must always supply one: its own cwd
/// is meaningless (a Finder-launched app leaves it at `/`), and resolving there
/// silently hid every project-layer role.
pub fn resolve_in(root: Option<&std::path::Path>, name: &str) -> Option<Role> {
    let project = match root {
        Some(r) => layered::project_dir_in(r, KIND),
        None => layered::project_dir(KIND),
    };
    for (d, src) in [
        (project, Source::Project),
        (layered::user_dir(KIND), Source::User),
    ] {
        if let Ok(text) = std::fs::read_to_string(layered::file_in(&d, name)) {
            return parse(name, &text, src).ok();
        }
    }
    layered::builtin(BUILTINS, name).and_then(|text| parse(name, text, Source::Builtin).ok())
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
    let seen = layered::scan(KIND, BUILTINS);
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
    if !role.tools.is_empty() {
        println!("tools:   {}", role.tools.join(", "));
    }
    println!("---\n{}", role.description);
    Ok(())
}

fn scaffold(name: &str) -> String {
    if let Some(text) = layered::builtin(BUILTINS, name) {
        return text.to_string();
    }
    format!(
        "name = \"{name}\"\n\
         # channels = [\"{name}\"]\n\
         # agent = \"claude\"\n\
         # model = \"\"\n\
         # tools = [\"Read\", \"Edit\", \"Bash(git:*)\"]   # pre-granted tools (claude --allowedTools)\n\
         description = \"\"\"\n\
         Describe what the {name} role does, what it owns, and how it should\n\
         coordinate with the rest of the mesh.\n\
         \"\"\"\n"
    )
}

fn check(name: &str, text: &str) -> Result<()> {
    parse(name, text, Source::Project).map(|_| ())
}

fn create(name: &str, user: bool) -> Result<()> {
    if !layered::valid(name) {
        bail!("role name must be lowercase letters, digits, `.` or `-`");
    }
    let path = layered::file_in(&layered::dir(KIND, user), name);
    if path.exists() {
        bail!(
            "role `{name}` already exists at {} — use `edit`",
            path.display()
        );
    }
    layered::open_editor(&path, scaffold(name), &check)
}

fn edit(name: &str, user: bool) -> Result<()> {
    let target = layered::file_in(&layered::dir(KIND, user), name);
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
            bail!("`{name}` is a built-in role; create an override to change it");
        }
        bail!(
            "no {} role named `{name}`",
            if user { "user" } else { "project" }
        );
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
    if !role.tools.is_empty() {
        let list = role
            .tools
            .iter()
            .map(|t| format!("{:?}", t))
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!("tools = [{list}]\n"));
    }
    out.push_str(&format!(
        "description = \"\"\"\n{}\n\"\"\"\n",
        role.description
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The daemon used to resolve project roles against its own cwd (`/` when
    /// Finder-launched), so they silently never resolved. The launch pipeline
    /// passes the worker's root explicitly instead.
    #[test]
    fn resolve_in_uses_the_explicit_project_root() {
        let root = std::env::temp_dir().join(format!("relay-role-root-{}", std::process::id()));
        let dir = root.join(".relay").join("roles");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("shipit.toml"),
            "description = \"\"\"\nShip it.\n\"\"\"\n",
        )
        .unwrap();
        let role = resolve_in(Some(&root), "shipit").expect("project role under the given root");
        assert!(matches!(role.source, Source::Project));
        assert_eq!(role.description, "Ship it.");
        // Without the root, the ambient cwd has no such role.
        assert!(resolve("shipit").is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A serialized role must survive the round trip: `edit` seeds from the
    /// fully resolved layer, so what it writes has to parse back identically.
    #[test]
    fn serialize_round_trips_a_builtin() {
        let text = layered::builtin(BUILTINS, "supervisor").unwrap();
        let role = parse("supervisor", text, Source::Builtin).unwrap();
        let (driver, channels, tools) = (role.driver, role.channels.clone(), role.tools.clone());
        let desc = role.description.clone();
        let re = parse("supervisor", &serialize(role), Source::Project).unwrap();
        assert_eq!(re.driver, driver);
        assert_eq!(re.channels, channels);
        assert_eq!(re.tools, tools);
        assert_eq!(re.description, desc);
    }
}
