//! HTTPS fetching via `curl`, shared by the release check and asset downloads.
//! Defense in depth: only https (on redirects too), a size cap on in-memory
//! bodies, and `--` so a URL beginning with `-` can't be read as an option.

use std::path::Path;
use std::process::Command;
use std::time::Duration;

/// Cap for in-memory bodies (the GitHub release JSON): 8 MiB.
const MAX_BYTES: &str = "8388608";

/// How often to sample the partial file while a download runs. Fast enough for
/// a progress bar that looks live, slow enough to be free.
const SAMPLE: Duration = Duration::from_millis(120);

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

/// Download `url` to `dest`, reporting bytes written as they land.
///
/// `curl` runs as a child rather than a blocking `output()` call so the partial
/// file can be sampled while it writes — GitHub already told us the asset's
/// total size, so polling the file length is all a real progress bar needs and
/// it beats parsing curl's progress meter out of stderr.
///
/// `expected` is that size (0 when unknown). A finished download that doesn't
/// match it is reported as a failure: a truncated `.dmg` fails to mount and a
/// truncated AppImage would be renamed over a working install.
pub(crate) fn file(
    url: &str,
    dest: &Path,
    expected: u64,
    on_progress: &dyn Fn(u64),
) -> Result<(), String> {
    if !url.starts_with("https://") {
        return Err("refusing non-https download".to_string());
    }
    // Never write *through* whatever is already at `dest`: curl -o follows a
    // symlink, and for the AppImage path the staged file is renamed over the
    // running binary, so a planted link would end up being what we execute.
    let _ = std::fs::remove_file(dest);
    let mut child = Command::new("curl")
        .args([
            "-sL",
            "--fail",
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
            // No --max-time: a large asset on a slow link is legitimate. These
            // bound being *stuck* instead — no connection in 30s, or under
            // 1 KiB/s for a solid minute — so a dead network fails the install
            // rather than parking the prompt in "Updating…" forever.
            "--connect-timeout",
            "30",
            "--speed-limit",
            "1024",
            "--speed-time",
            "60",
            "-o",
        ])
        .arg(dest)
        .args(["-H", "User-Agent: sinclair-terminal", "--", url])
        .spawn()
        .map_err(|e| format!("curl: {e}"))?;

    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                on_progress(std::fs::metadata(dest).map(|m| m.len()).unwrap_or(0));
                std::thread::sleep(SAMPLE);
            }
            // Child has no Drop that reaps or kills, so bailing out here would
            // leave curl running and still writing to `dest` — and a retry
            // would then race a second curl against the first over the same
            // path. Take the process and the partial file down together.
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = std::fs::remove_file(dest);
                return Err(format!("curl: {e}"));
            }
        }
    };
    if !status.success() {
        let _ = std::fs::remove_file(dest);
        return Err(format!("download failed ({status})"));
    }

    let got = std::fs::metadata(dest).map(|m| m.len()).unwrap_or(0);
    if expected > 0 && got != expected {
        let _ = std::fs::remove_file(dest);
        return Err(format!("download is incomplete ({got} of {expected} bytes)"));
    }
    on_progress(got);
    Ok(())
}
