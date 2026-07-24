//! AI section: MCP, the Relay agent mesh, and the agent tool roster.

use super::{list, opt, text, toggle, ListKind, Section, Setting};

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
