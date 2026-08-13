use super::{build, http, paths, sandbox::Sandbox, LaunchArgs};
use anyhow::{anyhow, Result};
use std::io::Write;
use std::process::Command;

/// Launch an agent wired to the bus. Foreground takes over this terminal;
/// `--background` hands it to the server as a monitored worker. Both shapes
/// resolve through [`build::worker`], the same pipeline the MCP `spawn` tool
/// uses; the CLI's own defaults are stated here.
pub async fn launch(a: LaunchArgs) -> Result<()> {
    let info = paths::read_info()?;
    let name = resolve_name(a.name.as_deref())?;
    let sandbox = Sandbox::resolve(
        a.sandbox.as_deref(),
        a.sandbox_engine.as_deref(),
        a.sandbox_workdir.as_deref(),
    );
    // The bus is on the host either way; a sandboxed agent needs the address
    // that reaches it from inside the container.
    let endpoint = match &sandbox {
        Some(s) => s.endpoint(&paths::endpoint(&info.addr)),
        None => paths::endpoint(&info.addr),
    };

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
            mcp_dir_as_seen: sandbox
                .as_ref()
                .and(a.sandbox_relay_dir.as_deref())
                .or(sandbox.as_ref().map(|_| container::RELAY_DIR)),
        },
    )?;

    // A sandboxed agent runs inside the container; relay stays on the host.
    // The wrap carries the agent's environment as `-e` flags, so nothing is
    // lost by the engine sitting between the two.
    let (program, args) = match &sandbox {
        Some(s) => {
            let argv = s.wrap(&built.program, &built.args, &built.env);
            (argv[0].clone(), argv[1..].to_vec())
        }
        None => (built.program.clone(), built.args.clone()),
    };

    if a.background {
        let body = serde_json::json!({
            "name": name,
            "role": a.role,
            "program": program,
            "args": args,
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
        let label = if a.cmd.is_some() {
            "custom"
        } else {
            built.agent.as_str()
        };
        match &sandbox {
            Some(s) => println!(
                "launching {label} as '{name}' in sandbox {} on {endpoint} …",
                s.name
            ),
            None => println!("launching {label} as '{name}' on {endpoint} …"),
        }
        // Foreground: replace this process on Unix; on Windows, run it to
        // completion and exit with its status (there is no exec()).
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            let err = Command::new(&program).args(&args).current_dir(&cwd).exec();
            Err(anyhow!("failed to exec {program}: {err}"))
        }
        #[cfg(windows)]
        {
            let status = Command::new(&program)
                .args(&args)
                .current_dir(&cwd)
                .status()
                .map_err(|e| anyhow!("failed to run {program}: {e}"))?;
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
