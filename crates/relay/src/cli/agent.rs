use anyhow::{anyhow, Result};

/// A resolved command to run an agent.
pub struct Launch {
    pub program: String,
    pub args: Vec<String>,
}

/// Inputs needed to construct an agent launch.
pub struct Spec<'a> {
    pub agent: &'a str,
    pub custom: Option<&'a str>,
    pub name: &'a str,
    pub role: &'a str,
    pub prompt: &'a str,
    pub mcp_file: &'a str,
    pub url: &'a str,
    pub headless: bool,
    pub model: Option<&'a str>,
    pub channels: &'a [String],
    pub skip_perms: bool,
}

/// The harness an agent receives as its opening instruction. `brief` is the
/// optional role description; `task` is the per-launch focus.
///
/// `interactive` picks the shape. A parked worker registers and then blocks on
/// `wait`, costing nothing until a teammate sends it work. A `driver` (the
/// human-facing lead of a team, or a supervisor launched on its own) instead
/// stays interactive: it registers, then hands control back to the human in its
/// terminal and only calls `wait` to gather replies *after* it has delegated, 
/// never parking the human out of their own session.
pub fn harness_prompt(
    name: &str,
    role: &str,
    brief: &str,
    channels: &[String],
    task: Option<&str>,
    interactive: bool,
) -> String {
    let join = if channels.is_empty() {
        String::new()
    } else {
        format!("- After registering, `join` these channels: {}.\n", channels.join(", "))
    };
    let brief = if brief.trim().is_empty() {
        String::new()
    } else {
        format!("\nYour role:\n{}\n", brief.trim())
    };
    let task = task
        .filter(|t| !t.trim().is_empty())
        .map(|t| format!("\nYour standing focus: {}\n", t.trim()))
        .unwrap_or_default();
    let protocol = if interactive {
        "Protocol — follow exactly:\n\
         - Call `register` with name=\"{name}\" and role=\"{role}\" first.\n\
         {join}\
         - Then stop and let the human in this terminal give you a goal — do NOT \
         call `wait` yet. Stay interactive so they can type.\n\
         - When you have a goal, break it into tasks and delegate with `send` (to one \
         agent) or `post` (to a channel).\n\
         - After delegating, call `wait` to collect replies, integrate them, and \
         report progress back to the human here. Return control to the human \
         whenever you need their input.\n"
    } else {
        "Protocol — follow exactly:\n\
         - Call `register` with name=\"{name}\" and role=\"{role}\" first.\n\
         {join}\
         - Call `wait` to receive work; it blocks until a message arrives.\n\
         - Do the requested work in this session, then report back with `send` to the \
         message's sender (or `post` to the relevant channel).\n\
         - ALWAYS end your turn by calling `wait` again so you stay reachable. \
         Never stop the wait-loop.\n"
    };
    let protocol = protocol.replace("{name}", name).replace("{role}", role).replace("{join}", &join);
    format!(
        "You are \"{name}\", a {role} connected to the Relay mesh via the `relay` MCP tools.\n\
         {protocol}{brief}{task}"
    )
}

/// Build the command to launch `agent`, wiring in the relay MCP server.
pub fn build(spec: &Spec) -> Result<Launch> {
    if let Some(tmpl) = spec.custom {
        return Ok(from_template(tmpl, spec));
    }
    match spec.agent {
        "claude" => Ok(claude(spec)),
        "codex" => Ok(codex(spec)),
        "ollama" => ollama(spec),
        "gemini" => Ok(from_template(gemini_template(), spec)),
        other => Err(anyhow!(
            "unknown agent '{other}'. Use --agent claude|codex|ollama|gemini, or pass --cmd with a template."
        )),
    }
}

