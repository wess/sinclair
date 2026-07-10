//! Writing the user's config file. Two rules keep a settings toggle from ever
//! eating the rest of the file:
//!
//! - an *existing but unreadable* config aborts the write (a transient read
//!   failure must not be treated as an empty file and drop every other key);
//! - the new contents land in a temp file in the same directory and are
//!   renamed over the target, so a crash mid-write never truncates it.

use std::path::Path;

/// Upsert one `key = value` line into the user's config file, creating it if
/// needed.
pub(crate) fn upsert(key: &str, value: &str) {
    let Some(path) = config::default_path() else {
        return;
    };
    let Some(text) = current(&path) else {
        return;
    };
    persist(&path, &config::upsert(&text, key, value));
}

/// Replace every line for a repeated `key` with the given values.
pub(crate) fn set_list(key: &str, values: &[String]) {
    let Some(path) = config::default_path() else {
        return;
    };
    let Some(text) = current(&path) else {
        return;
    };
    persist(&path, &config::set_list(&text, key, values));
}

/// The file's current contents: empty when it doesn't exist yet, `None` (write
/// refused) when it exists but can't be read.
fn current(path: &Path) -> Option<String> {
    match std::fs::read_to_string(path) {
        Ok(text) => Some(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some(String::new()),
        Err(e) => {
            eprintln!(
                "sinclair: not writing config: could not read {}: {e}",
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
