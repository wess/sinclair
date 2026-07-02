//! Fix the process `PATH` at startup.
//!
//! A GUI-launched app (Finder / Dock on macOS, a `.desktop` launcher on Linux)
//! inherits a bare `PATH` — `/usr/bin:/bin:/usr/sbin:/sbin` — not the user's
//! login-shell PATH. So anything the app spawns can't find tools installed for
//! the shell: plugin runtimes (`bun`, `node`), agent-tool probes, the relay
//! mesh, or plugin commands like `git`/`docker`. Terminal panes are unaffected
//! (they're login shells that source the profile themselves).
//!
//! This queries the login shell once at startup and adopts its `PATH`, unless
//! the current PATH already looks like the user's (launched from a terminal).

use std::process::Command;

/// Adopt the login shell's `PATH` when the inherited one looks like a bare
/// GUI-launch PATH. Call once, early in `main`, before spawning anything.
pub fn fix() {
    let current = std::env::var("PATH").unwrap_or_default();
    if looks_inherited(&current) {
        return;
    }
    if let Some(path) = login_path() {
        if !path.is_empty() && path != current {
            // Safe: called at startup before any other thread reads the env.
            std::env::set_var("PATH", path);
        }
    }
}

/// Heuristic: a PATH that contains any directory under `$HOME` (e.g. `~/.bun/bin`,
/// `~/.cargo/bin`, `~/.local/bin`, asdf/nvm shims) came from the user's shell,
/// so we leave it alone. A bare GUI PATH has none.
fn looks_inherited(path: &str) -> bool {
    let home = std::env::var("HOME").unwrap_or_default();
    !home.is_empty() && path.split(':').any(|p| p.starts_with(&home))
}

/// Query the login shell (`-lic`, so both profile and rc files apply) for its
/// `$PATH`, extracted between markers so shell-startup noise can't corrupt it.
fn login_path() -> Option<String> {
    let shell = pty::default_shell();
    let out = Command::new(&shell)
        .args(["-lic", "printf '__PROMPTPATH__%s__PROMPTPATH__' \"$PATH\""])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let mut parts = text.split("__PROMPTPATH__");
    parts.next(); // startup noise before the first marker
    let path = parts.next()?.trim().to_string();
    (!path.is_empty()).then_some(path)
}
