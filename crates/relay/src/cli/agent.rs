use anyhow::{anyhow, Result};

/// Environment variable a codex worker reads its bearer token from. Codex takes
/// the token by env var name (`bearer_token_env_var`), never inline, so the
/// value has to reach the child's environment.
pub const TOKEN_ENV: &str = "RELAY_TOKEN";

/// Extra environment a worker running `program` needs to reach the bus.
///
/// Only codex needs anything: it takes the bearer token by env-var *name*, so
/// the value must be in the child's environment rather than a config file. The
/// token is regenerated every daemon run and therefore cannot be persisted with
/// a worker — a rehydrated worker rebuilds its environment through here with the
/// current token. Matched on the file stem so an explicit `--bin` path still
/// resolves.
pub fn env_for(program: &str, token: &str) -> Vec<(String, String)> {
    let stem = std::path::Path::new(program)
        .file_stem()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if stem == "codex" {
        vec![(TOKEN_ENV.to_string(), token.to_string())]
    } else {
        Vec::new()
    }
}

/// A resolved command to run an agent.
pub struct Launch {
    pub program: String,
    pub args: Vec<String>,
    /// Extra environment for the child process. Empty for agents that carry
    /// their credentials in the MCP config file itself.
    pub env: Vec<(String, String)>,
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
    /// Run the agent with its own permission/approval prompts bypassed: claude
    /// `--dangerously-skip-permissions`, codex `approval_policy="never"`,
    /// gemini `--yolo`. Needed whenever nobody is watching to answer one — a
    /// headless worker (no terminal at all) or an unattended pane such as a
    /// team split. A `--cmd` template and the ollama bridge get nothing: relay
    /// doesn't know the template's flag, and the bridge has no such prompt.
    pub skip_perms: bool,
    /// Load *only* the relay MCP config, ignoring the project `.mcp.json` and the
    /// user's global servers (`claude --strict-mcp-config`). Off by default so an
    /// agent keeps its project/user MCP servers *and* gains relay; opt in for a
    /// hermetic worker. See issue #3.
    pub strict_mcp: bool,
    /// Bearer token for the bus. Most agents get it from the generated MCP
    /// config; codex only accepts it via an environment variable.
    pub token: &'a str,
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
             then `wait` for replies and report back. An empty or failed `wait` means replies \
             are still coming — call it again rather than giving up.\n",
        (true, false) => "Protocol: `register` name=\"{name}\" role=\"{role}\".{join} Then loop: \
             `wait` for work, do it, report with `send`/`post`, `wait` again. An empty or failed \
             `wait` is normal — call it again. Never stop the loop.\n",
        (false, true) => "Protocol — follow exactly:\n\
             - Call `register` with name=\"{name}\" and role=\"{role}\" first.\n\
             {join}\
             - Then stop and let the human in this terminal give you a goal — do NOT \
             call `wait` yet. Stay interactive so they can type.\n\
             - When you have a goal, break it into tasks and delegate with `send` (to one \
             agent) or `post` (to a channel).\n\
             - After delegating, call `wait` to collect replies, integrate them, and \
             report progress back to the human here. Return control to the human \
             whenever you need their input.\n\
             - `wait` returning no messages, or failing with an error, does NOT mean the \
             work is finished — it is a normal timeout. Call `wait` again until every \
             task you delegated has reported back.\n",
        (false, false) => "Protocol — follow exactly:\n\
             - Call `register` with name=\"{name}\" and role=\"{role}\" first.\n\
             {join}\
             - Call `wait` to receive work; it blocks until a message arrives.\n\
             - Do the requested work in this session, then report back with `send` to the \
             message's sender (or `post` to the relevant channel).\n\
             - `wait` returning no messages, or failing with an error, is normal and \
             expected — it just means nothing arrived in time. Call `wait` again \
             immediately. Never treat it as a reason to stop or to report a problem.\n\
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
        "gemini" => gemini(spec),
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
    } else {
        args.push(spec.prompt.into());
    }
    // Applies to both shapes: a headless worker has no terminal to prompt in,
    // and an unattended pane has no human to answer.
    if spec.skip_perms {
        args.push("--dangerously-skip-permissions".into());
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
        env: Vec::new(),
    }
}

