//! OS profiles: a labelled container image plus the shell to run inside it.
//!
//! Built-ins cover the common distros; users add or override via the config
//! `container` key, one entry per line:
//!
//! ```text
//! container = Debian | debian:latest | bash
//! container = Tools  | ghcr.io/me/tools:latest | zsh | persist
//! ```
//!
//! Fields are `label | image | command | lifecycle`. Only `label` and `image`
//! are required; `command` defaults to `bash`, and `lifecycle` is `persist`
//! or `ephemeral` (omitted = follow the global default).

/// A selectable OS image and how to enter it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    /// Display name shown in the picker (e.g. `Debian`).
    pub label: String,
    /// OCI image reference (e.g. `debian:latest`).
    pub image: String,
    /// Shell/command run inside the container (e.g. `bash`).
    pub command: String,
    /// Per-profile lifecycle override: `Some(true)` keeps the container after
    /// the tab closes, `Some(false)` removes it, `None` follows the global
    /// `container-persist` default.
    pub persist: Option<bool>,
}

impl Profile {
    fn new(label: &str, image: &str, command: &str) -> Self {
        Self {
            label: label.to_string(),
            image: image.to_string(),
            command: command.to_string(),
            persist: None,
        }
    }
}

/// The built-in OS profiles, in menu order. Alpine ships only `sh`.
pub fn builtin() -> Vec<Profile> {
    vec![
        Profile::new("Debian", "debian:latest", "bash"),
        Profile::new("Ubuntu", "ubuntu:latest", "bash"),
        Profile::new("Alpine", "alpine:latest", "sh"),
        Profile::new("Fedora", "fedora:latest", "bash"),
        Profile::new("Arch Linux", "archlinux:latest", "bash"),
    ]
}

/// Parse one config `container` entry: `label | image | command | lifecycle`.
pub fn parse_profile(raw: &str) -> Result<Profile, String> {
    let mut parts = raw.split('|').map(str::trim);
    let label = parts.next().unwrap_or("").to_string();
    if label.is_empty() {
        return Err("missing label (expected `label | image | command`)".to_string());
    }
    let image = parts.next().unwrap_or("").to_string();
    if image.is_empty() {
        return Err(format!("profile `{label}` is missing an image reference"));
    }
    let command = match parts.next().map(str::trim).filter(|c| !c.is_empty()) {
        Some(c) => c.to_string(),
        None => "bash".to_string(),
    };
    let persist = match parts.next().map(str::trim).filter(|c| !c.is_empty()) {
        None => None,
        Some(s) => match s.to_ascii_lowercase().as_str() {
            "persist" | "keep" => Some(true),
            "ephemeral" | "rm" => Some(false),
            other => return Err(format!("unknown lifecycle `{other}` (persist|ephemeral)")),
        },
    };
    Ok(Profile {
        label,
        image,
        command,
        persist,
    })
}

/// Merge the built-ins with user `container` entries. A user entry whose label
/// matches a built-in (case-insensitively) replaces it in place; otherwise it
/// is appended. Bad entries are skipped and returned as error strings.
pub fn profiles(raw: &[String]) -> (Vec<Profile>, Vec<String>) {
    let mut out = builtin();
    let mut errors = Vec::new();
    for entry in raw {
        match parse_profile(entry) {
            Ok(profile) => {
                match out
                    .iter()
                    .position(|p| p.label.eq_ignore_ascii_case(&profile.label))
                {
                    Some(i) => out[i] = profile,
                    None => out.push(profile),
                }
            }
            Err(message) => errors.push(format!("`{entry}`: {message}")),
        }
    }
    (out, errors)
}

#[cfg(test)]
#[path = "../tests/profile.rs"]
mod tests;
