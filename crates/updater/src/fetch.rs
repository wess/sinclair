//! HTTPS fetching via `curl`, shared by the release check and asset downloads.
//! Defense in depth: only https (on redirects too), a size cap on in-memory
//! bodies, and `--` so a URL beginning with `-` can't be read as an option.

use std::path::Path;
use std::process::Command;

/// Cap for in-memory bodies (the GitHub release JSON): 8 MiB.
const MAX_BYTES: &str = "8388608";

/// Fetch `url` into memory (small JSON bodies only — capped at [`MAX_BYTES`]).
pub(crate) fn bytes(url: &str) -> Result<Vec<u8>, String> {
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
    if out.status.success() {
        Ok(out.stdout)
    } else {
        Err(format!("fetch failed ({})", out.status))
    }
}

/// Download `url` to `dest` (streamed, so large assets are fine).
pub(crate) fn file(url: &str, dest: &Path) -> Result<(), String> {
    if !url.starts_with("https://") {
        return Err("refusing non-https download".to_string());
    }
    let out = Command::new("curl")
        .args(["-sL", "--fail", "--proto", "=https", "--proto-redir", "=https", "-o"])
        .arg(dest)
        .args(["-H", "User-Agent: sinclair-terminal", "--", url])
        .output()
        .map_err(|e| format!("curl: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!("download failed ({})", out.status))
    }
}
