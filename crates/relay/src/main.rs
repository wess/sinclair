mod bus;
mod cli;
mod control;
mod db;
mod mcp;
mod proc;
mod protocol;
mod spawn;
mod state;
mod tools;
mod transport;

use anyhow::Result;
use clap::Parser;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = cli::Cli::parse();
    cli::run(cli).await
}