/// Codex speaks streamable-HTTP MCP, wired via `-c mcp_servers.relay.url`.
///
/// Unlike claude and gemini, codex reads no MCP config file, so the bearer token
/// cannot ride along in one: it accepts only the *name* of an environment
/// variable to read (`bearer_token_env_var`), and the value is passed to the
/// child through [`Launch::env`]. Without this every codex request 401s.
fn codex(spec: &Spec) -> Launch {
    let mcp = format!("mcp_servers.relay.url=\"{}\"", spec.url);
    let auth = format!("mcp_servers.relay.bearer_token_env_var=\"{TOKEN_ENV}\"");
    let mut args: Vec<String> = Vec::new();
    if spec.headless {
        args.push("exec".into());
    }
    args.push(spec.prompt.into());
    // `exec` can never answer an approval, and neither can an unattended pane.
    if spec.headless || spec.skip_perms {
        args.push("-c".into());
        args.push("approval_policy=\"never\"".into());
    }
    args.push("-c".into());
    args.push(mcp);
    args.push("-c".into());
    args.push(auth);
    if let Some(m) = spec.model {
        args.push("-c".into());
        args.push(format!("model=\"{m}\""));
    }
    Launch {
        program: "codex".into(),
        args,
        env: env_for("codex", spec.token),
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
    Ok(Launch {
        program: exe,
        args,
        env: Vec::new(),
    })
}

/// Gemini needs no special wiring beyond the template; `--yolo` is its
/// equivalent of claude's permission bypass.
fn gemini(spec: &Spec) -> Launch {
    let mut launch = from_template(gemini_template(), spec);
    if spec.skip_perms {
        launch.args.push("--yolo".into());
    }
    launch
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
        env: Vec::new(),
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
            token: "tok",
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

    /// An unattended interactive pane (a team split) needs the bypass just as
    /// much as a headless worker: nobody is there to answer the prompt. Before
    /// this, `skip_perms` only reached the `-p` branch.
    #[test]
    fn skip_perms_reaches_an_interactive_claude() {
        let mut s = spec("claude", &[]);
        s.skip_perms = true;
        let launch = build(&s).unwrap();
        assert!(!launch.args.iter().any(|a| a == "-p"), "should stay interactive");
        assert!(launch.args.iter().any(|a| a == "--dangerously-skip-permissions"));
    }

    #[test]
    fn skip_perms_sets_codex_approval_policy() {
        let mut s = spec("codex", &[]);
        s.skip_perms = true;
        let launch = build(&s).unwrap();
        assert!(!launch.args.iter().any(|a| a == "exec"), "should stay interactive");
        assert!(launch.args.iter().any(|a| a == "approval_policy=\"never\""));
    }

    #[test]
    fn skip_perms_yolos_gemini() {
        let mut s = spec("gemini", &[]);
        s.skip_perms = true;
        assert!(build(&s).unwrap().args.iter().any(|a| a == "--yolo"));
        // And stays off when it wasn't asked for.
        assert!(!build(&spec("gemini", &[])).unwrap().args.iter().any(|a| a == "--yolo"));
    }

    /// Relay knows the bypass flag for the agents it builds argv for, and only
    /// those. A `--cmd` template and the ollama bridge must come back untouched
    /// rather than get a claude flag they'd choke on.
    #[test]
    fn skip_perms_never_guesses_a_flag() {
        let mut s = spec("ollama", &[]);
        s.skip_perms = true;
        let launch = build(&s).unwrap();
        assert!(!launch.args.iter().any(|a| a.contains("skip-permissions") || a == "--yolo"));

        let mut s = spec("claude", &[]);
        s.custom = Some("myagent --prompt {prompt}");
        s.skip_perms = true;
        let launch = build(&s).unwrap();
        assert_eq!(launch.program, "myagent");
        assert!(!launch.args.iter().any(|a| a.contains("skip-permissions")));
    }

    /// The default stays gated: an attended pane keeps its prompts.
    #[test]
    fn no_bypass_without_skip_perms() {
        for agent in ["claude", "codex", "gemini"] {
            let launch = build(&spec(agent, &[])).unwrap();
            assert!(
                !launch.args.iter().any(|a| {
                    a == "--dangerously-skip-permissions"
                        || a == "--yolo"
                        || a == "approval_policy=\"never\""
                }),
                "{agent} bypassed permissions unasked: {:?}",
                launch.args
            );
        }
    }

    /// A headless worker gets the bypass regardless — `claude -p` and `codex
    /// exec` have no terminal to prompt in.
    #[test]
    fn headless_codex_still_never_approves() {
        let mut s = spec("codex", &[]);
        s.headless = true;
        let launch = build(&s).unwrap();
        assert_eq!(launch.args[0], "exec");
        assert!(launch.args.iter().any(|a| a == "approval_policy=\"never\""));
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

    /// Codex reads no MCP config file, so the bearer token has to arrive by env
    /// var — it only accepts the variable's *name* in config. Without both
    /// halves every codex worker 401s on the bus.
    #[test]
    fn codex_carries_the_bearer_token() {
        let mut s = spec("codex", &[]);
        s.token = "sekrit";
        let launch = build(&s).unwrap();
        let cfg = launch.args.join(" ");
        assert!(
            cfg.contains(&format!("mcp_servers.relay.bearer_token_env_var=\"{TOKEN_ENV}\"")),
            "codex must be told which env var holds the token, got: {cfg}"
        );
        assert_eq!(
            launch.env,
            vec![(TOKEN_ENV.to_string(), "sekrit".to_string())],
            "and the value must reach the child's environment"
        );
    }

    /// Claude gets its credentials from the generated MCP config file, so it
    /// needs no extra environment — the token should not leak into the process
    /// environment (and any subprocess it spawns) for no reason.
    #[test]
    fn claude_needs_no_token_env() {
        let mut s = spec("claude", &[]);
        s.token = "sekrit";
        assert!(build(&s).unwrap().env.is_empty());
    }

    /// A rehydrated worker rebuilds its environment from the program name, since
    /// the token is regenerated every daemon run and cannot be persisted. An
    /// explicit `--bin` path must still resolve.
    #[test]
    fn env_for_matches_codex_by_stem() {
        assert_eq!(
            env_for("/opt/homebrew/bin/codex", "t"),
            vec![(TOKEN_ENV.to_string(), "t".to_string())]
        );
        assert_eq!(env_for("codex", "t").len(), 1);
        assert!(env_for("/usr/local/bin/claude", "t").is_empty());
        assert!(env_for("", "t").is_empty());
    }

    /// Every prompt shape must tell the agent that an empty or failed `wait` is
    /// routine and to call it again. Without this the agent treats a timed-out
    /// park as a failure, writes an explanation, and — under `claude -p` — the
    /// process exits.
    #[test]
    fn every_shape_survives_an_empty_or_failed_wait() {
        for (interactive, optimize) in [(false, false), (false, true), (true, false), (true, true)] {
            let p = harness_prompt("a", "worker", "", &[], None, interactive, optimize);
            assert!(
                p.contains("empty or failed `wait`") || p.contains("failing with an error"),
                "interactive={interactive} optimize={optimize} never mentions a failed wait:\n{p}"
            );
        }
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
