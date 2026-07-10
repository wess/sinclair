//! The community plugin catalog: list and install plugins from this project's
//! own `plugins/` directory on GitHub. Browsing reads the GitHub contents API;
//! installing downloads a plugin folder's files into the user's plugin dir.
//!
//! The source repo is fixed (`wess/sinclair`) — we never fetch from an arbitrary
//! location — and plugin names are validated before they touch the filesystem.

use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

const REPO: &str = "wess/sinclair";

/// Names of plugins available in the catalog (the directories under `plugins/`),
/// sorted. Hits the GitHub contents API via `curl`.
pub fn list() -> Result<Vec<String>, String> {
    let body = fetch(&format!("https://api.github.com/repos/{REPO}/contents/plugins"))?;
    let value: Value = serde_json::from_slice(&body).map_err(|e| format!("parse catalog: {e}"))?;
    let entries = value.as_array().ok_or("unexpected catalog response")?;
    let mut names: Vec<String> = entries
        .iter()
        .filter(|e| e.get("type").and_then(Value::as_str) == Some("dir"))
        .filter_map(|e| e.get("name").and_then(Value::as_str).map(str::to_string))
        .filter(|n| valid_name(n))
        .collect();
    names.sort();
    Ok(names)
}

/// Download every file of catalog plugin `name` into
/// `~/.config/sinclair/plugins/<name>`. Only flat plugin folders are supported.
pub fn install(name: &str) -> Result<PathBuf, String> {
    if !valid_name(name) {
        return Err(format!("invalid plugin name {name:?}"));
    }
    let dir = plugin::defaultdir().ok_or("no plugin directory (set HOME/XDG_CONFIG_HOME)")?;
    let dest = dir.join(name);

    let body = fetch(&format!(
        "https://api.github.com/repos/{REPO}/contents/plugins/{name}"
    ))?;
    let value: Value = serde_json::from_slice(&body).map_err(|e| format!("parse listing: {e}"))?;
    let entries = value.as_array().ok_or("plugin is not a directory")?;

    // Download into a private temp dir in the same directory, then rename over
    // the destination: an install is all-or-nothing, leaves no partial dir, and
    // concurrent installs don't interleave files.
    let tmp = dir.join(format!(".{name}.{}.tmp", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).map_err(|e| format!("create {}: {e}", tmp.display()))?;

    let download = || -> Result<usize, String> {
        let mut wrote = 0usize;
        for entry in entries {
            if entry.get("type").and_then(Value::as_str) != Some("file") {
                continue; // flat folders only for now
            }
            let (Some(file), Some(url)) = (
                entry.get("name").and_then(Value::as_str),
                entry.get("download_url").and_then(Value::as_str),
            ) else {
                continue;
            };
            if !valid_file(file) {
                continue;
            }
            let bytes = fetch(url)?;
            std::fs::write(tmp.join(file), bytes).map_err(|e| format!("write {file}: {e}"))?;
            wrote += 1;
        }
        Ok(wrote)
    };

    match download() {
        Ok(0) => {
            let _ = std::fs::remove_dir_all(&tmp);
            Err("nothing downloaded".to_string())
        }
        Ok(_) => {
            let _ = std::fs::remove_dir_all(&dest);
            std::fs::rename(&tmp, &dest).map_err(|e| {
                let _ = std::fs::remove_dir_all(&tmp);
                format!("install {name}: {e}")
            })?;
            record_install(name, &dest);
            Ok(dest)
        }
        Err(e) => {
            let _ = std::fs::remove_dir_all(&tmp);
            Err(e)
        }
    }
}

/// Record a catalog install in `installed.toml`: the manifest's version and
/// its declared capabilities as the granted set (the install confirmation is
/// consent for what the manifest declares — capability narrowing then caps a
/// later update that declares more). Best-effort: a manifest that fails to
/// parse still gets a record under the folder name so it can be disabled.
fn record_install(name: &str, dest: &std::path::Path) {
    let manifest_path = dest.join(plugin::MANIFEST);
    let parsed = std::fs::read_to_string(&manifest_path)
        .ok()
        .and_then(|text| plugin::parse(manifest_path, &text).0);
    let (id, version, granted) = match &parsed {
        Some(p) => (p.id.clone(), p.version.clone(), p.capabilities.clone()),
        None => (name.to_string(), String::new(), Vec::new()),
    };
    let mut installed = plugin::Installed::load();
    installed.record(&id, &version, &format!("catalog:{name}"), granted);
    if let Err(e) = installed.save() {
        eprintln!("sinclair: could not record install of {name}: {e}");
    }
}

/// Drop a plugin's `installed.toml` record (after its folder is removed), so a
/// stale disabled/granted state can't silently apply to a future reinstall of
/// the same id.
fn forget_install(name: &str) {
    let mut installed = plugin::Installed::load();
    let source = format!("catalog:{name}");
    let before = installed.plugins.len();
    installed
        .plugins
        .retain(|id, entry| !(entry.source == source || (entry.source.is_empty() && id == name)));
    if installed.plugins.len() != before {
        if let Err(e) = installed.save() {
            eprintln!("sinclair: could not update installed.toml: {e}");
        }
    }
}

/// Remove an installed plugin (by folder name) from the managed plugin
/// directory. Refuses names outside that directory, so a plugin referenced
/// from config by an absolute path is never touched. A symlinked dev plugin is
/// unlinked, not followed into.
pub fn uninstall(name: &str) -> Result<(), String> {
    if !valid_name(name) {
        return Err(format!("invalid plugin name {name:?}"));
    }
    let dir = plugin::defaultdir().ok_or("no plugin directory")?;
    let dest = dir.join(name);
    let meta =
        std::fs::symlink_metadata(&dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    if meta.file_type().is_symlink() {
        std::fs::remove_file(&dest).map_err(|e| format!("uninstall {name}: {e}"))?;
    } else {
        std::fs::remove_dir_all(&dest).map_err(|e| format!("uninstall {name}: {e}"))?;
    }
    forget_install(name);
    Ok(())
}

/// Max bytes we'll accept for any single fetch (catalog listing or plugin
/// file), so a redirecting endpoint or runaway file can't OOM us.
const MAX_BYTES: &str = "8388608"; // 8 MiB

pub(crate) fn fetch(url: &str) -> Result<Vec<u8>, String> {
    // Defense in depth: only https, only over https on redirects, a size cap,
    // and `--` so a URL beginning with `-` can't be read as an option.
    if !url.starts_with("https://") {
        return Err("refusing non-https url".to_string());
    }
    let out = Command::new("curl")
        .args([
            "-sL",
            "--fail",
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
            "--max-filesize",
            MAX_BYTES,
            "-H",
            "Accept: application/vnd.github+json",
            "-H",
            "User-Agent: sinclair-terminal",
            "--",
            url,
        ])
        .output()
        .map_err(|e| format!("curl: {e}"))?;
    if !out.status.success() {
        return Err(format!("fetch failed ({})", out.status));
    }
    Ok(out.stdout)
}

/// A plugin folder name: the manifest id charset, no path separators or dots.
fn valid_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// A downloadable file name: a plain file, never a path, traversal, or dotfile.
fn valid_file(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && !s.contains('/')
        && !s.contains('\\')
        && !s.starts_with('.')
}

#[cfg(test)]
#[path = "../tests/catalog.rs"]
mod tests;
