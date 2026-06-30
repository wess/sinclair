//! Listing and attaching to already-running containers.
//!
//! `ps_argv` asks the engine for running containers in a stable, parseable
//! format; `parse_ps` turns that into [`Running`] rows; `exec_argv` builds the
//! interactive `exec` invocation used to attach a tab to one of them.

use crate::engine::Engine;

/// Field separator for the `docker ps --format` template. Chosen to not occur
/// in ids, names, images, or status strings.
const SEP: &str = "\u{1f}"; // ASCII Unit Separator

/// A running container as reported by the engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Running {
    pub id: String,
    pub name: String,
    pub image: String,
    pub status: String,
}

/// Argv listing running containers, one per line, fields separated by [`SEP`].
pub fn ps_argv(engine: Engine) -> Vec<String> {
    vec![
        engine.binary().to_string(),
        "ps".to_string(),
        "--format".to_string(),
        format!("{{{{.ID}}}}{SEP}{{{{.Names}}}}{SEP}{{{{.Image}}}}{SEP}{{{{.Status}}}}"),
    ]
}

/// Parse the output of [`ps_argv`] into rows. Blank and malformed lines are
/// skipped.
pub fn parse_ps(output: &str) -> Vec<Running> {
    output
        .lines()
        .filter_map(|line| {
            let line = line.trim_end_matches('\r');
            if line.is_empty() {
                return None;
            }
            let mut f = line.split(SEP);
            let id = f.next()?.trim();
            let name = f.next().unwrap_or("").trim();
            let image = f.next().unwrap_or("").trim();
            let status = f.next().unwrap_or("").trim();
            if id.is_empty() {
                return None;
            }
            Some(Running {
                id: id.to_string(),
                name: name.to_string(),
                image: image.to_string(),
                status: status.to_string(),
            })
        })
        .collect()
}

/// Argv that attaches an interactive shell to an existing container
/// (`engine exec -it <id> <command...>`).
pub fn exec_argv(engine: Engine, container: &str, command: &str) -> Vec<String> {
    let mut argv = vec![
        engine.binary().to_string(),
        "exec".to_string(),
        "-it".to_string(),
        container.to_string(),
    ];
    let words: Vec<String> = command.split_whitespace().map(str::to_string).collect();
    if words.is_empty() {
        argv.push("sh".to_string());
    } else {
        argv.extend(words);
    }
    argv
}

/// Argv that attaches an interactive shell to a container, preferring `bash`
/// but falling back to `sh` (minimal images like Alpine have only `sh`). The
/// fallback script is a single argv element passed to `sh -c`.
pub fn attach_argv(engine: Engine, container: &str) -> Vec<String> {
    vec![
        engine.binary().to_string(),
        "exec".to_string(),
        "-it".to_string(),
        container.to_string(),
        "sh".to_string(),
        "-c".to_string(),
        "command -v bash >/dev/null 2>&1 && exec bash || exec sh".to_string(),
    ]
}

#[cfg(test)]
#[path = "../tests/list.rs"]
mod tests;
