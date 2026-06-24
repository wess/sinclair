use super::{agent, http, paths, LaunchArgs};
use anyhow::{anyhow, Result};
use std::io::Write;
use std::os::unix::process::CommandExt;
use std::process::Command;

/// Launch an agent wired to the bus. Foreground takes over this terminal;
/// `--background` hands it to the server as a monitored worker.
pub async fn launch(a: LaunchArgs) -> Result<()> {
    let info = paths::read_info()?;
    let endpoint = paths::endpoint(&info.addr);
    let name = resolve_name(a.name.as_deref())?;
    let cwd = a.cwd.clone().unwrap_or_else(|| {
        std::env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| ".".into())
    });
    let mcp = paths::write_mcp_config(&endpoint, &name)?;
    let prompt = agent::harness_prompt(&name, &a.role, &a.channels, a.task.as_deref());

    let built = agent::build(&agent::Spec {
        agent: &a.agent,
        custom: a.cmd.as_deref(),
        name: &name,
        prompt: &prompt,
        mcp_file: &mcp,
        url: &endpoint,
        headless: a.background,
        model: a.model.as_deref(),
        skip_perms: a.background,
    })?;

    if a.background {
        let body = serde_json::json!({
            "name": name,
            "role": a.role,
            "program": built.program,
            "args": built.args,
            "cwd": cwd,
            "keep_alive": true,
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
        let label = if a.cmd.is_some() { "custom" } else { a.agent.as_str() };
        println!("launching {label} as '{name}' on {endpoint} …");
        // exec replaces this process with the agent CLI; only returns on failure.
        let err = Command::new(&built.program)
            .args(&built.args)
            .current_dir(&cwd)
            .exec();
        Err(anyhow!("failed to exec {}: {err}", built.program))
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
