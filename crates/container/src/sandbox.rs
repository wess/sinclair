//! The shared sandbox: one long-lived container per project that the human and
//! every agent on a team work inside.
//!
//! The shape differs from a one-off OS tab ([`crate::Target`]) in three ways,
//! and each one exists so several panes can share a single container:
//!
//! - It is **created detached** running a keepalive loop, so its life is not
//!   tied to whichever pane happened to start it. Panes attach with `exec`.
//! - It **identity-mounts** the project, so a path means the same thing on both
//!   sides and git worktrees created from either side resolve from the other.
//! - It is **named deterministically** from the project path, so reopening a
//!   project finds the container that is already up instead of starting a
//!   second one.
//!
//! Everything here is argv construction. Running the commands, tracking how
//! many panes are attached, and deciding when to tear the container down are
//! the host's job.

use crate::engine::Engine;
use crate::mount::Mount;

/// Label marking a container as a Sinclair sandbox.
pub const LABEL_SANDBOX: &str = "sinclair.sandbox";
/// Label recording the host project path a sandbox belongs to.
pub const LABEL_PROJECT: &str = "sinclair.project";
/// Label recording who created the container, which decides who may remove it.
pub const LABEL_OWNER: &str = "sinclair.owner";
/// Value of [`LABEL_OWNER`] for containers Sinclair created itself.
pub const OWNER_SINCLAIR: &str = "sinclair";

/// Where the agent's `$HOME` lives inside the sandbox.
///
/// It is a named volume rather than part of the image so a `claude login` (and
/// the folder-trust answers, and shell history) survives rebuilding the image
/// or recreating the container.
pub const HOME_DIR: &str = "/sandbox/home";

/// Where the per-agent relay MCP configs are mounted.
pub const RELAY_DIR: &str = "/sandbox/relay";

/// The container's process: a keepalive loop, so the sandbox outlives any one
/// pane. `sleep infinity` is GNU-only — busybox (Alpine) rejects it — so this
/// spells the loop out and works on every base image.
const KEEPALIVE: [&str; 3] = ["sh", "-c", "while :; do sleep 3600; done"];

/// How the sandbox reaches the network, and with it the relay bus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Network {
    /// The engine's default bridge. Combined with a gateway host entry this
    /// reaches a relay bound to host loopback.
    #[default]
    Bridge,
    /// Share the host's network namespace: loopback inside *is* host loopback,
    /// so the bus is reachable with no rebinding and nothing is exposed. Linux
    /// only in practice.
    Host,
    /// No network at all. The strictest sandbox; agents cannot reach the bus.
    None,
}

impl Network {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "bridge" | "bus" | "default" => Some(Self::Bridge),
            "host" => Some(Self::Host),
            "none" | "off" => Some(Self::None),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Bridge => "bridge",
            Self::Host => "host",
            Self::None => "none",
        }
    }
}

/// Resource ceilings, so a supervisor that spawns its cap of workers cannot
/// take the host down with it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Limits {
    /// `--memory`, e.g. `8g`.
    pub memory: Option<String>,
    /// `--cpus`, e.g. `4`.
    pub cpus: Option<String>,
    /// `--pids-limit`.
    pub pids: Option<u32>,
}

impl Limits {
    fn args(&self) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(m) = self
            .memory
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            out.push("--memory".to_string());
            out.push(m.to_string());
        }
        if let Some(c) = self
            .cpus
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            out.push("--cpus".to_string());
            out.push(c.to_string());
        }
        if let Some(p) = self.pids {
            out.push("--pids-limit".to_string());
            out.push(p.to_string());
        }
        out
    }
}

/// The engine's view of a container that may or may not exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Missing,
    Created,
    Running,
    Paused,
    Exited,
}

impl State {
    /// True when a pane can `exec` into it right now.
    pub fn is_running(self) -> bool {
        matches!(self, Self::Running)
    }

    /// True when the container exists but needs `start` first.
    pub fn is_stopped(self) -> bool {
        matches!(self, Self::Created | Self::Exited | Self::Paused)
    }
}

