use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::OnceLock;

/// State directory for this instance. Defaults to `./.relay` (per-project,
/// standalone use); Prompt sets an absolute path via `--home` so every relay
/// call shares one mesh regardless of the calling pane's working directory.
static HOME: OnceLock<PathBuf> = OnceLock::new();

pub fn set_home(path: PathBuf) {
    let _ = HOME.set(path);
}

/// On-disk record of a running server, written by `start`/`serve` and read by
/// the other subcommands.
#[derive(Serialize, Deserialize)]
pub struct ServerInfo {
    pub pid: u32,
    pub addr: String,
    pub db: String,
}

pub fn dir() -> PathBuf {
    HOME.get().cloned().unwrap_or_else(|| PathBuf::from(".relay"))
}

/// `dir()` resolved to an absolute path (children run in their own cwd).
pub fn abs_dir() -> PathBuf {
    let d = dir();
    if d.is_absolute() {
        d
    } else {
        std::env::current_dir()
            .map(|c| c.join(&d))
            .unwrap_or(d)
    }
}

pub fn ensure_dir() -> Result<()> {
    std::fs::create_dir_all(dir())?;
    Ok(())
}

pub fn info_path() -> PathBuf {
    dir().join("server.json")
}

pub fn log_path() -> PathBuf {
    dir().join("server.log")
}

pub fn write_info(info: &ServerInfo) -> Result<()> {
    ensure_dir()?;
    std::fs::write(info_path(), serde_json::to_vec_pretty(info)?)?;
    Ok(())
}

pub fn read_info() -> Result<ServerInfo> {
    let bytes = std::fs::read(info_path())
        .map_err(|_| anyhow!("no server here — run `relay start` first"))?;
    Ok(serde_json::from_slice(&bytes)?)
}

pub fn clear_info() {
    let _ = std::fs::remove_file(info_path());
}

/// Write the per-agent MCP config pointing at the bus; returns its absolute path.
pub fn write_mcp_config(endpoint: &str, name: &str) -> Result<String> {
    ensure_dir()?;
    let path = abs_dir().join(format!("{name}.mcp.json"));
    let cfg = serde_json::json!({ "mcpServers": { "relay": { "type": "http", "url": endpoint } } });
    std::fs::write(&path, serde_json::to_vec_pretty(&cfg)?)?;
    Ok(path.to_string_lossy().into_owned())
}

/// Loopback URL workers/agents use to reach the bus.
pub fn endpoint(addr: &str) -> String {
    let host = addr.replace("0.0.0.0", "127.0.0.1");
    format!("http://{host}/mcp")
}

/// True if a process with this pid is alive.
pub fn alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as i32, 0) == 0 }
}
