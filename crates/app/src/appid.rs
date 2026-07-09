//! The app's runtime identity — `sinclairdev` for a dev build, `sinclair` for
//! the shipped binary.
//!
//! A dev build's executable is named `sinclairdev` (the cargo bin target),
//! while the release scripts install the same binary as `sinclair`. Deriving
//! the name from our own executable means `cargo run` (debug *or* `--release`)
//! is always `sinclairdev`, so a dev instance never clashes with an installed
//! one: it gets its own window title, its own Wayland/X11 app id, and —
//! crucially — its own single-instance socket, so the two run side by side
//! instead of one forwarding into the other.

use std::sync::OnceLock;

/// `"sinclairdev"` when running a dev build, `"sinclair"` for the shipped
/// binary.
pub fn id() -> &'static str {
    static ID: OnceLock<&'static str> = OnceLock::new();
    ID.get_or_init(|| {
        let dev = std::env::current_exe()
            .ok()
            .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
            .is_some_and(|stem| stem == "sinclairdev");
        if dev {
            "sinclairdev"
        } else {
            "sinclair"
        }
    })
}
