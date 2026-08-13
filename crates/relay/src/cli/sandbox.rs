//! Launching an agent inside a project's shared sandbox container.
//!
//! Relay itself stays on the host. Only the *last* step changes: instead of
//! exec-ing the agent CLI directly, it is exec'd inside the container. Role
//! resolution, the harness prompt, channel merging, and the MCP config are all
//! built by [`crate::cli::build::worker`] exactly as they are for a host
//! launch, so the two planes cannot drift.
//!
//! Two things have to be restated from the container's point of view:
//!
//! - **The bus URL.** Relay binds host loopback, which inside a bridged
//!   container is the container's own loopback. The endpoint handed to the
//!   agent names the engine's gateway host instead.
//! - **The MCP config path.** It is written into a directory the sandbox
//!   mounts, and the agent is given the path *as mounted*, not the host one.

use container::Engine;

/// Where an agent runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sandbox {
    pub engine: Engine,
    /// Container name or id to exec into.
    pub name: String,
    /// Working directory inside the container.
    pub workdir: String,
}

impl Sandbox {
    /// Resolve the launch flags into a target, or `None` for a host launch.
    pub fn resolve(
        name: Option<&str>,
        engine: Option<&str>,
        workdir: Option<&str>,
    ) -> Option<Self> {
        let name = name.map(str::trim).filter(|s| !s.is_empty())?;
        Some(Self {
            engine: Engine::parse(engine.unwrap_or("")).unwrap_or(Engine::Docker),
            name: name.to_string(),
            workdir: workdir
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or(".")
                .to_string(),
        })
    }

    /// The bus URL as seen from inside the container. A loopback endpoint is
    /// rewritten to the engine's gateway host; anything else (a LAN address, a
    /// name the container can already resolve) is left alone.
    pub fn endpoint(&self, endpoint: &str) -> String {
        let gateway = self.engine.gateway_host();
        endpoint
            .replace("//127.0.0.1:", &format!("//{gateway}:"))
            .replace("//localhost:", &format!("//{gateway}:"))
            .replace("//0.0.0.0:", &format!("//{gateway}:"))
    }

    /// Wrap a resolved command so it runs inside the container.
    ///
    /// The agent is exec'd through a login shell: the image's profile is what
    /// puts its toolchain on `PATH`, and an agent that cannot find the tools a
    /// human sees in the same container is worse than no sandbox at all.
    pub fn wrap(&self, program: &str, args: &[String], env: &[(String, String)]) -> Vec<String> {
        let mut argv = vec![
            self.engine.binary().to_string(),
            "exec".to_string(),
            "-it".to_string(),
            "-w".to_string(),
            self.workdir.clone(),
        ];
        for (k, v) in env {
            argv.push("-e".to_string());
            argv.push(format!("{k}={v}"));
        }
        argv.push(self.name.clone());
        argv.push("sh".to_string());
        argv.push("-lc".to_string());
        let mut line = quote(program);
        for a in args {
            line.push(' ');
            line.push_str(&quote(a));
        }
        // `exec` so the agent is the shell's own process: signals and the exit
        // status reach it directly rather than being absorbed by a wrapper.
        argv.push(format!("exec {line}"));
        argv
    }
}

/// Single-quote for `sh -lc`. Prompts contain newlines and quotes, so nothing
/// may be passed through unquoted.
fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

#[cfg(test)]
#[path = "../../tests/cli/sandbox.rs"]
mod tests;
