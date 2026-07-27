//! Reading a project's `devcontainer.json`.
//!
//! Sinclair does not need this file — it builds and owns its own sandbox, and
//! nothing degrades when the file is absent, which is the common case. When a
//! project *does* have one, honouring it means the agents work in the same
//! environment the humans already use, so it is read opportunistically.
//!
//! Only the fields that change how a container is created or entered are
//! extracted. Features, customizations, and port forwarding belong to the Dev
//! Containers tooling and are ignored here.

use config::json::{self, Value};

/// The subset of `devcontainer.json` that affects a sandbox.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DevContainer {
    pub name: Option<String>,
    /// `image`, when the container runs a prebuilt image.
    pub image: Option<String>,
    /// `build.dockerfile`, relative to the config file.
    pub dockerfile: Option<String>,
    /// `workspaceFolder`: where the project is mounted inside.
    pub workspace_folder: Option<String>,
    /// `workspaceMount`, verbatim (engine `--mount` syntax).
    pub workspace_mount: Option<String>,
    pub remote_user: Option<String>,
    /// `containerEnv` + `remoteEnv`, merged in that order.
    pub env: Vec<(String, String)>,
    /// `mounts`, verbatim.
    pub mounts: Vec<String>,
    /// `runArgs`, passed through to the engine.
    pub run_args: Vec<String>,
    /// `shutdownAction`: `none` keeps the container up when the editor closes.
    pub shutdown_action: Option<String>,
    /// `postCreateCommand`, normalised to a list of shell commands.
    pub post_create: Vec<String>,
}

/// Candidate config paths inside a project, in the order the Dev Containers
/// tooling looks for them.
pub fn config_paths(project: &str) -> Vec<String> {
    let root = project.trim_end_matches('/');
    vec![
        format!("{root}/.devcontainer/devcontainer.json"),
        format!("{root}/.devcontainer.json"),
        format!("{root}/.devcontainer/devcontainer.jsonc"),
    ]
}

impl DevContainer {
    /// Where the project lands inside the container. An explicit
    /// `workspaceFolder` wins; otherwise the tooling's default is
    /// `/workspaces/<folder name>`.
    pub fn workspace_folder_for(&self, project: &str) -> String {
        if let Some(w) = self
            .workspace_folder
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            return w.to_string();
        }
        format!("/workspaces/{}", basename(project))
    }

    /// True when the project's mount lands at the same path it has on the host,
    /// which is the arrangement that keeps git worktrees valid on both sides.
    pub fn is_identity_mapped(&self, project: &str) -> bool {
        self.workspace_folder_for(project) == project.trim_end_matches('/')
    }

    /// True when closing the editor stops the container — which would kill a
    /// working agent team mid-task. `shutdownAction: "none"` avoids it.
    pub fn stops_on_close(&self) -> bool {
        !matches!(
            self.shutdown_action.as_deref().map(str::trim),
            Some("none") | Some("None")
        )
    }
}

/// Parse a `devcontainer.json`, resolving the `${…}` variables that depend on
/// where the project lives. Unknown variables are left alone.
pub fn parse(text: &str, project: &str) -> Result<DevContainer, String> {
    let value = json::parse(text).map_err(|e| format!("line {}: {}", e.line, e.message))?;
    let Value::Obj(members) = value else {
        return Err("expected a `{ ... }` object".to_string());
    };
    let mut dc = DevContainer::default();
    let mut container_env = Vec::new();
    let mut remote_env = Vec::new();
    for m in &members {
        match m.key.as_str() {
            "name" => dc.name = string(&m.value),
            "image" => dc.image = string(&m.value),
            "build" => {
                if let Value::Obj(build) = &m.value {
                    for b in build {
                        if b.key == "dockerfile" {
                            dc.dockerfile = string(&b.value);
                        }
                    }
                }
            }
            "workspaceFolder" => dc.workspace_folder = string(&m.value),
            "workspaceMount" => dc.workspace_mount = string(&m.value),
            // `remoteUser` is the one the tooling actually enters as, so it
            // wins over `containerUser` whichever order they appear in.
            "remoteUser" => dc.remote_user = string(&m.value),
            "containerUser" if dc.remote_user.is_none() => {
                dc.remote_user = string(&m.value)
            }
            "containerEnv" => container_env = pairs(&m.value),
            "remoteEnv" => remote_env = pairs(&m.value),
            "mounts" => dc.mounts = strings(&m.value),
            "runArgs" => dc.run_args = strings(&m.value),
            "shutdownAction" => dc.shutdown_action = string(&m.value),
            "postCreateCommand" => dc.post_create = commands(&m.value),
            _ => {}
        }
    }
    dc.env = container_env;
    for (k, v) in remote_env {
        dc.env.retain(|(x, _)| *x != k);
        dc.env.push((k, v));
    }
    Ok(substitute(dc, project))
}

/// Replace the location variables the spec defines. `containerWorkspaceFolder`
/// resolves against the folder this config itself selects, so it is applied in
/// a second pass once that is known.
fn substitute(mut dc: DevContainer, project: &str) -> DevContainer {
    let project = project.trim_end_matches('/');
    let local = |s: &str| {
        s.replace("${localWorkspaceFolder}", project)
            .replace("${localWorkspaceFolderBasename}", basename(project))
    };
    dc.workspace_folder = dc.workspace_folder.as_deref().map(local);
    dc.workspace_mount = dc.workspace_mount.as_deref().map(local);
    dc.image = dc.image.as_deref().map(local);

    let container = dc.workspace_folder_for(project);
    let all = |s: &str| {
        local(s)
            .replace("${containerWorkspaceFolder}", &container)
            .replace("${containerWorkspaceFolderBasename}", basename(&container))
    };
    dc.mounts = dc.mounts.iter().map(|s| all(s)).collect();
    dc.run_args = dc.run_args.iter().map(|s| all(s)).collect();
    dc.post_create = dc.post_create.iter().map(|s| all(s)).collect();
    dc.env = dc.env.iter().map(|(k, v)| (k.clone(), all(v))).collect();
    dc
}

fn basename(path: &str) -> &str {
    path.trim_end_matches('/')
        .rsplit('/')
        .find(|s| !s.is_empty())
        .unwrap_or("workspace")
}

fn string(v: &Value) -> Option<String> {
    match v {
        Value::Str(s) => Some(s.clone()),
        _ => None,
    }
}

fn strings(v: &Value) -> Vec<String> {
    match v {
        Value::Arr(items) => items.iter().filter_map(string).collect(),
        Value::Str(s) => vec![s.clone()],
        _ => Vec::new(),
    }
}

fn pairs(v: &Value) -> Vec<(String, String)> {
    match v {
        Value::Obj(members) => members
            .iter()
            .filter_map(|m| string(&m.value).map(|s| (m.key.clone(), s)))
            .collect(),
        _ => Vec::new(),
    }
}

/// `postCreateCommand` may be a string, an argv array, or an object of named
/// commands. All three normalise to a list of shell command lines.
fn commands(v: &Value) -> Vec<String> {
    match v {
        Value::Str(s) => vec![s.clone()],
        Value::Arr(items) => {
            let argv: Vec<String> = items.iter().filter_map(string).collect();
            if argv.is_empty() {
                Vec::new()
            } else {
                vec![argv.join(" ")]
            }
        }
        Value::Obj(members) => members
            .iter()
            .flat_map(|m| commands(&m.value))
            .collect(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
#[path = "../tests/devcontainer.rs"]
mod tests;
