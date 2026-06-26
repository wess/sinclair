//! Manages the bundled `relay` sidecar — the agent mesh. Prompt never runs the
//! mesh in-process; it starts/stops the bundled binary as a detached daemon and
//! launches agents into splits. Every parameter comes from settings, passed
//! explicitly on the command line (no environment variables).

use serde::{Deserialize, Serialize};
use std::net::ToSocketAddrs;
use std::path::PathBuf;

/// A saved agent definition, shown under AI → Agents for relaunch.
#[derive(Clone, Serialize, Deserialize)]
pub struct AgentDef {
    pub name: String,
    pub provider: String,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub task: Option<String>,
}

fn defs_path() -> PathBuf {
    home().join("agents.json")
}

pub fn list_agent_defs() -> Vec<AgentDef> {
    std::fs::read(defs_path())
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

/// Save (upsert by name) an agent definition.
pub fn save_agent_def(def: AgentDef) {
    let mut defs = list_agent_defs();
    defs.retain(|d| d.name != def.name);
    defs.push(def);
    let _ = std::fs::create_dir_all(home());
    let _ = std::fs::write(defs_path(), serde_json::to_vec_pretty(&defs).unwrap_or_default());
}

/// Build the launch command for a previously-saved agent.
pub fn launch_saved_command(name: &str) -> Option<String> {
    let def = list_agent_defs().into_iter().find(|d| d.name == name)?;
    Some(launch_agent_command(
        &def.provider,
        &def.name,
        def.role.as_deref(),
        def.task.as_deref(),
    ))
}

/// The bundled `relay` binary: prefer a sibling of the running executable
/// (inside the app bundle / target dir), else fall back to PATH.
fn binary() -> String {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let cand = dir.join("relay");
            if cand.exists() {
                return cand.to_string_lossy().into_owned();
            }
        }
    }
    "relay".to_string()
}

/// Fixed state directory for the mesh, beside the config file, so every relay
/// call shares one mesh regardless of the calling pane's working directory.
fn home() -> PathBuf {
    config::default_path()
        .and_then(|p| p.parent().map(|d| d.join("relay")))
        .unwrap_or_else(|| PathBuf::from(".relay"))
}

fn home_str() -> String {
    home().to_string_lossy().into_owned()
}

fn db_str() -> String {
    home().join("relay.db").to_string_lossy().into_owned()
}

/// Run a relay subcommand without blocking the UI thread.
fn run_bg(args: Vec<String>) {
    let bin = binary();
    std::thread::spawn(move || {
        let _ = std::process::Command::new(bin).args(&args).output();
    });
}

fn start_args(opts: &config::Options) -> Vec<String> {
    vec![
        "--home".into(),
        home_str(),
        "start".into(),
        "--addr".into(),
        opts.relay_address.clone(),
        "--db".into(),
        db_str(),
    ]
}

/// Whether the Relay menu and commands should be available.
pub fn enabled(opts: &config::Options) -> bool {
    opts.ai_enabled && opts.relay_enabled
}

/// Start the daemon at app launch, only if configured to auto-start.
pub fn on_launch(opts: &config::Options) {
    if enabled(opts) && opts.relay_start_on_launch {
        let _ = std::fs::create_dir_all(home());
        run_bg(start_args(opts));
    }
}

/// Ensure the daemon is up before launching an agent. Starts it synchronously
/// (the `start` command polls for health) if enabled but not running. Returns
/// whether it's running afterward.
pub fn ensure_running(opts: &config::Options) -> bool {
    if !enabled(opts) {
        return false;
    }
    if running() {
        return true;
    }
    let _ = std::fs::create_dir_all(home());
    let _ = std::process::Command::new(binary())
        .args(start_args(opts))
        .output();
    running()
}

/// Start the daemon now (AI → Relay → Start Server).
pub fn start(opts: &config::Options) {
    let _ = std::fs::create_dir_all(home());
    run_bg(start_args(opts));
}

/// Stop the daemon now (AI → Relay → Stop Server).
pub fn stop() {
    run_bg(vec!["--home".into(), home_str(), "stop".into()]);
}

/// Restart the daemon: stop, then start, as one background sequence so the
/// new instance never races the old one for the address.
pub fn restart(opts: &config::Options) {
    let _ = std::fs::create_dir_all(home());
    let bin = binary();
    let stop_args = vec!["--home".to_string(), home_str(), "stop".to_string()];
    let start = start_args(opts);
    std::thread::spawn(move || {
        let _ = std::process::Command::new(&bin).args(&stop_args).output();
        let _ = std::process::Command::new(&bin).args(&start).output();
    });
}

/// Reconcile the daemon with current settings after a config reload.
pub fn on_reload(opts: &config::Options) {
    let _ = std::fs::create_dir_all(home());
    if enabled(opts) {
        run_bg(start_args(opts));
    } else {
        run_bg(vec!["--home".into(), home_str(), "stop".into()]);
    }
}

/// Shell command to stream the bus in a split.
pub fn feed_command() -> String {
    format!("\"{}\" --home \"{}\" feed --follow", binary(), home_str())
}