/// A resolved sandbox: everything needed to create it and to exec into it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sandbox {
    pub engine: Engine,
    /// Container name — deterministic, from the project path (see [`name_for`]).
    pub name: String,
    /// The image to run. Either a user-supplied reference or the tag of an
    /// image Sinclair built (see [`crate::Recipe`]).
    pub image: String,
    /// Default working directory for panes: the project root as seen inside.
    pub workdir: String,
    pub mounts: Vec<Mount>,
    pub env: Vec<(String, String)>,
    /// `--user`, e.g. `501:20`. Left unset the container runs as the image's
    /// user (usually root), which is right on macOS where the file-sharing
    /// layer remaps ownership, and wrong on Linux where it would leave
    /// root-owned files in the user's repo.
    pub user: Option<String>,
    pub network: Network,
    /// Add a `host.docker.internal` entry pointing at the host gateway, so a
    /// relay bound to host loopback is reachable from a bridged container.
    pub gateway: bool,
    pub limits: Limits,
    pub labels: Vec<(String, String)>,
}

impl Sandbox {
    /// A sandbox for `project` (an absolute host path), identity-mounting it.
    pub fn for_project(engine: Engine, project: &str, image: &str) -> Self {
        Self {
            engine,
            name: name_for(project),
            image: image.to_string(),
            workdir: project.to_string(),
            mounts: vec![Mount::identity(project)],
            env: vec![("HOME".to_string(), HOME_DIR.to_string())],
            user: None,
            network: Network::default(),
            gateway: true,
            limits: Limits::default(),
            labels: vec![
                (LABEL_SANDBOX.to_string(), "1".to_string()),
                (LABEL_PROJECT.to_string(), project.to_string()),
                (LABEL_OWNER.to_string(), OWNER_SINCLAIR.to_string()),
                // Stamped so VS Code's Dev Containers extension and the
                // `devcontainer` CLI find this container for the same folder
                // rather than building a second one beside it.
                ("devcontainer.local_folder".to_string(), project.to_string()),
            ],
        }
    }

    /// Attach the named volume holding the agent's `$HOME`.
    pub fn with_home_volume(mut self, volume: &str) -> Self {
        self.mounts.push(Mount::rw(volume, HOME_DIR));
        self
    }

    /// Mount the directory holding generated relay MCP configs, so an agent
    /// launched into the sandbox can read the one addressed to it.
    pub fn with_relay_dir(mut self, host_dir: &str) -> Self {
        self.mounts.push(Mount::ro(host_dir, RELAY_DIR));
        self
    }

    /// Add an environment variable, replacing any existing entry for the key.
    pub fn with_env(mut self, key: &str, value: &str) -> Self {
        self.env.retain(|(k, _)| k != key);
        self.env.push((key.to_string(), value.to_string()));
        self
    }