fn claude(spec: &Spec) -> Launch {
    let mut args: Vec<String> = Vec::new();
    if spec.headless {
        args.extend([
            "-p".into(),
            spec.prompt.into(),
            "--output-format".into(),
            "stream-json".into(),
            "--verbose".into(),
        ]);
        if spec.skip_perms {
            args.push("--dangerously-skip-permissions".into());
        }
    } else {
        args.push(spec.prompt.into());
    }
    args.extend([
        "--mcp-config".into(),
        spec.mcp_file.into(),
        "--strict-mcp-config".into(),
    ]);
    if let Some(m) = spec.model {
        args.extend(["--model".into(), m.into()]);
    }
    Launch {
        program: "claude".into(),
        args,
    }
}

/// Codex speaks streamable-HTTP MCP, wired via `-c mcp_servers.relay.url`.
fn codex(spec: &Spec) -> Launch {
    let mcp = format!("mcp_servers.relay.url=\"{}\"", spec.url);
    let mut args: Vec<String> = Vec::new();
    if spec.headless {
        args.push("exec".into());
        args.push(spec.prompt.into());
        args.push("-c".into());
        args.push("approval_policy=\"never\"".into());
    } else {
        args.push(spec.prompt.into());
    }
    args.push("-c".into());
    args.push(mcp);
    if let Some(m) = spec.model {
        args.push("-c".into());
        args.push(format!("model=\"{m}\""));
    }
    Launch {
        program: "codex".into(),
        args,
    }
}

/// Ollama is not an agent CLI; relay runs its own bridge loop as `relay agent
/// ollama ...`, which drives the model and speaks to the bus over the control
/// plane.
fn ollama(spec: &Spec) -> Result<Launch> {
    let exe = std::env::current_exe()?.to_string_lossy().into_owned();
    let mut args = vec![
        "--home".into(),
        super::paths::abs_dir().to_string_lossy().into_owned(),
        "agent".into(),
        "ollama".into(),
        "--name".into(),
        spec.name.into(),
        "--role".into(),
        spec.role.into(),
        "--url".into(),
        spec.url.into(),
        "--system".into(),
        spec.prompt.into(),
    ];
    if let Some(m) = spec.model {
        args.push("--model".into());
        args.push(m.into());
    }
    for ch in spec.channels {
        args.push("--channel".into());
        args.push(ch.clone());
    }
    Ok(Launch { program: exe, args })
}

fn gemini_template() -> &'static str {
    "gemini --mcp-config {mcp} --prompt {prompt}"
}

/// Split a template into argv, substituting placeholders per token so that
/// {prompt} stays a single argument even though it contains spaces.
fn from_template(tmpl: &str, spec: &Spec) -> Launch {
    let mut tokens = tmpl.split_whitespace().map(|t| subst(t, spec));
    let program = tokens.next().unwrap_or_default();
    Launch {
        program,
        args: tokens.collect(),
    }
}

fn subst(token: &str, spec: &Spec) -> String {
    token
        .replace("{prompt}", spec.prompt)
        .replace("{mcp}", spec.mcp_file)
        .replace("{url}", spec.url)
        .replace("{name}", spec.name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_harness_parks_on_wait() {
        let p = harness_prompt("backend", "backend", "", &[], None, false);
        assert!(p.contains("Call `wait` to receive work"));
        assert!(p.contains("Never stop the wait-loop"));
        assert!(!p.contains("human in this terminal"));
    }

    #[test]
    fn driver_harness_stays_interactive() {
        let p = harness_prompt("lead", "supervisor", "", &[], None, true);
        assert!(p.contains("do NOT call `wait` yet"));
        assert!(p.contains("human in this terminal"));
        assert!(!p.contains("Never stop the wait-loop"));
        assert!(p.contains("register"));
    }

    #[test]
    fn channels_join_line_threads_into_protocol() {
        let chans = vec!["frontend".to_string(), "ui".to_string()];
        let p = harness_prompt("fe", "frontend", "", &chans, None, false);
        assert!(p.contains("`join` these channels: frontend, ui"));
    }
}