/// Enabled agent providers, in display order.
pub fn enabled_agents(opts: &config::Options) -> Vec<String> {
    let mut v = Vec::new();
    if opts.agent_claude {
        v.push("claude".to_string());
    }
    if opts.agent_codex {
        v.push("codex".to_string());
    }
    if opts.agent_ollama {
        v.push("ollama".to_string());
    }
    if opts.agent_gemini {
        v.push("gemini".to_string());
    }
    v
}

/// Available role names (built-in + user + project), via the relay CLI.
pub fn role_list() -> Vec<String> {
    let Ok(out) = std::process::Command::new(binary())
        .args(["role", "list", "--json"])
        .output()
    else {
        return Vec::new();
    };
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or(serde_json::Value::Null);
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|r| r["name"].as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

/// Build a `relay launch` command for a specific provider/name/role-or-task.
pub fn launch_agent_command(
    provider: &str,
    name: &str,
    role: Option<&str>,
    task: Option<&str>,
) -> String {
    let mut s = format!(
        "\"{}\" --home \"{}\" launch {name} --agent {provider}",
        binary(),
        home_str()
    );
    if let Some(r) = role.filter(|r| !r.is_empty()) {
        s.push_str(&format!(" --role {r}"));
    }
    if let Some(t) = task.filter(|t| !t.is_empty()) {
        s.push_str(&format!(" --task \"{}\"", t.replace('"', "'")));
    }
    keep_open(s)
}

/// Wrap a launch command so a failure leaves the pane open with the reason
/// (instead of the shell exiting and the pane vanishing). On success the agent
/// `exec`s and replaces the shell, so the fallback never runs.
fn keep_open(cmd: String) -> String {
    format!(
        "{cmd} || {{ echo; echo '[relay] launch failed — check Settings → AI (is the server running?)'; exec \"${{SHELL:-/bin/sh}}\"; }}"
    )
}

/// Names of available teams (built-in + user + project), via the relay CLI.
pub fn team_list() -> Vec<String> {
    let Ok(out) = std::process::Command::new(binary())
        .args(["team", "list", "--json"])
        .output()
    else {
        return Vec::new();
    };
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or(serde_json::Value::Null);
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|t| t["name"].as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

/// A team's layout shape and ordered `(member, role)` roster.
pub fn team_info(name: &str) -> Option<(String, Vec<(String, String)>)> {
    let out = std::process::Command::new(binary())
        .args(["team", "info", name, "--json"])
        .output()
        .ok()?;
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    let layout = v["layout"].as_str()?.to_string();
    let members = v["members"]
        .as_array()?
        .iter()
        .map(|m| {
            (
                m["name"].as_str().unwrap_or("agent").to_string(),
                m["role"].as_str().unwrap_or("worker").to_string(),
            )
        })
        .collect();
    Some((layout, members))
}

/// Shell command that launches one team member in a pane. The team's first
/// member is the human-driven `lead` — it stays interactive instead of parking
/// on the `wait`-loop, so the human can steer it.
pub fn launch_member(member: &str, role: &str, lead: bool) -> String {
    let flag = if lead { " --lead" } else { "" };
    keep_open(format!(
        "\"{}\" --home \"{}\" launch {member} --role {role}{flag}",
        binary(),
        home_str()
    ))
}

/// Path to the relay server's log file.
pub fn log_path() -> PathBuf {
    home().join("server.log")
}

/// Shell command to tail the relay server log in a split.
pub fn log_command() -> String {
    format!("tail -n 200 -F \"{}\"", log_path().display())
}

/// Probe whether a tool is reachable. CLIs are checked with `--version`; Ollama
/// is probed on its API port. Returns a short detail on success or failure.
pub fn test_tool(tool: &str) -> Result<String, String> {
    if tool == "ollama" {
        let addr: std::net::SocketAddr = "127.0.0.1:11434".parse().unwrap();
        return std::net::TcpStream::connect_timeout(&addr, std::time::Duration::from_millis(500))
            .map(|_| "Ollama reachable".to_string())
            .map_err(|_| "not running — start `ollama serve`".to_string());
    }
    match std::process::Command::new(tool).arg("--version").output() {
        Ok(out) if out.status.success() => {
            let v = String::from_utf8_lossy(&out.stdout);
            let line = v.lines().next().unwrap_or("ok").trim();
            Ok(if line.is_empty() { "ok".into() } else { line.to_string() })
        }
        Ok(_) => Err(format!("`{tool} --version` failed")),
        Err(_) => Err(format!("`{tool}` not found on PATH")),
    }
}

/// Whether the relay server is actually listening (reads its record, probes it).
pub fn running() -> bool {
    let Ok(bytes) = std::fs::read(home().join("server.json")) else {
        return false;
    };
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    let Some(addr) = v["addr"].as_str() else {
        return false;
    };
    addr.to_socket_addrs()
        .ok()
        .and_then(|mut a| a.next())
        .map(|sa| {
            std::net::TcpStream::connect_timeout(&sa, std::time::Duration::from_millis(200)).is_ok()
        })
        .unwrap_or(false)
}
