pub mod agent;
pub mod bridge;
pub mod build;
pub mod feed;
pub mod http;
pub mod launch;
pub mod layered;
pub mod paths;
pub mod ps;
pub mod role;
pub mod server;
pub mod team;
pub mod watch;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "relay", version, about = "Supervisor → team mesh for coding agents")]
pub struct Cli {
    /// State directory shared by all relay calls (default: ./.relay).
    #[arg(long, global = true)]
    home: Option<String>,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Start the relay server as a background daemon.
    Start(ServeArgs),
    /// Stop the running server (and its workers).
    Stop,
    /// Pause the mesh: stop the daemon. The SQLite bus and the persisted worker
    /// registry survive on disk, so `resume` brings the team back.
    Pause,
    /// Resume the mesh: start the daemon, rehydrating persisted workers and
    /// resuming each one's prior session.
    Resume(ServeArgs),
    /// Restart the server.
    Restart(ServeArgs),
    /// Show whether the server is running.
    Status,
    /// Run the server in the foreground (used internally by `start`).
    #[command(hide = true)]
    Serve(ServeArgs),

    /// Launch an agent under relay, wired to the bus, MCP, and wait-loop harness.
    Launch(LaunchArgs),
    /// List registered agents and background workers.
    Ps,
    /// Stream live status as newline-delimited JSON (used by the app sidebar).
    Watch,
    /// Stop a background worker by name.
    Kill { name: String },
    /// Print the message bus (use --follow to stream).
    Feed {
        #[arg(short, long)]
        follow: bool,
    },
    /// Manage reusable agent roles.
    Role {
        #[command(subcommand)]
        action: RoleCmd,
    },
    /// Manage teams (a layout + a roster of agents).
    Team {
        #[command(subcommand)]
        action: TeamCmd,
    },
    /// Run a bridge agent that drives a non-MCP backend (e.g. ollama) on the bus.
    #[command(hide = true)]
    Agent(AgentArgs),
}

#[derive(clap::Args)]
pub struct AgentArgs {
    /// Backend to bridge: currently `ollama`.
    pub backend: String,
    /// Agent name in the mesh.
    #[arg(long)]
    pub name: String,
    /// Role label.
    #[arg(long, default_value = "worker")]
    pub role: String,
    /// Relay MCP URL (control plane shares its host:port).
    #[arg(long)]
    pub url: String,
    /// Model to run.
    #[arg(long, default_value = "llama3.1")]
    pub model: String,
    /// System prompt / role brief.
    #[arg(long, default_value = "")]
    pub system: String,
    /// Channel to join (repeatable).
    #[arg(long = "channel")]
    pub channels: Vec<String>,
    /// Ollama base URL.
    #[arg(long, default_value = "http://127.0.0.1:11434")]
    pub ollama: String,
}

#[derive(Subcommand)]
pub enum TeamCmd {
    /// List teams (project, user, built-in).
    List {
        /// Emit JSON (for Sinclair).
        #[arg(long)]
        json: bool,
    },
    /// Show a resolved team.
    Info {
        name: String,
        #[arg(long)]
        json: bool,
    },
    /// Create a new team in $EDITOR.
    Create {
        name: String,
        #[arg(long)]
        user: bool,
    },
    /// Edit a team in $EDITOR (copies a built-in / lower layer first).
    Edit {
        name: String,
        #[arg(long)]
        user: bool,
    },
    /// Delete a team file.
    Delete {
        name: String,
        #[arg(long)]
        user: bool,
    },
    /// Save a team from a JSON spec on stdin (non-interactive; used by Sinclair's
    /// team builder). JSON: {name, layout, members:[{name, role, agent}]}.
    Save {
        /// Write to the user dir instead of the project (./.relay/teams).
        #[arg(long)]
        user: bool,
    },
}

#[derive(Subcommand)]
pub enum RoleCmd {
    /// List available roles (project, user, built-in).
    List {
        /// Emit JSON (for Sinclair).
        #[arg(long)]
        json: bool,
    },
    /// Show a resolved role.
    Info { name: String },
    /// Create a new role in $EDITOR.
    Create {
        name: String,
        /// Write to the user dir instead of the project (./.relay/roles).
        #[arg(long)]
        user: bool,
    },
    /// Edit a role in $EDITOR (copies a built-in / lower layer if needed).
    Edit {
        name: String,
        #[arg(long)]
        user: bool,
    },
    /// Delete a role file.
    Delete {
        name: String,
        #[arg(long)]
        user: bool,
    },
}

