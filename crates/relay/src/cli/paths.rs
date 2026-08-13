use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

/// State directory for this instance. Defaults to `./.relay` (per-project,
/// standalone use); Sinclair sets an absolute path via `--home` so every relay
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
    /// The address actually bound — what clients connect to.
    pub addr: String,
    /// The address `--addr` asked for; differs from `addr` when that port was
    /// taken and the daemon walked forward to a free one. Defaults empty for
    /// records written by older daemons.
    #[serde(default)]
    pub requested: String,
    pub db: String,
    /// Bearer token clients present on every request. Stored here (0600) so
    /// only the same user can read it; defaults empty for forward/back compat.
    #[serde(default)]
    pub token: String,
}

pub fn dir() -> PathBuf {
    HOME.get()
        .cloned()
        .unwrap_or_else(|| PathBuf::from(".relay"))
}

/// `dir()` resolved to an absolute path (children run in their own cwd).
pub fn abs_dir() -> PathBuf {
    let d = dir();
    if d.is_absolute() {
        d
    } else {
        std::env::current_dir().map(|c| c.join(&d)).unwrap_or(d)
    }
}

pub fn ensure_dir() -> Result<()> {
    let d = dir();
    std::fs::create_dir_all(&d)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&d, std::fs::Permissions::from_mode(0o700));
    }
    Ok(())
}

/// Restrict a freshly written file in the state dir to owner read/write.
pub fn lock_file(_path: &std::path::Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(_path, std::fs::Permissions::from_mode(0o600));
    }
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
    lock_file(&info_path());
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

/// Per-server tool-call timeout written into the generated MCP config, in
/// milliseconds. `wait` parks for up to `tools::WAIT_MAX_SECS` (four minutes);
/// ten minutes here leaves room for that plus the work a tool does, without
/// relying on whatever the client's default happens to be. A client whose
/// default wall-clock limit is shorter than the park would abort every `wait`.
const TOOL_TIMEOUT_MS: u64 = 600_000;

/// The MCP client config for one agent: where the bus is, how to authenticate,
/// and how long a tool call may take.
fn mcp_config(endpoint: &str, token: &str) -> serde_json::Value {
    serde_json::json!({
        "mcpServers": {
            "relay": {
                "type": "http",
                "url": endpoint,
                "headers": { "Authorization": format!("Bearer {token}") },
                "timeout": TOOL_TIMEOUT_MS,
            }
        }
    })
}

/// Write the per-agent MCP config pointing at the bus; returns its absolute
/// path. The config carries the bearer token so the agent's MCP client can
/// authenticate to the bus, and an explicit tool timeout so a parked `wait` is
/// not cut short by a client-side default.
pub fn write_mcp_config(endpoint: &str, name: &str, token: &str) -> Result<String> {
    ensure_dir()?;
    let path = abs_dir().join(format!("{name}.mcp.json"));
    let cfg = mcp_config(endpoint, token);
    std::fs::write(&path, serde_json::to_vec_pretty(&cfg)?)?;
    lock_file(&path);
    Ok(path.to_string_lossy().into_owned())
}

/// Loopback URL workers/agents use to reach the bus.
pub fn endpoint(addr: &str) -> String {
    let host = addr.replace("0.0.0.0", "127.0.0.1");
    format!("http://{host}/mcp")
}

/// True if a process with this pid is alive.
pub fn alive(pid: u32) -> bool {
    crate::proc::alive(pid)
}

/// Pids of spawned workers, persisted so a fresh daemon can reap children left
/// running after a previous instance was SIGKILLed (and never SIGTERM'd them).
pub fn workers_pids_path() -> PathBuf {
    dir().join("workers.pids")
}

static PIDS_LOCK: Mutex<()> = Mutex::new(());

fn read_pids() -> Vec<u32> {
    std::fs::read_to_string(workers_pids_path())
        .map(|s| s.lines().filter_map(|l| l.trim().parse().ok()).collect())
        .unwrap_or_default()
}

fn write_pids(pids: &[u32]) {
    let body = pids
        .iter()
        .map(|p| p.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    if std::fs::write(workers_pids_path(), body).is_ok() {
        lock_file(&workers_pids_path());
    }
}

/// Record a spawned worker's pid so a future daemon can reap it.
pub fn record_worker_pid(pid: u32) {
    if pid == 0 {
        return;
    }
    let _g = PIDS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut pids = read_pids();
    if !pids.contains(&pid) {
        pids.push(pid);
        write_pids(&pids);
    }
}

/// Drop a worker's pid from the record once it has stopped.
pub fn forget_worker_pid(pid: u32) {
    if pid == 0 {
        return;
    }
    let _g = PIDS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut pids = read_pids();
    if pids.contains(&pid) {
        pids.retain(|&p| p != pid);
        write_pids(&pids);
    }
}

/// On daemon startup, SIGKILL any still-alive workers left by a previous
/// instance, then clear the record. Tolerant of a missing or garbage file.
pub fn reap_stray_workers() {
    let _g = PIDS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let me = std::process::id();
    for pid in read_pids() {
        if pid != 0 && pid != me && alive(pid) {
            crate::proc::kill(pid);
        }
    }
    let _ = std::fs::remove_file(workers_pids_path());
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The generated config must pin a tool timeout. Left unset, the client's
    /// own default governs, and one shorter than a parked `wait` aborts every
    /// park — which the agent sees as a tool error, not an empty inbox.
    #[test]
    fn the_generated_config_pins_a_tool_timeout() {
        let cfg = mcp_config("http://127.0.0.1:7777/mcp", "sekrit");
        let relay = &cfg["mcpServers"]["relay"];
        let timeout = relay["timeout"].as_u64().expect("a timeout must be set");
        assert_eq!(timeout, TOOL_TIMEOUT_MS);
        // Comfortably longer than a full park, or it defeats the purpose.
        let park_ms = crate::tools::WAIT_MAX_SECS * 1000;
        assert!(
            timeout > park_ms,
            "a {timeout}ms timeout cannot outlast a {park_ms}ms park"
        );
        assert_eq!(relay["type"], "http");
        assert_eq!(relay["url"], "http://127.0.0.1:7777/mcp");
        assert_eq!(relay["headers"]["Authorization"], "Bearer sekrit");
    }
}
