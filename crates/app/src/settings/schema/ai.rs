//! AI section: MCP, the Relay agent mesh, and the agent tool roster.

use super::{choice, list, opt, strs, text, toggle, ListKind, Section, Setting};

/// Keys the Agent-tools group lays out itself (toggle + Test button + path
/// and flags fields per tool). Kept out of the generic row list so the
/// section doesn't show them twice.
pub const TOOL_KEYS: &[&str] = &[
    "agent-claude",
    "agent-claude-path",
    "agent-claude-args",
    "agent-codex",
    "agent-codex-path",
    "agent-codex-args",
    "agent-ollama",
    "agent-gemini",
    "agent-gemini-path",
    "agent-gemini-args",
];

pub(super) fn settings() -> Vec<Setting> {
    let s = Section::Ai;
    vec![
        toggle(
            "ai-enabled",
            "Enable AI features",
            "Master switch for everything that talks to a model.",
            s,
            |o| o.ai_enabled,
        ),
        toggle(
            "ai-optimize-tokens",
            "Optimize tokens",
            "Compact prompts sent to agents to spend fewer tokens.",
            s,
            |o| o.ai_optimize_tokens,
        ),
        toggle(
            "mcp-server-enabled",
            "MCP server",
            "Expose this terminal to agents over the Model Context Protocol.",
            s,
            |o| o.mcp_server_enabled,
        ),
        toggle(
            "relay-enabled",
            "Relay agent mesh",
            "Run the Relay sidecar so agent sessions can coordinate.",
            s,
            |o| o.relay_enabled,
        ),
        toggle(
            "relay-start-on-launch",
            "Start Relay on launch",
            "Start the Relay server when Sinclair launches.",
            s,
            |o| o.relay_start_on_launch,
        ),
        text(
            "relay-address",
            "Relay address",
            "Bind address for the Relay server. If the port is taken, the next free one is used.",
            s,
            |o| o.relay_address.clone(),
            "127.0.0.1:7777",
        ),
        text(
            "relay-default-agent",
            "Default agent",
            "Agent CLI used when launching without picking one.",
            s,
            |o| o.relay_default_agent.clone(),
            "claude",
        ),
        toggle(
            "relay-team-autonomy",
            "Team members run unattended",
            "Launch team members with permission prompts skipped. A team fills every split at \
             once, so a member that stops to ask would wait until you find its pane.",
            s,
            |o| o.relay_team_autonomy,
        ),
        toggle(
            "relay-team-window",
            "Open teams in their own window",
            "Give a team a window to itself — one member per pane, resizable dividers between \
             them — instead of folding it into the layout you're working in.",
            s,
            |o| o.relay_team_window,
        ),
        toggle(
            "sandbox-enabled",
            "Shared sandbox",
            "Run this project's panes and agents inside one container, so a team shares a \
             filesystem and a toolchain. Needs Docker or Podman.",
            s,
            |o| o.sandbox_enabled,
        ),
        text(
            "sandbox-image",
            "Sandbox image",
            "Ready-made image to use as-is. Blank builds one from the base below and installs \
             the agent CLI into it.",
            s,
            |o| opt(&o.sandbox_image),
            "build one",
        ),
        text(
            "sandbox-base",
            "Sandbox base image",
            "Base the generated image layers on. The generated layer uses apt, so a non-Debian \
             base needs a ready-made image instead.",
            s,
            |o| opt(&o.sandbox_base),
            container::DEFAULT_BASE,
        ),
        choice(
            "sandbox-network",
            "Sandbox network",
            "How the sandbox reaches the network. `none` is the strictest, and cuts agents off \
             from the Relay bus.",
            s,
            |o| o.sandbox_network.clone().unwrap_or_default(),
            || strs(&["bridge", "host", "none"]),
            Some("bridge"),
        ),
        text(
            "sandbox-user",
            "Sandbox user",
            "User the container runs as. `host` uses your own uid, which keeps a Linux bind \
             mount from filling with root-owned files.",
            s,
            |o| opt(&o.sandbox_user),
            "host on Linux, image default on macOS",
        ),
        text(
            "sandbox-memory",
            "Sandbox memory limit",
            "Ceiling for the whole sandbox, e.g. 8g. Blank is unlimited.",
            s,
            |o| opt(&o.sandbox_memory),
            "unlimited",
        ),
        text(
            "sandbox-cpus",
            "Sandbox CPU limit",
            "Ceiling for the whole sandbox, e.g. 4. Blank is unlimited.",
            s,
            |o| opt(&o.sandbox_cpus),
            "unlimited",
        ),
        toggle(
            "sandbox-persist",
            "Keep the sandbox running",
            "Leave the container up when the last pane using it closes. Rebuilding a toolchain \
             every session is slow, and an idle container costs nothing.",
            s,
            |o| o.sandbox_persist,
        ),
        toggle(
            "sandbox-devcontainer",
            "Use devcontainer.json",
            "Read a project's .devcontainer/devcontainer.json when it has one, and enter a \
             container an editor already started for it.",
            s,
            |o| o.sandbox_devcontainer,
        ),
        list(
            ListKind::SandboxAgents,
            "Agent CLIs installed into the generated sandbox image. Empty installs the \
             default agent.",
            s,
        ),
        list(
            ListKind::SandboxPackages,
            "Extra apt packages baked into the generated sandbox image.",
            s,
        ),
        list(
            ListKind::SandboxSetup,
            "Extra commands baked into the generated sandbox image, one layer each.",
            s,
        ),
        list(
            ListKind::SandboxMount,
            "Extra sandbox mounts: source:target[:ro]. A bare path mounts at itself.",
            s,
        ),
        list(
            ListKind::SandboxEnv,
            "Extra environment inside the sandbox: KEY=VALUE.",
            s,
        ),
        toggle(
            "agent-claude",
            "Claude Code",
            "Offer Claude Code in the agent pickers.",
            s,
            |o| o.agent_claude,
        ),
        text(
            "agent-claude-path",
            "Claude path",
            "Explicit path to the claude binary; blank searches PATH.",
            s,
            |o| opt(&o.agent_claude_path),
            "Search PATH",
        ),
        text(
            "agent-claude-args",
            "Claude flags",
            "Extra CLI flags appended when launching Claude Code.",
            s,
            |o| opt(&o.agent_claude_args),
            "--dangerously-skip-permissions",
        ),
        toggle(
            "agent-codex",
            "Codex",
            "Offer Codex in the agent pickers.",
            s,
            |o| o.agent_codex,
        ),
        text(
            "agent-codex-path",
            "Codex path",
            "Explicit path to the codex binary; blank searches PATH.",
            s,
            |o| opt(&o.agent_codex_path),
            "Search PATH",
        ),
        text(
            "agent-codex-args",
            "Codex flags",
            "Extra CLI flags appended when launching Codex.",
            s,
            |o| opt(&o.agent_codex_args),
            "extra CLI flags",
        ),
        toggle(
            "agent-ollama",
            "Ollama",
            "Offer Ollama (bridged over its API) in the agent pickers.",
            s,
            |o| o.agent_ollama,
        ),
        toggle(
            "agent-gemini",
            "Gemini",
            "Offer Gemini in the agent pickers.",
            s,
            |o| o.agent_gemini,
        ),
        text(
            "agent-gemini-path",
            "Gemini path",
            "Explicit path to the gemini binary; blank searches PATH.",
            s,
            |o| opt(&o.agent_gemini_path),
            "Search PATH",
        ),
        text(
            "agent-gemini-args",
            "Gemini flags",
            "Extra CLI flags appended when launching Gemini.",
            s,
            |o| opt(&o.agent_gemini_args),
            "extra CLI flags",
        ),
        list(
            ListKind::AgentTool,
            "User-defined agents: label|command, with {prompt} {mcp} {url} {name}.",
            s,
        ),
    ]
}
