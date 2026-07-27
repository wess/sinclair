//! Turning settings (and a project's `devcontainer.json`, when it has one)
//! into a concrete sandbox description.
//!
//! Pure resolution: no engine is contacted and no file is read here, so the
//! whole policy — which image, which mounts, which user, what to warn about —
//! is testable without Docker installed.

use container::{Engine, Limits, Mount, Network, Recipe, Sandbox};

/// A resolved sandbox plus everything the host needs to bring it up.
#[derive(Debug, Clone)]
pub struct Spec {
    /// Absolute host path of the project the sandbox serves.
    pub project: String,
    pub sandbox: Sandbox,
    /// The image to build first. `None` when the image is used as-is.
    pub recipe: Option<Recipe>,
    /// Things the user should know before agents start working in here.
    pub notes: Vec<String>,
}

/// Inputs the caller resolves from the environment before building a spec.
pub struct Env<'a> {
    pub engine: Engine,
    pub project: &'a str,
    /// The project's parsed `devcontainer.json`, when it has one and
    /// `sandbox-devcontainer` is on.
    pub devcontainer: Option<&'a container::DevContainer>,
    /// `uid:gid` owning the project, used when `sandbox-user` is `host`.
    pub host_user: Option<String>,
    /// Host directory holding generated relay MCP configs.
    pub relay_dir: Option<&'a str>,
    /// Agent CLIs to install when nothing is configured explicitly.
    pub default_agent: &'a str,
}

/// Build the spec for `env` under `opts`.
pub fn build(opts: &config::Options, env: &Env) -> Spec {
    let project = env.project.trim_end_matches('/').to_string();
    let mut notes = Vec::new();

    let (image, recipe) = image_for(opts, env, &mut notes);
    let mut sandbox = Sandbox::for_project(env.engine, &project, &image)
        .with_home_volume(&container::home_volume_for(&project));
    if let Some(dir) = env.relay_dir {
        sandbox = sandbox.with_relay_dir(dir);
    }

    for raw in &opts.sandbox_mount {
        match Mount::parse(raw) {
            Ok(m) => sandbox.mounts.push(m),
            Err(e) => notes.push(format!("sandbox-mount `{raw}`: {e}")),
        }
    }

    // devcontainer env first, so an explicit `sandbox-env` still wins.
    if let Some(dc) = env.devcontainer {
        for (k, v) in &dc.env {
            sandbox = sandbox.with_env(k, v);
        }
    }
    for raw in &opts.sandbox_env {
        match raw.split_once('=') {
            Some((k, v)) if !k.trim().is_empty() => sandbox = sandbox.with_env(k.trim(), v),
            _ => notes.push(format!("sandbox-env `{raw}`: expected KEY=VALUE")),
        }
    }

    sandbox.user = user_for(opts, env, &mut notes);
    if let Some(raw) = opts.sandbox_network.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        match Network::parse(raw) {
            Some(n) => sandbox.network = n,
            None => notes.push(format!("sandbox-network `{raw}`: expected bridge, host, or none")),
        }
    }
    if sandbox.network == Network::None {
        notes.push(
            "sandbox-network is `none`: agents in this sandbox cannot reach the relay bus."
                .to_string(),
        );
    }
    sandbox.limits = Limits {
        memory: opts.sandbox_memory.clone(),
        cpus: opts.sandbox_cpus.clone(),
        // A supervisor can spawn its cap of workers into one container; a pid
        // ceiling is what keeps a runaway loop off the rest of the machine.
        pids: Some(4096),
    };

    if let Some(dc) = env.devcontainer {
        if dc.stops_on_close() {
            notes.push(
                "This project's devcontainer.json stops the container when the editor closes. \
                 Set \"shutdownAction\": \"none\" so closing VS Code does not end a working team."
                    .to_string(),
            );
        }
    }

    Spec {
        project,
        sandbox,
        recipe,
        notes,
    }
}

/// Which image to run, and whether Sinclair has to build it first.
///
/// An explicit `sandbox-image` is used verbatim — the user is saying the image
/// is already right. Otherwise a thin layer is generated, using the project's
/// devcontainer image as the base when it has one, so agents land in the same
/// toolchain the humans use with the agent CLIs added on top.
fn image_for(opts: &config::Options, env: &Env, notes: &mut Vec<String>) -> (String, Option<Recipe>) {
    if let Some(image) = opts.sandbox_image.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        return (image.to_string(), None);
    }
    let mut agents: Vec<String> = opts
        .sandbox_agents
        .iter()
        .map(|a| a.trim().to_string())
        .filter(|a| !a.is_empty())
        .collect();
    if agents.is_empty() {
        agents.push(env.default_agent.to_string());
    }
    let base = env
        .devcontainer
        .and_then(|dc| dc.image.clone())
        .filter(|s| !s.trim().is_empty())
        .or_else(|| opts.sandbox_base.clone().filter(|s| !s.trim().is_empty()))
        .unwrap_or_else(|| container::DEFAULT_BASE.to_string());
    if env.devcontainer.is_some_and(|dc| dc.dockerfile.is_some()) && opts.sandbox_image.is_none() {
        notes.push(
            "This project's devcontainer.json builds from a Dockerfile, which Sinclair does not \
             build. Set `sandbox-image` to the image it produces, or let the default base be used."
                .to_string(),
        );
    }
    let recipe = Recipe {
        base,
        packages: opts.sandbox_packages.clone(),
        agents,
        setup: opts.sandbox_setup.clone(),
    };
    (recipe.tag(), Some(recipe))
}

/// The `--user` the container runs as.
///
/// On Linux a container running as root writes root-owned files into the bind
/// mount, i.e. into the user's own repository. `host` resolves to the uid that
/// owns the project, which avoids it. macOS remaps ownership in the file
/// sharing layer, so the default there is to leave it alone.
fn user_for(opts: &config::Options, env: &Env, notes: &mut Vec<String>) -> Option<String> {
    let raw = opts.sandbox_user.as_deref().map(str::trim).unwrap_or("");
    match raw {
        "" => {
            if cfg!(target_os = "linux") && env.host_user.is_some() {
                env.host_user.clone()
            } else {
                None
            }
        }
        "host" => {
            if env.host_user.is_none() {
                notes.push("sandbox-user is `host` but the project's owner could not be read; \
                            running as the image's user."
                    .to_string());
            }
            env.host_user.clone()
        }
        "root" => None,
        other => Some(other.to_string()),
    }
}

#[cfg(test)]
#[path = "../../tests/sandbox.rs"]
mod tests;
