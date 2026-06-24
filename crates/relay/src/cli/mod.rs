pub mod agent;
pub mod feed;
pub mod http;
pub mod launch;
pub mod paths;
pub mod ps;
pub mod server;

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
    /// Restart the server.
    Restart(ServeArgs),
    /// Show whether the server is running.
    Status,
    /// Run the server in the foreground (used internally by `start`).
    #[command(hide = true)]
    Serve(ServeArgs),

    /// Launch an agent under relay — wired to the bus, MCP, and wait-loop harness.
    Launch(LaunchArgs),
    /// List registered agents and background workers.
    Ps,
    /// Stop a background worker by name.
    Kill { name: String },
    /// Print the message bus (use --follow to stream).
    Feed {
        #[arg(short, long)]
        follow: bool,
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
    /// Agent CLI to run: claude | codex | gemini (or use --cmd).
    #[arg(long, default_value = "claude")]
    pub agent: String,
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
    /// Run as a server-managed background worker instead of in this terminal.
    #[arg(long)]
    pub background: bool,
}

pub async fn run(cli: Cli) -> Result<()> {
    if let Some(home) = cli.home.as_deref() {
        paths::set_home(home.into());
    }
    match cli.cmd {
        Cmd::Serve(a) => server::serve(a).await,
        Cmd::Start(a) => server::start(a),
        Cmd::Stop => server::stop(),
        Cmd::Restart(a) => server::restart(a),
        Cmd::Status => server::status(),
        Cmd::Launch(a) => launch::launch(a).await,
        Cmd::Ps => ps::ps(),
        Cmd::Kill { name } => ps::kill(&name),
        Cmd::Feed { follow } => feed::feed(follow),
    }
}
