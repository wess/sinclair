//! Writing the user's settings.json. Edits go through `config::jsonedit` so
//! the user's comments and formatting survive every GUI write. Two rules
//! keep a settings toggle from ever eating the rest of the file:
//!
//! - an *existing but unreadable* file aborts the write (a transient read
//!   failure must not be treated as an empty file and drop every other key),
//!   and so does text jsonedit can't safely edit;
//! - the new contents land in a temp file in the same directory and are
//!   renamed over the target, so a crash mid-write never truncates it.

use std::path::{Path, PathBuf};

/// Set `key` to the scalar `value`, encoded per the key's kind (booleans and
/// numbers land unquoted). An empty value removes the key — back to the
/// built-in default, matching what the empty string meant in the legacy
/// format.
pub(crate) fn upsert(key: &str, value: &str) {
    if value.is_empty() {
        return remove(key);
    }
    edit(key, |text| {
        config::jsonedit::upsert(text, key, &config::settings::encode(key, value))
    });
}

/// Replace a repeated `key` with the given entries; empty removes the key.
pub(crate) fn set_list(key: &str, values: &[String]) {
    if values.is_empty() {
        return remove(key);
    }
    edit(key, |text| {
        config::jsonedit::upsert(text, key, &config::settings::encode_list(values))
    });
}

/// Remove `key` from the file, restoring the built-in default.
pub(crate) fn remove(key: &str) {
    edit(key, |text| config::jsonedit::remove(text, key));
}

fn edit(key: &str, apply: impl Fn(&str) -> Option<String>) {
    let Some(path) = config::default_path() else {
        return;
    };
    migrate();
    let Some(text) = current(&path) else {
        return;
    };
    let text = if text.trim().is_empty() {
        config::settings::starter()
    } else {
        text
    };
    match apply(&text) {
        Some(updated) => persist(&path, &updated),
        None => eprintln!(
            "sinclair: not writing `{key}`: {} is not a JSON object (fix it or delete it)",
            path.display()
        ),
    }
}

/// One-time upgrade: when no settings.json exists but a legacy `key = value`
/// config does, convert it. The old file stays in place (no longer read), so
/// nothing is lost if the user rolls back.
pub(crate) fn migrate() {
    let Some(path) = config::default_path() else {
        return;
    };
    if path.exists() {
        return;
    }
    let Some(legacy) = config::legacy_path().filter(|p| p.exists()) else {
        return;
    };
    let Ok(text) = std::fs::read_to_string(&legacy) else {
        return;
    };
    persist(&path, &config::settings::from_legacy(&text));
    eprintln!(
        "sinclair: migrated {} to {}",
        legacy.display(),
        path.display()
    );
}

/// Make sure settings.json exists (migrating or writing the starter file if
/// needed) and return its path, for opening in an editor.
pub(crate) fn ensure_settings_file() -> Option<PathBuf> {
    let path = config::default_path()?;
    migrate();
    if !path.exists() {
        persist(&path, &config::settings::starter());
    }
    Some(path)
}

/// The file's current contents: empty when it doesn't exist yet, `None` (write
/// refused) when it exists but can't be read.
fn current(path: &Path) -> Option<String> {
    match std::fs::read_to_string(path) {
        Ok(text) => Some(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some(String::new()),
        Err(e) => {
            eprintln!(
                "sinclair: not writing settings: could not read {}: {e}",
                path.display()
            );
            None
        }
    }
}

/// Write `contents` via a temp file + rename in the same directory.
fn persist(path: &Path, contents: &str) {
    let Some(dir) = path.parent() else {
        return;
    };
    let _ = std::fs::create_dir_all(dir);
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
    let Some(name) = name else {
        return;
    };
    let tmp = dir.join(format!(".{name}.{}.tmp", std::process::id()));
    if let Err(e) = std::fs::write(&tmp, contents) {
        eprintln!("sinclair: could not write {}: {e}", tmp.display());
        return;
    }
    if let Err(e) = std::fs::rename(&tmp, path) {
        eprintln!("sinclair: could not update {}: {e}", path.display());
        let _ = std::fs::remove_file(&tmp);
    }
}

#[cfg(test)]
#[path = "../tests/confwrite.rs"]
mod tests;
