//! Editing the config file in place. The settings UI writes single
//! `key = value` lines back; the file stays the source of truth and the
//! live-reload watcher picks the change up.

/// Return `text` with `key` set to `value`: the first non-comment line for
/// `key` is replaced in place (preserving every other line and comment),
/// or a new line is appended when the key is absent.
pub fn upsert(text: &str, key: &str, value: &str) -> String {
    let mut out = String::with_capacity(text.len() + key.len() + value.len() + 4);
    let mut replaced = false;
    for line in text.lines() {
        if !replaced && line_key(line) == Some(key) {
            out.push_str(&format!("{key} = {value}"));
            replaced = true;
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    if !replaced {
        out.push_str(&format!("{key} = {value}\n"));
    }
    out
}

/// Return `text` with every line for `key` replaced by one `key = value`
/// line per entry, in order. The block lands where the first existing entry
/// was (preserving surrounding lines and comments), or is appended when the
/// key is absent. An empty `values` removes every line for `key`.
pub fn set_list(text: &str, key: &str, values: &[String]) -> String {
    let mut out = String::with_capacity(text.len() + 16);
    let mut emitted = false;
    for line in text.lines() {
        if line_key(line) == Some(key) {
            if !emitted {
                for v in values {
                    out.push_str(&format!("{key} = {v}\n"));
                }
                emitted = true;
            }
            // Drop the original line; the block above replaces all of them.
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    if !emitted {
        for v in values {
            out.push_str(&format!("{key} = {v}\n"));
        }
    }
    out
}

/// The key of a `key = value` line, or `None` for comments/blanks/malformed.
fn line_key(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    let (key, _) = line.split_once('=')?;
    Some(key.trim())
}

#[cfg(test)]
#[path = "../tests/edit.rs"]
mod tests;
