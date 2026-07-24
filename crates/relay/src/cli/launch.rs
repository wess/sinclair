use super::{build, http, paths, LaunchArgs};
use anyhow::{anyhow, Result};
use std::io::Write;
use std::process::Command;

/// Launch an agent wired to the bus. Foreground takes over this terminal;
/// `--background` hands it to the server as a monitored worker. Both shapes
/// resolve through [`build::worker`], the same pipeline the MCP `spawn` tool
/// uses; the CLI's own defaults are stated here.
pub async fn launch(a: LaunchArgs) -> Result<()> {
    let info = paths::read_info()?;
    let endpoint = paths::endpoint(&info.addr);
    let name = resolve_name(a.name.as_deref())?;

    let cwd = a.cwd.clone().unwrap_or_else(|| {
        std::env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| ".".into())
    });
    let built = build::worker(
        &endpoint,
        &info.token,
        &build::Options {
            name: &name,
            role: &a.role,
            // The agent's cwd (default: where the user ran `launch`) is the
            // project root its role resolves against.
            role_root: Some(std::path::Path::new(&cwd)),
            agent: a.agent.as_deref(),
            task: a.task.as_deref(),
            channels: &a.channels,
            tools: &a.allow_tools,
            model: a.model.as_deref(),
            lead: a.lead,
            optimize: a.optimize,
            headless: a.background,
            // A background worker always skips permission prompts (it has no
            // terminal to answer them in); a foreground agent keeps the
            // interactive flow unless the caller says nobody is watching.
            skip_perms: a.background || a.skip_permissions,
            strict_mcp: a.strict_mcp,
            extra_args: &a.agent_args,
            bin: a.bin.as_deref(),
            custom: a.cmd.as_deref(),
        },
    )?;

    if a.background {
        let body = serde_json::json!({
            "name": name,
            "role": a.role,
            "program": built.program,
            "args": built.args,
            "cwd": cwd,
            "keep_alive": true,
            "session_id": built.session_id,
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
        let label = if a.cmd.is_some() { "custom" } else { built.agent.as_str() };
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
