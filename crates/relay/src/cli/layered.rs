//! The layered TOML store roles and teams share: project → user → built-in
//! resolution, name validation, and the `$EDITOR` round-trip. The project
//! layer is a repo's `./.relay/<kind>`: for CLI commands that is the ambient
//! working directory (the repo the user is standing in), while launch
//! pipelines pass the worker's own root through [`project_dir_in`] — the
//! daemon's cwd is meaningless (a Finder-launched app leaves it at `/`), so it
//! must never resolve against it.

use anyhow::{bail, Result};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Project,
    User,
    Builtin,
}

impl Source {
    pub fn label(self) -> &'static str {
        match self {
            Source::Project => "project",
            Source::User => "user",
            Source::Builtin => "built-in",
        }
    }
}

/// Names stay lowercase so the on-disk layout is predictable across layers.
pub fn valid(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'.')
}

/// Project layer in the ambient working directory: `./.relay/<kind>` — the
/// repo the caller is standing in. CLI use only; the daemon passes an explicit
/// root (see [`project_dir_in`]).
pub fn project_dir(kind: &str) -> PathBuf {
    PathBuf::from(".relay").join(kind)
}

/// Project layer under an explicit root (a worker's cwd): `<root>/.relay/<kind>`.
pub fn project_dir_in(root: &Path, kind: &str) -> PathBuf {
    root.join(".relay").join(kind)
}

/// User layer: `$XDG_CONFIG_HOME/relay/<kind>` (or `~/.config/relay/<kind>`).
pub fn user_dir(kind: &str) -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("relay").join(kind)
}

pub fn dir(kind: &str, user: bool) -> PathBuf {
    if user {
        user_dir(kind)
    } else {
        project_dir(kind)
    }
}

pub fn file_in(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("{name}.toml"))
}

pub fn builtin(table: &[(&str, &'static str)], name: &str) -> Option<&'static str> {
    table.iter().find(|(n, _)| *n == name).map(|(_, t)| *t)
}

/// Every name across the three layers; the highest layer wins the label.
pub fn scan(kind: &str, builtins: &[(&str, &str)]) -> BTreeMap<String, Source> {
    let mut seen: BTreeMap<String, Source> = BTreeMap::new();
    for (n, _) in builtins {
        seen.insert((*n).to_string(), Source::Builtin);
    }
    for (d, src) in [
        (user_dir(kind), Source::User),
        (project_dir(kind), Source::Project),
    ] {
        if let Ok(entries) = std::fs::read_dir(&d) {
            for e in entries.flatten() {
                let path = e.path();
                if path.extension().and_then(|x| x.to_str()) == Some("toml") {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        seen.insert(stem.to_string(), src);
                    }
                }
            }
        }
    }
    seen
}

/// Editor command: $VISUAL, then $EDITOR, then `vi`.
fn editor() -> String {
    for var in ["VISUAL", "EDITOR"] {
        if let Ok(v) = std::env::var(var) {
            if !v.trim().is_empty() {
                return v;
            }
        }
    }
    "vi".to_string()
}

/// Open `$EDITOR` on a temp seed, run `check(name, text)` on the result, and
/// only then move it into place — a draft that fails to parse is left behind
/// for another attempt instead of clobbering the target.
pub fn open_editor(
    target: &Path,
    seed: String,
    check: &dyn Fn(&str, &str) -> Result<()>,
) -> Result<()> {
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = target.with_extension("toml.tmp");
    std::fs::write(&tmp, seed)?;

    let status = std::process::Command::new(editor()).arg(&tmp).status()?;
    if !status.success() {
        let _ = std::fs::remove_file(&tmp);
        bail!("editor exited without saving");
    }

    let text = std::fs::read_to_string(&tmp)?;
    let name = target.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    if let Err(e) = check(name, &text) {
        bail!("{e}\nleft your draft at {}", tmp.display());
    }
    std::fs::rename(&tmp, target)?;
    println!("saved {}", target.display());
    Ok(())
}