    /// Create the container detached, running the keepalive loop.
    pub fn create_argv(&self) -> Vec<String> {
        let mut argv = vec![
            self.engine.binary().to_string(),
            "run".to_string(),
            "-d".to_string(),
            // Reap the zombies a long-lived exec host accumulates.
            "--init".to_string(),
            "--name".to_string(),
            self.name.clone(),
            "-w".to_string(),
            self.workdir.clone(),
        ];
        for m in &self.mounts {
            argv.push("-v".to_string());
            argv.push(m.arg());
        }
        for (k, v) in &self.env {
            argv.push("-e".to_string());
            argv.push(format!("{k}={v}"));
        }
        if let Some(u) = self
            .user
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            argv.push("--user".to_string());
            argv.push(u.to_string());
        }
        match self.network {
            Network::Bridge => {
                if self.gateway {
                    argv.push("--add-host".to_string());
                    argv.push(format!("{}:host-gateway", self.engine.gateway_host()));
                }
            }
            Network::Host => {
                argv.push("--network".to_string());
                argv.push("host".to_string());
            }
            Network::None => {
                argv.push("--network".to_string());
                argv.push("none".to_string());
            }
        }
        argv.extend(self.limits.args());
        for (k, v) in &self.labels {
            argv.push("--label".to_string());
            argv.push(format!("{k}={v}"));
        }
        argv.push(self.image.clone());
        argv.extend(KEEPALIVE.iter().map(|s| s.to_string()));
        argv
    }

    /// Attach an interactive login shell, preferring bash. A login shell is
    /// what picks up the PATH the image's profile sets, so an agent sees the
    /// same toolchain a human does in the same container.
    pub fn shell_argv(&self, cwd: Option<&str>) -> Vec<String> {
        let mut argv = self.exec_prefix(cwd);
        argv.push("sh".to_string());
        argv.push("-c".to_string());
        argv.push("command -v bash >/dev/null 2>&1 && exec bash -l || exec sh -l".to_string());
        argv
    }

    /// Run `command` inside the sandbox through a login shell.
    pub fn exec_argv(&self, command: &str, cwd: Option<&str>) -> Vec<String> {
        let mut argv = self.exec_prefix(cwd);
        argv.push("sh".to_string());
        argv.push("-lc".to_string());
        argv.push(command.to_string());
        argv
    }

    /// `engine exec -it [-u …] -w … [-e …] NAME`, the head every attach shares.
    fn exec_prefix(&self, cwd: Option<&str>) -> Vec<String> {
        let mut argv = vec![
            self.engine.binary().to_string(),
            "exec".to_string(),
            "-it".to_string(),
        ];
        if let Some(u) = self
            .user
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            argv.push("--user".to_string());
            argv.push(u.to_string());
        }
        argv.push("-w".to_string());
        argv.push(cwd.unwrap_or(&self.workdir).to_string());
        for (k, v) in &self.env {
            argv.push("-e".to_string());
            argv.push(format!("{k}={v}"));
        }
        argv.push(self.name.clone());
        argv
    }

    pub fn start_argv(&self) -> Vec<String> {
        vec![
            self.engine.binary().to_string(),
            "start".to_string(),
            self.name.clone(),
        ]
    }

    pub fn stop_argv(&self) -> Vec<String> {
        vec![
            self.engine.binary().to_string(),
            "stop".to_string(),
            self.name.clone(),
        ]
    }

    pub fn rm_argv(&self) -> Vec<String> {
        vec![
            self.engine.binary().to_string(),
            "rm".to_string(),
            "-f".to_string(),
            self.name.clone(),
        ]
    }

    pub fn state_argv(&self) -> Vec<String> {
        state_argv(self.engine, &self.name)
    }
}

/// Argv asking the engine for one container's state. Uses `ps -a` rather than
/// `inspect` so a missing container is empty output and a zero exit, not an
/// error the caller has to special-case.
pub fn state_argv(engine: Engine, name: &str) -> Vec<String> {
    vec![
        engine.binary().to_string(),
        "ps".to_string(),
        "-a".to_string(),
        "--filter".to_string(),
        format!("name=^{name}$"),
        "--format".to_string(),
        "{{.State}}".to_string(),
    ]
}

/// Parse the output of [`state_argv`].
pub fn parse_state(output: &str) -> State {
    match output
        .trim()
        .lines()
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "running" | "up" => State::Running,
        "created" | "configured" => State::Created,
        "paused" => State::Paused,
        "" => State::Missing,
        _ => State::Exited,
    }
}

/// The deterministic container name for a project path: a readable slug of the
/// final path component plus a hash of the whole path, so two checkouts named
/// `api` in different places never collide and reopening either finds its own.
pub fn name_for(project: &str) -> String {
    let base = project
        .trim_end_matches('/')
        .rsplit('/')
        .find(|s| !s.is_empty())
        .unwrap_or("project");
    format!(
        "sinclair-sbx-{}-{}",
        slug(base),
        crate::hash::short(project)
    )
}

/// The named volume holding the agent `$HOME` for a project.
pub fn home_volume_for(project: &str) -> String {
    format!("{}-home", name_for(project))
}

/// Lowercase, keeping only characters an engine accepts in a container name.
fn slug(s: &str) -> String {
    let out: String = s
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let out = out.trim_matches('-').to_string();
    if out.is_empty() {
        "project".to_string()
    } else {
        out.chars().take(24).collect()
    }
}

#[cfg(test)]
#[path = "../tests/sandbox.rs"]
mod tests;