#[derive(clap::Args)]
pub struct ServeArgs {
    /// Bind address, e.g. 127.0.0.1:7777.
    #[arg(long)]
    pub addr: Option<String>,
    /// SQLite path for the shared bus.
    #[arg(long)]
    pub db: Option<String>,
}

#[derive(clap::Args)]
pub struct LaunchArgs {
    /// Unique name for this agent in the mesh (prompted if omitted).
    pub name: Option<String>,
    /// Agent CLI to run: claude | codex | gemini (or use --cmd). Defaults to the
    /// role's agent, else claude.
    #[arg(long)]
    pub agent: Option<String>,
    /// Role label.
    #[arg(long, default_value = "worker")]
    pub role: String,
    /// Standing task / focus for the agent.
    #[arg(long)]
    pub task: Option<String>,
    /// Channel to join (repeatable).
    #[arg(long = "channel")]
    pub channels: Vec<String>,
    /// Model override passed to the agent CLI.
    #[arg(long)]
    pub model: Option<String>,
    /// Working directory for the agent.
    #[arg(long)]
    pub cwd: Option<String>,
    /// Custom launch template; placeholders: {prompt} {mcp} {url} {name}.
    #[arg(long)]
    pub cmd: Option<String>,
    /// Explicit path to the agent binary, overriding the PATH lookup for the
    /// built-in agent. Ignored for --cmd and the ollama bridge.
    #[arg(long)]
    pub bin: Option<String>,
    /// Run as a server-managed background worker instead of in this terminal.
    #[arg(long)]
    pub background: bool,
    /// Launch as the human-driven lead: stay interactive instead of parking on
    /// the `wait`-loop. Implied for any `driver` role (e.g. supervisor).
    #[arg(long)]
    pub lead: bool,
    /// Compact the harness prompt to spend fewer tokens (terse protocol).
    #[arg(long)]
    pub optimize: bool,
    /// Load only the relay MCP server, ignoring the project `.mcp.json` and the
    /// user's global servers (`claude --strict-mcp-config`). Off by default so
    /// the agent keeps its project/user MCP servers alongside relay (issue #3).
    #[arg(long = "strict-mcp")]
    pub strict_mcp: bool,
    /// Pre-grant a tool to this agent (repeatable), passed to `claude
    /// --allowedTools`, e.g. `--allow-tool Read --allow-tool "Bash(git:*)"`.
    /// Merges with the role's `tools`. See issue #8.
    #[arg(long = "allow-tool")]
    pub allow_tools: Vec<String>,
    /// Extra flag appended to the agent's own CLI (repeatable), e.g.
    /// `--agent-arg --dangerously-skip-permissions`.
    #[arg(long = "agent-arg")]
    pub agent_args: Vec<String>,
    /// Run unattended: bypass the agent's permission prompts with whatever flag
    /// it uses (claude `--dangerously-skip-permissions`, codex
    /// `approval_policy=never`, gemini `--yolo`). Resolved after the role picks
    /// the agent, so it works without naming one. Implied by `--background`,
    /// which has no terminal to prompt in. Use it for a pane nobody is
    /// watching, such as a team member's split.
    #[arg(long = "skip-permissions")]
    pub skip_permissions: bool,
}

pub async fn run(cli: Cli) -> Result<()> {
    if let Some(home) = cli.home.as_deref() {
        paths::set_home(home.into());
    }
    match cli.cmd {
        Cmd::Serve(a) => server::serve(a).await,
        Cmd::Start(a) => server::start(a),
        Cmd::Stop => server::stop(),
        Cmd::Pause => server::stop(),
        Cmd::Resume(a) => server::start(a),
        Cmd::Restart(a) => server::restart(a),
        Cmd::Status => server::status(),
        Cmd::Launch(a) => launch::launch(a).await,
        Cmd::Ps => ps::ps(),
        Cmd::Watch => watch::watch(),
        Cmd::Kill { name } => ps::kill(&name),
        Cmd::Feed { follow } => feed::feed(follow),
        Cmd::Role { action } => role::run(action),
        Cmd::Team { action } => team::run(action),
        Cmd::Agent(a) => bridge::run(a),
    }
}
