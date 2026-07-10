//! Teams: parsing and saving team specs, generating them via an agent, and
//! launching team members into splits.

use super::*;

/// Names of available teams (built-in + user + project), via the relay CLI.
pub fn team_list() -> Vec<String> {
    let Ok(out) = std::process::Command::new(binary())
        .args(["team", "list", "--json"])
        .output()
    else {
        return Vec::new();
    };
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or(serde_json::Value::Null);
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|t| t["name"].as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

/// One team member from `team info`: `(name, role, agent)`. `agent` is empty
/// when the member doesn't override the role's default provider.
pub type TeamMember = (String, String, String);

/// A team's layout shape and ordered roster.
pub fn team_info(name: &str) -> Option<(String, Vec<TeamMember>)> {
    let out = std::process::Command::new(binary())
        .args(["team", "info", name, "--json"])
        .output()
        .ok()?;
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    let layout = v["layout"].as_str()?.to_string();
    let members = v["members"]
        .as_array()?
        .iter()
        .map(|m| {
            (
                m["name"].as_str().unwrap_or("agent").to_string(),
                m["role"].as_str().unwrap_or("worker").to_string(),
                m["agent"].as_str().unwrap_or("").to_string(),
            )
        })
        .collect();
    Some((layout, members))
}

/// Persist a team through `relay team save` (JSON on stdin), so relay stays the
/// owner of team storage. `--user` writes to the user dir; otherwise it's the
/// project dir resolved against `cwd` (the focused pane's directory). Returns
/// the saved team name on success, or an error message.
pub fn save_team(spec: &TeamSpec, user: bool, cwd: Option<&std::path::Path>) -> Result<String, String> {
    use std::io::Write;
    let json = serde_json::to_string(spec).map_err(|e| e.to_string())?;
    let mut cmd = std::process::Command::new(binary());
    cmd.arg("team").arg("save");
    if user {
        cmd.arg("--user");
    } else if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    cmd.stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| format!("could not run relay: {e}"))?;
    child
        .stdin
        .take()
        .ok_or("no stdin")?
        .write_all(json.as_bytes())
        .map_err(|e| e.to_string())?;
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(spec.name.clone())
    } else {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Err(if err.is_empty() { "team save failed".into() } else { err })
    }
}

/// The configured Claude binary (explicit path, else `claude` on PATH).
fn claude_binary(opts: &config::Options) -> String {
    opts.agent_claude_path
        .clone()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "claude".to_string())
}

/// Ask Claude (one-shot `-p`) to design a team from a plain-English description,
/// returning a parsed [`TeamSpec`] for review. Blocking — run it off the UI
/// thread. `roles` seeds the prompt with the roles available on this machine.
pub fn generate_team(
    opts: &config::Options,
    roles: &[String],
    description: &str,
) -> Result<TeamSpec, String> {
    let bin = claude_binary(opts);
    let prompt = team_prompt(roles, description);
    let out = std::process::Command::new(&bin)
        .arg("-p")
        .arg(&prompt)
        .output()
        .map_err(|e| format!("could not run `{bin}`: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let err = err.trim();
        return Err(if err.is_empty() {
            format!("`{bin} -p` failed")
        } else {
            err.to_string()
        });
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let json = extract_json(&text).ok_or("the model didn't return JSON")?;
    let mut spec: TeamSpec =
        serde_json::from_str(json).map_err(|e| format!("couldn't parse the team: {e}"))?;
    if !TEAM_SHAPES.contains(&spec.layout.as_str()) {
        spec.layout = "columns".to_string();
    }
    spec.members.retain(|m| !m.name.trim().is_empty());
    if spec.members.is_empty() {
        return Err("the model returned no members".into());
    }
    Ok(spec)
}

/// Ask Claude for a single next-command suggestion completing `input`, given
/// recent history for context. Returns the full command line (which must start
/// with `input`) or `None`. Blocking — run off the UI thread.
pub fn suggest_command(
    opts: &config::Options,
    recent: &[String],
    input: &str,
) -> Option<String> {
    let bin = claude_binary(opts);
    let history = recent.join("\n");
    let prompt = format!(
        "You complete a shell command line. Recent commands (newest first):\n{history}\n\n\
         The user has typed: {input}\n\n\
         Reply with ONLY the single most likely full command line, starting with exactly \
         what they typed, no prose, no backticks, no explanation."
    );
    let out = std::process::Command::new(&bin).arg("-p").arg(&prompt).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let line = String::from_utf8_lossy(&out.stdout);
    let line = line.lines().find(|l| !l.trim().is_empty())?.trim();
    let line = line.trim_matches('`').trim();
    (line.starts_with(input) && line.len() > input.len()).then(|| line.to_string())
}

/// The generation prompt: constrain Claude to emit only the team JSON.
fn team_prompt(roles: &[String], description: &str) -> String {
    let roles = if roles.is_empty() {
        "supervisor, worker, frontend, backend, reviewer, devops, qa".to_string()
    } else {
        roles.join(", ")
    };
    format!(
        "You design small teams of coding agents for the Relay mesh. \
         Available roles: {roles}. Available layouts: {}. \
         Design a team for this request:\n\n{description}\n\n\
         Reply with ONLY a JSON object, no prose, no code fences, of the form: \
         {{\"name\":\"kebab-case-name\",\"layout\":\"one-of-the-layouts\",\
         \"members\":[{{\"name\":\"short-name\",\"role\":\"one-of-the-roles\"}}]}}. \
         Make the first member a supervisor that drives the team. Keep it to 2-5 members.",
        TEAM_SHAPES.join(", ")
    )
}

/// Pull the first balanced `{...}` block out of a model reply (it may wrap the
/// JSON in prose or code fences).
pub(crate) fn extract_json(text: &str) -> Option<&str> {
    let start = text.find('{')?;
    let mut depth = 0usize;
    for (i, c) in text[start..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&text[start..start + i + 1]);
                }
            }
            _ => {}
        }
    }
    None
}

/// Shell command that launches one team member in a pane. The team's first
/// member is the human-driven `lead`, it stays interactive instead of parking
/// on the `wait`-loop, so the human can steer it. `agent` overrides the role's
/// default provider when set (issue #8).
pub fn launch_member(member: &str, role: &str, agent: &str, lead: bool, optimize: bool) -> String {
    let flag = if lead { " --lead" } else { "" };
    let opt = if optimize { " --optimize" } else { "" };
    let agent = agent.trim();
    let agent_flag = if agent.is_empty() {
        String::new()
    } else {
        format!(" --agent {}", sh_quote(agent))
    };
    keep_open(format!(
        "{} --home {} launch {} --role {}{agent_flag}{flag}{opt}",
        sh_quote(&binary()),
        sh_quote(&home_str()),
        sh_quote(member),
        sh_quote(role)
    ))
}
