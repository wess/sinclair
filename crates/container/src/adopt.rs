//! Finding a container that already belongs to a project.
//!
//! A project may already have a container up: one Sinclair started earlier, or
//! one VS Code's Dev Containers extension built. Starting a second beside it
//! wastes a toolchain and splits the team across two filesystems, so resolution
//! always looks before it creates.
//!
//! Discovery keys on `devcontainer.local_folder`, the label the Dev Containers
//! extension and the `devcontainer` CLI both stamp with the host workspace
//! path. Sinclair stamps the same label on the sandboxes it creates, so one
//! query finds either kind, and [`Owner`] says which — because that decides
//! whether Sinclair may ever stop or remove it.

use crate::engine::Engine;
use crate::sandbox::{parse_state, State, LABEL_OWNER, OWNER_SINCLAIR};

/// Field separator for the `--format` template, matching [`crate::list`].
const SEP: &str = "\u{1f}";

/// Who created a container, and therefore who may destroy it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Owner {
    /// Sinclair created it; Sinclair may stop and remove it.
    Sinclair,
    /// Something else created it — VS Code, the `devcontainer` CLI, compose.
    /// Sinclair may enter it and must never stop or remove it: the user's
    /// editor session is very likely attached to the same container.
    Foreign,
}

impl Owner {
    /// True when Sinclair is allowed to tear this container down.
    pub fn may_remove(self) -> bool {
        matches!(self, Self::Sinclair)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Sinclair => "Sinclair",
            Self::Foreign => "external",
        }
    }
}

/// A container discovered for a project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub id: String,
    pub name: String,
    pub image: String,
    pub state: State,
    pub owner: Owner,
    /// `devcontainer.config_file`, when the creator recorded one.
    pub config_file: Option<String>,
}

/// Argv listing every container (running or not) claimed by `project`.
pub fn find_argv(engine: Engine, project: &str) -> Vec<String> {
    vec![
        engine.binary().to_string(),
        "ps".to_string(),
        "-a".to_string(),
        "--filter".to_string(),
        format!("label=devcontainer.local_folder={}", project.trim_end_matches('/')),
        "--format".to_string(),
        format!("{{{{.ID}}}}{SEP}{{{{.Names}}}}{SEP}{{{{.Image}}}}{SEP}{{{{.State}}}}{SEP}{{{{.Labels}}}}"),
    ]
}

/// Parse the output of [`find_argv`].
pub fn parse_found(output: &str) -> Vec<Found> {
    output
        .lines()
        .filter_map(|line| {
            let line = line.trim_end_matches('\r');
            if line.trim().is_empty() {
                return None;
            }
            let mut f = line.split(SEP);
            let id = f.next()?.trim();
            if id.is_empty() {
                return None;
            }
            let name = f.next().unwrap_or("").trim();
            let image = f.next().unwrap_or("").trim();
            let state = parse_state(f.next().unwrap_or(""));
            let labels = parse_labels(f.next().unwrap_or(""));
            let owner = if label(&labels, LABEL_OWNER).as_deref() == Some(OWNER_SINCLAIR) {
                Owner::Sinclair
            } else {
                Owner::Foreign
            };
            Some(Found {
                id: id.to_string(),
                name: name.to_string(),
                image: image.to_string(),
                // A container the engine reports but has no state for is gone.
                state: if state == State::Missing {
                    State::Exited
                } else {
                    state
                },
                owner,
                config_file: label(&labels, "devcontainer.config_file"),
            })
        })
        .collect()
}

/// Pick the container to use out of everything found for a project: a running
/// one first, then a stopped one, preferring Sinclair's own within each group
/// so a foreign container is never restarted on the user's behalf.
pub fn best(found: &[Found]) -> Option<&Found> {
    let rank = |f: &Found| match (f.state.is_running(), f.owner) {
        (true, Owner::Sinclair) => 0,
        (true, Owner::Foreign) => 1,
        (false, Owner::Sinclair) => 2,
        (false, Owner::Foreign) => 3,
    };
    found.iter().min_by_key(|f| rank(f))
}

/// Parse the engine's comma-separated `k=v` label list.
pub fn parse_labels(raw: &str) -> Vec<(String, String)> {
    raw.split(',')
        .filter_map(|pair| {
            let pair = pair.trim();
            if pair.is_empty() {
                return None;
            }
            match pair.split_once('=') {
                Some((k, v)) => Some((k.trim().to_string(), v.trim().to_string())),
                None => Some((pair.to_string(), String::new())),
            }
        })
        .collect()
}

fn label(labels: &[(String, String)], key: &str) -> Option<String> {
    labels
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.clone())
        .filter(|v| !v.is_empty())
}

#[cfg(test)]
#[path = "../tests/adopt.rs"]
mod tests;
