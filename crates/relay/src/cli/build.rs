//! The one pipeline that turns "launch agent X as role Y" into a command:
//! resolve the role, merge channels and tool grants, render the harness
//! prompt, and build the agent argv. Shared by `relay launch` (both foreground
//! and `--background`) and the MCP `spawn` tool so the two planes cannot
//! drift; each plane states its own defaults at the call site.

use super::{agent, paths, role};
use anyhow::Result;

pub struct Options<'a> {
    pub name: &'a str,
    pub role: &'a str,
    /// Project root the role resolves against (the worker's cwd): its
    /// `.relay/roles` is the project layer. `None` falls back to the ambient
    /// working directory — only correct for CLI calls, never the daemon.
    pub role_root: Option<&'a std::path::Path>,
    /// Agent CLI override; falls back to the role's `agent`, then `claude`.
    pub agent: Option<&'a str>,
    /// Per-launch focus injected into the harness.
    pub task: Option<&'a str>,
    /// Channels to join, merged with the role's.
    pub channels: &'a [String],
    /// Extra pre-granted tool rules, merged with the role's `tools`.
    pub tools: &'a [String],
    /// Model override; falls back to the role's `model`.
    pub model: Option<&'a str>,
    /// Launch as the human-driven lead; a `driver` role implies it. Only
    /// meaningful for a foreground launch.
    pub lead: bool,
    /// Compact the harness prompt to spend fewer tokens.
    pub optimize: bool,
    /// A server-monitored background worker rather than an interactive pane.
    pub headless: bool,
    /// Bypass the agent's permission prompts (see [`agent::Spec::skip_perms`]).
    /// Required for a headless worker; opt-in for an unattended pane.
    pub skip_perms: bool,
    pub strict_mcp: bool,
    /// Extra flags appended verbatim to the agent's own argv.
    pub extra_args: &'a [String],
    /// Explicit agent binary path, overriding the PATH lookup. Ignored for
    /// `custom` templates and the ollama bridge.
    pub bin: Option<&'a str>,
    /// Custom launch template ({prompt} {mcp} {url} {name}).
    pub custom: Option<&'a str>,
}

/// A fully resolved worker command.
pub struct Built {
    pub program: String,
    pub args: Vec<String>,
    /// Extra environment the child needs (codex reads its bearer token from a
    /// named env var rather than a config file).
    pub env: Vec<(String, String)>,
    /// The agent CLI the options resolved to (an explicit choice, the role's,
    /// or `claude`).
    pub agent: String,
    /// Fixed claude session id for a resumable headless worker: passed as
    /// `--session-id` on first launch, `--resume` on respawn, so context
    /// survives a crash or daemon restart. `None` for foreground or non-claude.
    pub session_id: Option<String>,
}

/// Resolve `o` into a launchable command (see the module docs).
pub fn worker(endpoint: &str, token: &str, o: &Options) -> Result<Built> {
    let role = role::resolve_in(o.role_root, o.role);
    let brief = role.as_ref().map(|r| r.description.clone()).unwrap_or_default();

    let mut channels = role.as_ref().map(|r| r.channels.clone()).unwrap_or_default();
    for c in o.channels {
        if !channels.contains(c) {
            channels.push(c.clone());
        }
    }
    let mut allowed = role.as_ref().map(|r| r.tools.clone()).unwrap_or_default();
    for t in o.tools {
        if !allowed.contains(t) {
            allowed.push(t.clone());
        }
    }
    let agent_name = o
        .agent
        .map(str::to_string)
        .or_else(|| role.as_ref().and_then(|r| r.agent.clone()))
        .unwrap_or_else(|| "claude".to_string());
    let model = o
        .model
        .map(str::to_string)
        .or_else(|| role.as_ref().and_then(|r| r.model.clone()));
    let interactive = !o.headless && (o.lead || role.as_ref().is_some_and(|r| r.driver));

    let mcp = paths::write_mcp_config(endpoint, o.name, token)?;
    let prompt = agent::harness_prompt(
        o.name,
        o.role,
        &brief,
        &channels,
        o.task,
        interactive,
        o.optimize,
    );
    let mut launch = agent::build(&agent::Spec {
        agent: &agent_name,
        custom: o.custom,
        name: o.name,
        role: o.role,
        prompt: &prompt,
        mcp_file: &mcp,
        url: endpoint,
        headless: o.headless,
        model: model.as_deref(),
        channels: &channels,
        skip_perms: o.skip_perms,
        strict_mcp: o.strict_mcp,
        token,
        allowed_tools: &allowed,
        extra_args: o.extra_args,
    })?;
    if let Some(bin) = o.bin.map(str::trim).filter(|b| !b.is_empty()) {
        if o.custom.is_none() && agent_name != "ollama" {
            launch.program = bin.to_string();
        }
    }
    // A resumable headless claude worker gets a fixed session id so it can
    // resume its context after a crash or daemon restart (issue #4).
    let session_id =
        (o.headless && agent_name == "claude").then(|| uuid::Uuid::new_v4().to_string());
    Ok(Built {
        program: launch.program,
        args: launch.args,
        env: launch.env,
        agent: agent_name,
        session_id,
    })
}
