//! Autosuggestion/autocomplete for the focused terminal: assembles completion
//! candidates for the line being typed (read from the vt input mark) out of the
//! enabled sources — captured history, a built-in common-command list, and
//! filesystem paths — and ranks them with the local `assist` model. The view
//! owns rendering (ghost text / popup) and key handling; this module is the
//! candidate/ghost logic plus the per-pane state.

use std::path::Path;

/// Which suggestion behaviors and sources are on, mirrored from `Options`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct SuggestConfig {
    pub ghost: bool,
    pub popup: bool,
    pub tab: bool,
    pub ai: bool,
    pub history: bool,
    pub commands: bool,
    pub paths: bool,
    pub assist: bool,
}

impl SuggestConfig {
    pub fn from_opts(o: &config::Options) -> Self {
        Self {
            ghost: o.autosuggest_ghost,
            popup: o.autosuggest_popup,
            tab: o.autosuggest_tab,
            ai: o.autosuggest_ai,
            history: o.autosuggest_history,
            commands: o.autosuggest_commands,
            paths: o.autosuggest_paths,
            assist: o.autosuggest_assist,
        }
    }

    /// Any behavior that needs local candidates computed.
    pub fn wants_local(&self) -> bool {
        self.ghost || self.popup || self.tab
    }
}

/// Per-pane suggestion state.
#[derive(Default)]
pub struct Suggest {
    /// The input line the current candidates were computed for.
    pub input: String,
    /// Ranked completion candidates (full lines) for the popup / tab-cycle.
    pub candidates: Vec<String>,
    /// The accepted-suffix ghost drawn past the cursor (top candidate, or AI).
    pub ghost: Option<String>,
    /// Selected row while the popup is open.
    pub popup_open: bool,
    pub popup_sel: usize,
    /// Tab-cycle position through `candidates`, if cycling.
    pub tab_idx: Option<usize>,
    /// The input an AI suggestion is being/was fetched for, and its result, so
    /// a stale reply for an old line is ignored.
    pub ai_for: Option<String>,
    pub ai_ghost: Option<String>,
    /// Inputs beyond `input` that determine the cached candidate set.
    pub cached_cwd: Option<std::path::PathBuf>,
    pub cached_history_generation: u64,
    pub cached_at_end: bool,
    pub cached_config: Option<SuggestConfig>,
    /// Invalidates completion work that finishes after the input changed.
    pub request_generation: u64,
}

/// A short built-in list of common commands, as first-token completions.
const COMMON: &[&str] = &[
    "cd ",
    "ls",
    "ls -la",
    "clear",
    "exit",
    "cat ",
    "less ",
    "tail -f ",
    "grep -r ",
    "find . -name ",
    "git status",
    "git add ",
    "git commit -m ",
    "git push",
    "git pull",
    "git log",
    "git diff",
    "git checkout ",
    "git branch",
    "git stash",
    "cargo build",
    "cargo build --release",
    "cargo test",
    "cargo run",
    "cargo clippy --all-targets",
    "cargo fmt",
    "npm install",
    "npm run ",
    "make",
    "docker ps",
    "docker compose up",
    "kubectl get pods",
    "ssh ",
    "curl ",
];

/// Assemble and rank completion candidates for `input` from the enabled sources.
/// `history` is newest-first. `cwd` enables path completion.
pub fn candidates(
    cfg: &SuggestConfig,
    input: &str,
    history: &[String],
    cwd: Option<&Path>,
) -> Vec<String> {
    if input.trim().is_empty() {
        return Vec::new();
    }
    let mut pool: Vec<String> = Vec::new();
    if cfg.history {
        pool.extend(history.iter().cloned());
    }
    if cfg.commands {
        pool.extend(COMMON.iter().map(|s| s.to_string()));
    }
    if cfg.paths {
        pool.extend(path_candidates(input, cwd));
    }
    if cfg.assist {
        assist::suggest::rank(input, &pool, 50)
    } else {
        assist::suggest::complete(input, &pool, 50)
    }
}

/// The ghost suffix (past the cursor) for `input`, or `None`.
pub fn ghost(candidates: &[String], input: &str) -> Option<String> {
    assist::suggest::ghost(input, candidates)
}

/// Path completions: complete the last whitespace token of `input` against the
/// filesystem. Returns full input lines with the token completed. Directories
/// gain a trailing `/`. Empty when there's no cwd or nothing matches.
fn path_candidates(input: &str, cwd: Option<&Path>) -> Vec<String> {
    let cwd = match cwd {
        Some(c) => c,
        None => return Vec::new(),
    };
    let token_start = input.rfind(char::is_whitespace).map(|i| i + 1).unwrap_or(0);
    let head = &input[..token_start];
    let token = &input[token_start..];
    if token.is_empty() {
        return Vec::new();
    }
    // Split the token into a directory part and the fragment being completed.
    let (dir_part, frag) = match token.rfind('/') {
        Some(i) => (&token[..=i], &token[i + 1..]),
        None => ("", token),
    };
    let base = if dir_part.starts_with('/') {
        std::path::PathBuf::from(dir_part)
    } else if let Some(rest) = dir_part.strip_prefix("~/") {
        match dirs_home() {
            Some(h) => h.join(rest),
            None => return Vec::new(),
        }
    } else {
        cwd.join(dir_part)
    };
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(&base) else {
        return Vec::new();
    };
    for e in entries.flatten() {
        let name = e.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with(frag) || name.starts_with('.') && !frag.starts_with('.') {
            continue;
        }
        let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
        let suffix = if is_dir { "/" } else { "" };
        out.push(format!("{head}{dir_part}{name}{suffix}"));
        if out.len() >= 40 {
            break;
        }
    }
    out.sort();
    out
}

fn dirs_home() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME").map(std::path::PathBuf::from)
}

#[cfg(test)]
#[path = "../tests/suggest.rs"]
mod tests;
