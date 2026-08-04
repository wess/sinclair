//! Capture what the About panel says about this build: the date, and whether it
//! is a release at all. Emits `SINCLAIR_BUILD_DATE` (`YYYY-MM-DD`) and
//! `SINCLAIR_RELEASED` (`1`/`0`) into the crate's compile environment, read with
//! `env!` in `about.rs`.
//!
//! A build only counts as a release when `HEAD` *is* this version's tag, which
//! is what the release workflow checks out. Anything else — a dev build, a local
//! release build, a build from `main` after the version bump — is some later
//! commit whose date has nothing to do with when the version shipped. Claiming
//! one anyway is how the panel came to report a version released weeks earlier
//! as having been released today.

use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8(out.stdout).ok()?.trim().to_string();
    (!text.is_empty()).then_some(text)
}

fn main() {
    // Re-run when the checked-out commit or the tags move, since both decide
    // what this prints.
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/packed-refs");
    println!("cargo:rerun-if-changed=../../.git/refs/tags");

    let date = git(&["log", "-1", "--format=%cs"]).unwrap_or_else(|| "unknown".to_string());

    // `--exact-match` fails unless HEAD is itself tagged, so this cannot be
    // fooled by a later commit that merely descends from the release. A missing
    // tag (a shallow clone, tags never fetched) reads as "not a release", which
    // is the safe way to be wrong: it understates rather than inventing a date.
    let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_default();
    let released = git(&["describe", "--exact-match", "--tags", "HEAD"])
        .is_some_and(|tag| tag == format!("v{version}"));

    println!("cargo:rustc-env=SINCLAIR_BUILD_DATE={date}");
    println!("cargo:rustc-env=SINCLAIR_RELEASED={}", u8::from(released));
}
