//! Bind mounts and named volumes attached to a container.
//!
//! Both share one engine flag (`-v source:target[:ro]`); a `source` that looks
//! like a path is a bind mount, anything else names a volume. The distinction
//! matters to the engine, not to us, so one type covers both.

/// One `-v` entry: where it comes from on the host (or which named volume) and
/// where it lands inside the container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mount {
    /// Host path (absolute) or the name of an engine-managed volume.
    pub source: String,
    /// Absolute path inside the container.
    pub target: String,
    pub readonly: bool,
}

impl Mount {
    /// A read-write mount.
    pub fn rw(source: impl Into<String>, target: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            target: target.into(),
            readonly: false,
        }
    }

    /// A read-only mount, for anything the container should see but not edit.
    pub fn ro(source: impl Into<String>, target: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            target: target.into(),
            readonly: true,
        }
    }

    /// Mount a host path at the *same* absolute path inside the container.
    ///
    /// This is what keeps git worktrees working from both sides: a worktree
    /// records absolute paths in `.git`, so a pointer written inside the
    /// container only resolves on the host when the two agree. With an identity
    /// mount they always do, and no path translation layer is needed anywhere.
    pub fn identity(path: impl Into<String>) -> Self {
        let path = path.into();
        Self {
            source: path.clone(),
            target: path,
            readonly: false,
        }
    }

    /// True when this mount lands at the same path it came from.
    pub fn is_identity(&self) -> bool {
        self.source == self.target
    }

    /// The value for the engine's `-v` flag.
    pub fn arg(&self) -> String {
        if self.readonly {
            format!("{}:{}:ro", self.source, self.target)
        } else {
            format!("{}:{}", self.source, self.target)
        }
    }

    /// Parse a config entry: `source:target[:ro]`. A lone path is mounted at
    /// itself (identity), which is the form users want most of the time.
    ///
    /// Windows-style drive letters are not supported; sandboxes are Unix-only.
    pub fn parse(raw: &str) -> Result<Self, String> {
        let raw = raw.trim();
        if raw.is_empty() {
            return Err("empty mount".to_string());
        }
        let parts: Vec<&str> = raw.split(':').map(str::trim).collect();
        match parts.as_slice() {
            [one] => Ok(Self::identity(*one)),
            [source, target] => Self::checked(source, target, false),
            [source, target, mode] => match mode.to_ascii_lowercase().as_str() {
                "ro" => Self::checked(source, target, true),
                "rw" => Self::checked(source, target, false),
                other => Err(format!("unknown mount mode `{other}` (ro|rw)")),
            },
            _ => Err(format!(
                "`{raw}` has too many `:` parts (source:target[:ro])"
            )),
        }
    }

    /// Parse a `devcontainer.json` `mounts` entry, which uses the engine's
    /// `--mount` syntax (`type=bind,source=…,target=…`) rather than the
    /// `-v` shorthand [`Self::parse`] takes. Keys may appear in any order;
    /// anything else in the entry (`consistency`, `type`) is ignored, since it
    /// does not change where the mount lands.
    pub fn parse_mount_spec(raw: &str) -> Result<Self, String> {
        let (mut source, mut target, mut readonly) = (None, None, false);
        for field in raw.split(',') {
            let field = field.trim();
            if field.eq_ignore_ascii_case("readonly") || field.eq_ignore_ascii_case("ro") {
                readonly = true;
                continue;
            }
            let Some((key, value)) = field.split_once('=') else {
                continue;
            };
            match key.trim().to_ascii_lowercase().as_str() {
                "source" | "src" => source = Some(value.trim()),
                "target" | "destination" | "dst" => target = Some(value.trim()),
                "readonly" | "ro" => readonly = !matches!(value.trim(), "false" | "0"),
                _ => {}
            }
        }
        match (source, target) {
            (Some(s), Some(t)) => Self::checked(s, t, readonly),
            (None, _) => Err(format!("`{raw}` has no source")),
            (_, None) => Err(format!("`{raw}` has no target")),
        }
    }

    fn checked(source: &str, target: &str, readonly: bool) -> Result<Self, String> {
        if source.is_empty() {
            return Err("mount is missing a source".to_string());
        }
        if target.is_empty() {
            return Err("mount is missing a target path".to_string());
        }
        if !target.starts_with('/') {
            return Err(format!("mount target `{target}` must be absolute"));
        }
        Ok(Self {
            source: source.to_string(),
            target: target.to_string(),
            readonly,
        })
    }
}

#[cfg(test)]
#[path = "../tests/mount.rs"]
mod tests;
