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
    pub prompt: &'a str,
    pub mcp_file: &'a str,
    pub url: &'a str,
    pub headless: bool,
    pub model: Option<&'a str>,
    pub skip_perms: bool,
}

/// The wait-loop harness every agent receives as its opening instruction.
pub fn harness_prompt(name: &str, role: &str, channels: &[String], task: Option<&str>) -> String {
    let join = if channels.is_empty() {
        String::new()
    } else {
        format!("- After registering, `join` these channels: {}.\n", channels.join(", "))
    };
    let task = task
        .filter(|t| !t.trim().is_empty())
        .map(|t| format!("\nYour standing focus: {}\n", t.trim()))
        .unwrap_or_default();
    format!(
        "You are \"{name}\", a {role} connected to the Relay mesh via the `relay` MCP tools.\n\
         Protocol — follow exactly:\n\
         - Call `register` with name=\"{name}\" and role=\"{role}\" first.\n\
         {join}\
         - Call `wait` to receive work; it blocks until a message arrives.\n\
         - Do the requested work in this session, then report back with `send` to the \
         message's sender (or `post` to the relevant channel).\n\
         - ALWAYS end your turn by calling `wait` again so you stay reachable. \
         Never stop the wait-loop.\n\
         {task}"
    )
}

/// Build the command to launch `agent`, wiring in the relay MCP server.
pub fn build(spec: &Spec) -> Result<Launch> {
    if let Some(tmpl) = spec.custom {
        return Ok(from_template(tmpl, spec));
    }
    match spec.agent {
        "claude" => Ok(claude(spec)),
        "codex" => Ok(from_template(codex_template(spec.headless), spec)),
        "gemini" => Ok(from_template(gemini_template(), spec)),
        other => Err(anyhow!(
            "unknown agent '{other}'. Use --agent claude|codex|gemini, or pass --cmd with a template."
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

// NOTE: codex/gemini MCP wiring is best-effort and unverified — adjust the
// templates here or pass --cmd if your CLI expects something different.
fn codex_template(headless: bool) -> &'static str {
    if headless {
        "codex exec {prompt} -c mcp_servers.relay.url=\"{url}\""
    } else {
        "codex {prompt} -c mcp_servers.relay.url=\"{url}\""
    }
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
