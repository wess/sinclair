use super::{agent, http, paths, role, LaunchArgs};
use anyhow::{anyhow, Result};
use std::io::Write;
use std::process::Command;

/// Launch an agent wired to the bus. Foreground takes over this terminal;
/// `--background` hands it to the server as a monitored worker.
pub async fn launch(a: LaunchArgs) -> Result<()> {
    let info = paths::read_info()?;
    let endpoint = paths::endpoint(&info.addr);
    let name = resolve_name(a.name.as_deref())?;

    let role = role::resolve(&a.role);
    let brief = role.as_ref().map(|r| r.description.clone()).unwrap_or_default();
    let mut channels = role.as_ref().map(|r| r.channels.clone()).unwrap_or_default();
    for c in &a.channels {
        if !channels.contains(c) {
            channels.push(c.clone());
        }
    }
    let agent_name = a
        .agent
        .clone()
        .or_else(|| role.as_ref().and_then(|r| r.agent.clone()))
        .unwrap_or_else(|| "claude".to_string());
    let model = a
        .model
        .clone()
        .or_else(|| role.as_ref().and_then(|r| r.model.clone()));
    // Tool access: the role's `tools` plus any --allow-tool flags (issue #8).
    let mut allowed_tools = role.as_ref().map(|r| r.tools.clone()).unwrap_or_default();
    for t in &a.allow_tools {
        if !allowed_tools.contains(t) {
            allowed_tools.push(t.clone());
        }
    }

    let cwd = a.cwd.clone().unwrap_or_else(|| {
        std::env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| ".".into())
    });
    let interactive = !a.background && (a.lead || role.as_ref().is_some_and(|r| r.driver));
    let mcp = paths::write_mcp_config(&endpoint, &name, &info.token)?;
    let prompt =
        agent::harness_prompt(&name, &a.role, &brief, &channels, a.task.as_deref(), interactive, a.optimize);

    let mut built = agent::build(&agent::Spec {
        agent: &agent_name,
        custom: a.cmd.as_deref(),
        name: &name,
        role: &a.role,
        prompt: &prompt,
        mcp_file: &mcp,
        url: &endpoint,
        headless: a.background,
        model: model.as_deref(),
        channels: &channels,
        skip_perms: a.background,
        strict_mcp: a.strict_mcp,
        allowed_tools: &allowed_tools,
        extra_args: &a.agent_args,
    })?;

    if let Some(bin) = a.bin.as_deref().map(str::trim).filter(|b| !b.is_empty()) {
        if a.cmd.is_none() && agent_name != "ollama" {
            built.program = bin.to_string();
        }
    }

    if a.background {
        // A background claude worker gets a fixed session id so it can resume its
        // context after a crash or daemon restart (issue #4).
        let session_id = (agent_name == "claude").then(|| uuid::Uuid::new_v4().to_string());
        let body = serde_json::json!({
            "name": name,
            "role": a.role,
            "program": built.program,
            "args": built.args,
            "cwd": cwd,
            "keep_alive": true,
            "session_id": session_id,
        })
        .to_string();
        let resp = http::post(&info.addr, "/control/spawn", &body)?;
        let v: serde_json::Value = serde_json::from_str(&resp).unwrap_or_default();
        if v["ok"].as_bool().unwrap_or(false) {
            println!(
                "launched background worker '{}' (logs: {})",
                name,
                v["log"].as_str().unwrap_or("?")
            );
            Ok(())
        } else {
            Err(anyhow!(
                "spawn failed: {}",
                v["error"].as_str().unwrap_or("unknown")
            ))
        }
    } else {
        let label = if a.cmd.is_some() { "custom" } else { agent_name.as_str() };
        println!("launching {label} as '{name}' on {endpoint} …");
        // Foreground: replace this process on Unix; on Windows, run it to
        // completion and exit with its status (there is no exec()).
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            let err = Command::new(&built.program)
                .args(&built.args)
                .current_dir(&cwd)
                .exec();
            Err(anyhow!("failed to exec {}: {err}", built.program))
        }
        #[cfg(windows)]
        {
            let status = Command::new(&built.program)
                .args(&built.args)
                .current_dir(&cwd)
                .status()
                .map_err(|e| anyhow!("failed to run {}: {e}", built.program))?;
            std::process::exit(status.code().unwrap_or(1));
        }
    }
}

/// Use the provided name, or prompt for one (menu-driven launch passes none).
fn resolve_name(name: Option<&str>) -> Result<String> {
    if let Some(n) = name.map(str::trim).filter(|s| !s.is_empty()) {
        return Ok(n.to_string());
    }
    print!("agent name: ");
    std::io::stdout().flush().ok();
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    let n = line.trim().to_string();
    if n.is_empty() {
        return Err(anyhow!("no name given"));
    }
    Ok(n)
}
