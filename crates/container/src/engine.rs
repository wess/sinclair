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

    /// The hostname a container uses to reach a service on the host.
    ///
    /// Each engine ships its own name for this, and getting it wrong is the
    /// difference between an agent reaching the relay bus and every one of its
    /// `wait` calls failing. On a bridged network the name still needs an
    /// `--add-host …:host-gateway` entry to resolve (see
    /// [`crate::Sandbox::gateway`]); Docker Desktop provides it already, but
    /// adding it is harmless there and required on Linux.
    pub fn gateway_host(self) -> &'static str {
        match self {
            Self::Docker => "host.docker.internal",
            Self::Podman => "host.containers.internal",
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
