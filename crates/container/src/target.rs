//! A resolved launch target: the concrete `engine run …` invocation for a tab.

use crate::engine::Engine;
use crate::profile::Profile;

/// Everything needed to launch a container as a tab's backing process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub engine: Engine,
    pub image: String,
    /// The command/shell to run inside the container.
    pub command: String,
    /// Keep the container after the tab closes (no `--rm`).
    pub persist: bool,
    /// Container name (`--name`), used only for persistent containers so they
    /// can be found again later.
    pub name: Option<String>,
}

impl Target {
    /// Build a target from a chosen profile. `default_persist` applies when the
    /// profile does not pin its own lifecycle. `name` is attached only when the
    /// container persists.
    pub fn from_profile(
        engine: Engine,
        profile: &Profile,
        default_persist: bool,
        name: Option<String>,
    ) -> Self {
        let persist = profile.persist.unwrap_or(default_persist);
        Self {
            engine,
            image: profile.image.clone(),
            command: profile.command.clone(),
            persist,
            name: if persist { name } else { None },
        }
    }

    /// The argv that launches this container interactively. Shape:
    /// `engine run [--rm] -it [--name N] IMAGE COMMAND...`.
    pub fn argv(&self) -> Vec<String> {
        let mut argv = vec![self.engine.binary().to_string(), "run".to_string()];
        if !self.persist {
            argv.push("--rm".to_string());
        }
        argv.push("-it".to_string());
        if let Some(name) = &self.name {
            argv.push("--name".to_string());
            argv.push(name.clone());
        }
        argv.push(self.image.clone());
        argv.extend(self.command.split_whitespace().map(str::to_string));
        argv
    }
}

#[cfg(test)]
#[path = "../tests/target.rs"]
mod tests;
