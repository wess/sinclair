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
    /// Load *only* the relay MCP config, ignoring the project `.mcp.json` and the
    /// user's global servers (`claude --strict-mcp-config`). Off by default so an
    /// agent keeps its project/user MCP servers *and* gains relay; opt in for a
    /// hermetic worker. See issue #3.
    pub strict_mcp: bool,
    /// Tool-access allow-list, passed as `claude --allowedTools <rule...>` (one
    /// argv token per rule). Empty = no pre-grant. See issue #8.
    pub allowed_tools: &'a [String],
    /// Extra CLI flags appended verbatim to the agent's own argv (e.g.
    /// `--dangerously-skip-permissions`). Configured per provider by the host.
    pub extra_args: &'a [String],
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
///
/// `optimize` trades the verbose, spelled-out protocol for a terse one-line
/// variant (and a shorter framing) so a launch that carries no task still costs
/// noticeably fewer tokens. It changes wording only — every instruction the
/// agent needs to drive the mesh survives.
pub fn harness_prompt(
    name: &str,
    role: &str,
    brief: &str,
    channels: &[String],
    task: Option<&str>,
    interactive: bool,
    optimize: bool,
) -> String {
    let join = if channels.is_empty() {
        String::new()
    } else if optimize {
        format!(" `join` {};", channels.join(", "))
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
    let protocol = match (optimize, interactive) {
        (true, true) => "Protocol: `register` name=\"{name}\" role=\"{role}\".{join} Stay \
             interactive for the human's goal, split it into tasks, delegate with `send`/`post`, \
             then `wait` for replies and report back.\n",
        (true, false) => "Protocol: `register` name=\"{name}\" role=\"{role}\".{join} Then loop: \
             `wait` for work, do it, report with `send`/`post`, `wait` again. Never stop the loop.\n",
        (false, true) => "Protocol — follow exactly:\n\
             - Call `register` with name=\"{name}\" and role=\"{role}\" first.\n\
             {join}\
             - Then stop and let the human in this terminal give you a goal — do NOT \
             call `wait` yet. Stay interactive so they can type.\n\
             - When you have a goal, break it into tasks and delegate with `send` (to one \
             agent) or `post` (to a channel).\n\
             - After delegating, call `wait` to collect replies, integrate them, and \
             report progress back to the human here. Return control to the human \
             whenever you need their input.\n",
        (false, false) => "Protocol — follow exactly:\n\
             - Call `register` with name=\"{name}\" and role=\"{role}\" first.\n\
             {join}\
             - Call `wait` to receive work; it blocks until a message arrives.\n\
             - Do the requested work in this session, then report back with `send` to the \
             message's sender (or `post` to the relevant channel).\n\
             - ALWAYS end your turn by calling `wait` again so you stay reachable. \
             Never stop the wait-loop.\n",
    };
    let protocol = protocol.replace("{name}", name).replace("{role}", role).replace("{join}", &join);
    let intro = if optimize {
        format!("You are \"{name}\" ({role}) on the Relay mesh (`relay` MCP tools).\n")
    } else {
        format!(
            "You are \"{name}\", a {role} connected to the Relay mesh via the `relay` MCP tools.\n"
        )
    };
    format!("{intro}{protocol}{brief}{task}")
}

/// Build the command to launch `agent`, wiring in the relay MCP server.
pub fn build(spec: &Spec) -> Result<Launch> {
    if let Some(tmpl) = spec.custom {
        return Ok(with_extra(from_template(tmpl, spec), spec));
    }
    let launch = match spec.agent {
        "claude" => claude(spec),
        "codex" => codex(spec),
        // Ollama is driven by relay's own bridge, not an agent CLI, so host
        // flags meant for `claude`/`codex` don't apply — return as-is.
        "ollama" => return ollama(spec),
        "gemini" => from_template(gemini_template(), spec),
        other => {
            return Err(anyhow!(
                "unknown agent '{other}'. Use --agent claude|codex|ollama|gemini, or pass --cmd with a template."
            ))
        }
    };
    Ok(with_extra(launch, spec))
}

/// Append the host-configured extra flags to an agent's argv.
fn with_extra(mut launch: Launch, spec: &Spec) -> Launch {
    launch.args.extend(spec.extra_args.iter().cloned());
    launch
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
    args.extend(["--mcp-config".into(), spec.mcp_file.into()]);
    // `--mcp-config` is additive: without `--strict-mcp-config`, claude still
    // loads the project `.mcp.json` and the user's global servers alongside
    // relay. Only force strict when the host explicitly asks for a hermetic
    // worker, so a manager/worker keeps its project MCP servers (issue #3).
    if spec.strict_mcp {
        args.push("--strict-mcp-config".into());
    }
    // Pre-granted tools (issue #8). `--allowedTools` is variadic; one argv token
    // per rule keeps rules with spaces like `Bash(git commit:*)` intact, and the
    // list ends at the next `--flag`.
    if !spec.allowed_tools.is_empty() {
        args.push("--allowedTools".into());
        args.extend(spec.allowed_tools.iter().cloned());
    }
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

    fn spec<'a>(agent: &'a str, extra: &'a [String]) -> Spec<'a> {
        Spec {
            agent,
            custom: None,
            name: "a",
            role: "worker",
            prompt: "hi",
            mcp_file: "/tmp/mcp.json",
            url: "http://127.0.0.1:7777",
            headless: false,
            model: None,
            channels: &[],
            skip_perms: false,
            strict_mcp: false,
            allowed_tools: &[],
            extra_args: extra,
        }
    }

    #[test]
    fn extra_flags_append_to_claude_argv() {
        let extra = vec!["--dangerously-skip-permissions".to_string()];
        let launch = build(&spec("claude", &extra)).unwrap();
        assert_eq!(launch.program, "claude");
        assert_eq!(launch.args.last().unwrap(), "--dangerously-skip-permissions");
    }

    #[test]
    fn claude_is_additive_mcp_by_default() {
        // Issue #3: without opt-in, claude keeps the project/user MCP servers.
        let launch = build(&spec("claude", &[])).unwrap();
        assert!(launch.args.iter().any(|a| a == "--mcp-config"));
        assert!(!launch.args.iter().any(|a| a == "--strict-mcp-config"));
    }

    #[test]
    fn claude_strict_mcp_opt_in() {
        let mut s = spec("claude", &[]);
        s.strict_mcp = true;
        let launch = build(&s).unwrap();
        assert!(launch.args.iter().any(|a| a == "--strict-mcp-config"));
    }

    #[test]
    fn claude_allowed_tools_are_passed_verbatim() {
        // Issue #8: each rule is its own argv token so `Bash(git commit:*)` stays
        // intact, and the variadic list is followed by --model.
        let tools = vec!["Read".to_string(), "Bash(git commit:*)".to_string()];
        let mut s = spec("claude", &[]);
        s.allowed_tools = &tools;
        s.model = Some("claude-sonnet-4-6");
        let launch = build(&s).unwrap();
        let at = launch.args.iter().position(|a| a == "--allowedTools").unwrap();
        assert_eq!(launch.args[at + 1], "Read");
        assert_eq!(launch.args[at + 2], "Bash(git commit:*)");
        assert_eq!(launch.args[at + 3], "--model");
    }

    #[test]
    fn no_allowed_tools_flag_when_empty() {
        let launch = build(&spec("claude", &[])).unwrap();
        assert!(!launch.args.iter().any(|a| a == "--allowedTools"));
    }

    #[test]
    fn extra_flags_append_to_gemini_template() {
        let extra = vec!["--yolo".to_string()];
        let launch = build(&spec("gemini", &extra)).unwrap();
        assert!(launch.args.contains(&"--yolo".to_string()));
    }

    #[test]
    fn ollama_ignores_agent_flags() {
        // Ollama is a bridge, not an agent CLI — host flags must not leak in.
        let extra = vec!["--dangerously-skip-permissions".to_string()];
        let launch = build(&spec("ollama", &extra)).unwrap();
        assert!(!launch.args.iter().any(|a| a == "--dangerously-skip-permissions"));
    }

    #[test]
    fn worker_harness_parks_on_wait() {
        let p = harness_prompt("backend", "backend", "", &[], None, false, false);
        assert!(p.contains("Call `wait` to receive work"));
        assert!(p.contains("Never stop the wait-loop"));
        assert!(!p.contains("human in this terminal"));
    }

    #[test]
    fn driver_harness_stays_interactive() {
        let p = harness_prompt("lead", "supervisor", "", &[], None, true, false);
        assert!(p.contains("do NOT call `wait` yet"));
        assert!(p.contains("human in this terminal"));
        assert!(!p.contains("Never stop the wait-loop"));
        assert!(p.contains("register"));
    }

    #[test]
    fn channels_join_line_threads_into_protocol() {
        let chans = vec!["frontend".to_string(), "ui".to_string()];
        let p = harness_prompt("fe", "frontend", "", &chans, None, false, false);
        assert!(p.contains("`join` these channels: frontend, ui"));
    }

    #[test]
    fn optimized_harness_is_shorter_but_keeps_the_essentials() {
        let full = harness_prompt("w", "worker", "", &[], None, false, false);
        let lean = harness_prompt("w", "worker", "", &[], None, false, true);
        assert!(lean.len() < full.len());
        // Still names the agent, registers, and keeps the wait-loop alive.
        assert!(lean.contains("register"));
        assert!(lean.contains("`wait`"));
        assert!(lean.contains("Never stop the loop"));
    }

    #[test]
    fn optimized_channels_still_join() {
        let chans = vec!["frontend".to_string(), "ui".to_string()];
        let p = harness_prompt("fe", "frontend", "", &chans, None, false, true);
        assert!(p.contains("`join` frontend, ui"));
    }
}
