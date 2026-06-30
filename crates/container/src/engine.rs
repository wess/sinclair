//! The container engine: which CLI drives the container (`docker` or
//! `podman`). Both share the run/exec command surface this crate relies on.

use std::path::Path;

/// A container runtime CLI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    Docker,
    Podman,
}

impl Engine {
    /// The executable name looked up on `$PATH`.
    pub fn binary(self) -> &'static str {
        match self {
            Self::Docker => "docker",
            Self::Podman => "podman",
        }
    }

    /// Human-readable name for menus and diagnostics.
    pub fn label(self) -> &'static str {
        match self {
            Self::Docker => "Docker",
            Self::Podman => "Podman",
        }
    }

    /// Parse an explicit engine preference (config `container-engine`).
    /// `auto` (or empty) yields `None`, meaning "detect".
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "docker" => Some(Self::Docker),
            "podman" => Some(Self::Podman),
            _ => None,
        }
    }

    /// The first engine whose binary is on `$PATH`, Docker preferred.
    pub fn detect() -> Option<Self> {
        [Self::Docker, Self::Podman]
            .into_iter()
            .find(|e| on_path(e.binary()))
    }

    /// Argv that probes whether the engine daemon is up and responding
    /// (`docker info`). A zero exit status means running.
    pub fn info_argv(self) -> Vec<String> {
        vec![
            self.binary().to_string(),
            "info".to_string(),
            "--format".to_string(),
            "{{.ServerVersion}}".to_string(),
        ]
    }

    /// True when the engine binary exists *and* its daemon answers. Runs
    /// `info_argv` and checks the exit status. This shells out, so it is not
    /// unit-tested; callers should treat it as a best-effort probe.
    pub fn running(self) -> bool {
        std::process::Command::new(self.binary())
            .args(&self.info_argv()[1..])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// Resolve the engine to use: an explicit `pref` when it names one that is
    /// installed, otherwise autodetect. `auto`/unknown prefs fall back to
    /// detection. Returns `None` when no engine is available.
    pub fn resolve(pref: Option<&str>) -> Option<Self> {
        if let Some(engine) = pref.and_then(Self::parse) {
            if on_path(engine.binary()) {
                return Some(engine);
            }
        }
        Self::detect()
    }
}

/// True when `bin` resolves to a file on any `$PATH` entry.
fn on_path(bin: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| {
        let candidate = if dir.as_os_str().is_empty() {
            Path::new(bin).to_path_buf()
        } else {
            dir.join(bin)
        };
        candidate.is_file()
    })
}

#[cfg(test)]
#[path = "../tests/engine.rs"]
mod tests;
