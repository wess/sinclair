//! Capture the release date so the About panel can show when this version was
//! cut. Releases are built from the `vX.Y.Z` tag (see the release workflow), so
//! the committed date of `HEAD` is the release date. Emits `PROMPT_RELEASE_DATE`
//! (`YYYY-MM-DD`) into the crate's compile environment, read with `env!` in
//! `about.rs`. Falls back to "unknown" outside a git checkout (e.g. a source
//! tarball).

use std::process::Command;

fn main() {
    // Re-run when the checked-out commit changes so the date stays current.
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/packed-refs");

    let date = Command::new("git")
        .args(["log", "-1", "--format=%cs"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string());

    println!("cargo:rustc-env=PROMPT_RELEASE_DATE={date}");
}
