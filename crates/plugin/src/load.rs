//! Plugin discovery and loading.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::manifest::{self, Diagnostic, Plugin};
use crate::MANIFEST;

pub fn defaultdir() -> Option<PathBuf> {
    let current = appdir("sinclair")?;
    if !current.exists() {
        if let Some(legacy) = appdir("prompt").filter(|p| p.exists()) {
            return Some(legacy);
        }
    }
    Some(current)
}

/// The plugins directory under `app`'s per-user configuration directory.
fn appdir(app: &str) -> Option<PathBuf> {
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        if !xdg.is_empty() {
            return Some(PathBuf::from(xdg).join(app).join("plugins"));
        }
    }
    let home = std::env::var_os("HOME")?;
    if home.is_empty() {
        return None;
    }
    Some(PathBuf::from(home).join(".config").join(app).join("plugins"))
}

/// Load explicitly configured plugin directories, the user's installed plugins,
/// and the first-party plugins bundled with the app. Later sources are lower
/// priority: a user-installed plugin overrides a bundled one of the same id
/// (see the id de-duplication in [`loadmanifests`]). Duplicate manifest paths
/// are ignored.
pub fn load(paths: &[String]) -> (Vec<Plugin>, Vec<Diagnostic>) {
    let mut manifests = Vec::new();
    for path in paths {
        manifests.push(manifestpath(&expand(path)));
    }
    if let Some(dir) = defaultdir() {
        collectmanifests(&dir, &mut manifests);
    }
    for dir in bundleddirs() {
        collectmanifests(&dir, &mut manifests);
    }
    loadmanifests(manifests)
}

/// Push every `dir/<sub>/plugin.toml` under a plugin directory onto `out`.
fn collectmanifests(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let manifest = manifestpath(&path);
            if manifest.is_file() {
                out.push(manifest);
            }
        }
    }
}

/// Directories of first-party plugins shipped alongside the binary (e.g. Notes).
/// Resolved relative to the executable so it works for the macOS `.app`
/// (`Contents/Resources/plugins`), a Linux prefix install
/// (`<prefix>/share/sinclair/plugins`), and a portable layout (a `plugins` dir
/// next to the binary). In debug builds the workspace `plugins/` dir is also
/// included so a plain `cargo run` picks up first-party plugins without a bundle.
fn bundleddirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            for candidate in [
                dir.join("plugins"),
                dir.join("../Resources/plugins"),
                dir.join("../share/sinclair/plugins"),
            ] {
                if candidate.is_dir() {
                    dirs.push(candidate);
                }
            }
        }
    }
    #[cfg(debug_assertions)]
    {
        let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../plugins");
        if dev.is_dir() {
            dirs.push(dev);
        }
    }
    dirs
}

fn loadmanifests(paths: Vec<PathBuf>) -> (Vec<Plugin>, Vec<Diagnostic>) {
    let installed = crate::install::Installed::load();
    let mut seen = HashSet::new();
    let mut seenids = HashSet::new();
    let mut plugins = Vec::new();
    let mut diags = Vec::new();
    for source in paths {
        let path = manifestpath(&source);
        if !seen.insert(path.clone()) {
            continue;
        }
        match std::fs::read_to_string(&path) {
            Ok(text) => {
                let (plugin, mut local) = manifest::parse(path, &text);
                diags.append(&mut local);
                // A disabled plugin stays installed but does not load.
                if let Some(plugin) = plugin.filter(|p| installed.is_enabled(&p.id)) {
                    // First manifest for an id wins, so a user-installed or
                    // explicitly configured plugin overrides a bundled one.
                    if seenids.insert(plugin.id.clone()) {
                        plugins.push(plugin);
                    }
                }
            }
            Err(error) => diags.push(Diagnostic {
                path,
                line: 0,
                message: format!("failed to read plugin manifest: {error}"),
            }),
        }
    }
    (plugins, diags)
}

fn manifestpath(path: &Path) -> PathBuf {
    if path.file_name().is_some_and(|name| name == MANIFEST) {
        path.to_path_buf()
    } else {
        path.join(MANIFEST)
    }
}

fn expand(path: &str) -> PathBuf {
    if path == "~" {
        return home().unwrap_or_else(|| PathBuf::from(path));
    }
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = home() {
            return home.join(rest);
        }
    }
    PathBuf::from(path)
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
}

#[cfg(test)]
#[path = "../tests/load.rs"]
mod tests;
