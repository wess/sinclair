//! Building the sandbox image.
//!
//! A sandbox is only useful if the agent CLI exists inside it, and a stock
//! `debian:bookworm-slim` has no `claude`. Host binaries cannot be borrowed —
//! a macOS Mach-O executable will not run in a Linux container — so the image
//! has to provide them.
//!
//! Rather than depend on a registry Sinclair would have to publish and keep
//! current, a [`Recipe`] generates a thin Dockerfile layered on whatever base
//! the user names and builds it locally on first use. The tag embeds a hash of
//! the recipe, so the build runs again only when the recipe actually changes
//! and is otherwise an instant cache hit.
//!
//! The generated `RUN` lines assume a Debian/Ubuntu base (`apt-get`). Point
//! `sandbox-image` at a ready-made image to skip generation entirely.

use crate::engine::Engine;
use crate::sandbox::{HOME_DIR, RELAY_DIR};

/// The default base image: small, Debian, and `apt-get`-shaped so the
/// generated layer applies cleanly.
pub const DEFAULT_BASE: &str = "debian:bookworm-slim";

/// Packages every sandbox gets: enough to clone, build, and authenticate.
const BASE_PACKAGES: [&str; 8] = [
    "ca-certificates",
    "curl",
    "git",
    "openssh-client",
    "less",
    "procps",
    "nodejs",
    "npm",
];

/// The npm package that provides each agent CLI. An agent that is not listed
/// installs nothing — add it with a `setup` line instead.
fn agent_package(agent: &str) -> Option<&'static str> {
    match agent.trim().to_ascii_lowercase().as_str() {
        "claude" => Some("@anthropic-ai/claude-code"),
        "codex" => Some("@openai/codex"),
        "gemini" => Some("@google/gemini-cli"),
        _ => None,
    }
}

/// What to build on top of a base image.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Recipe {
    /// Base image. Empty means [`DEFAULT_BASE`].
    pub base: String,
    /// Extra apt packages beyond [`BASE_PACKAGES`].
    pub packages: Vec<String>,
    /// Agent CLIs to install (`claude`, `codex`, `gemini`).
    pub agents: Vec<String>,
    /// Extra shell commands, each becoming its own `RUN` layer. This is the
    /// escape hatch for toolchains Sinclair knows nothing about.
    pub setup: Vec<String>,
}

impl Recipe {
    /// A recipe installing `agents` on the default base.
    pub fn with_agents(agents: &[String]) -> Self {
        Self {
            agents: agents.to_vec(),
            ..Self::default()
        }
    }

    fn base_image(&self) -> &str {
        let base = self.base.trim();
        if base.is_empty() {
            DEFAULT_BASE
        } else {
            base
        }
    }

    /// The Dockerfile this recipe generates, fed to the engine on stdin.
    pub fn dockerfile(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("FROM {}\n", self.base_image()));
        out.push_str("ENV DEBIAN_FRONTEND=noninteractive\n");

        let mut packages: Vec<String> = BASE_PACKAGES.iter().map(|s| s.to_string()).collect();
        for p in &self.packages {
            let p = p.trim();
            if !p.is_empty() && !packages.iter().any(|x| x == p) {
                packages.push(p.to_string());
            }
        }
        out.push_str(&format!(
            "RUN apt-get update \\\n && apt-get install -y --no-install-recommends {} \\\n && rm -rf /var/lib/apt/lists/*\n",
            packages.join(" ")
        ));

        let npm: Vec<&str> = self.agents.iter().filter_map(|a| agent_package(a)).collect();
        if !npm.is_empty() {
            out.push_str(&format!("RUN npm install -g {}\n", npm.join(" ")));
        }

        // A fresh named volume is initialised from the image's contents at the
        // mount point, permissions included. Creating $HOME world-writable here
        // is what lets the sandbox run as an arbitrary `--user` uid on Linux
        // and still write its credentials.
        out.push_str(&format!(
            "RUN mkdir -p {HOME_DIR} {RELAY_DIR} && chmod 0777 {HOME_DIR}\n"
        ));
        out.push_str(&format!("ENV HOME={HOME_DIR}\n"));

        for line in &self.setup {
            let line = line.trim();
            if !line.is_empty() {
                out.push_str(&format!("RUN {line}\n"));
            }
        }
        out
    }

    /// The image tag for this recipe. Changing the recipe changes the tag, so
    /// a stale image is never silently reused.
    pub fn tag(&self) -> String {
        format!("sinclair-sandbox:{}", crate::hash::short(&self.dockerfile()))
    }

    /// Argv building this recipe. The Dockerfile goes in on stdin (`-`), so
    /// there is no build context directory and nothing to clean up.
    pub fn build_argv(&self, engine: Engine) -> Vec<String> {
        vec![
            engine.binary().to_string(),
            "build".to_string(),
            "-t".to_string(),
            self.tag(),
            "-".to_string(),
        ]
    }
}

/// Argv checking whether an image is present locally. Empty output means it is
/// not built yet; the exit status is zero either way.
pub fn exists_argv(engine: Engine, tag: &str) -> Vec<String> {
    vec![
        engine.binary().to_string(),
        "images".to_string(),
        "-q".to_string(),
        tag.to_string(),
    ]
}

#[cfg(test)]
#[path = "../tests/image.rs"]
mod tests;
