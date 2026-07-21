//! Desktop notifications, posted through whatever the platform offers that
//! carries our own app icon.
//!
//! macOS has two options and only one of them looks like us: `osascript
//! display notification` is trivial but every alert it posts wears Script
//! Editor's icon, because Script Editor is the app that posted it. The
//! UserNotifications framework attributes the alert to the running bundle, so
//! the notification shows Sinclair's icon — at the cost of needing a real
//! bundle identity, which a bare `cargo run` build doesn't have. [`mac`] tries
//! that path and falls back to `osascript` when it can't.

#[cfg(target_os = "macos")]
mod mac;

/// Post a notification without blocking the caller.
pub fn post(title: &str, body: &str) {
    let (title, body) = (title.to_string(), body.to_string());
    std::thread::spawn(move || send(&title, &body));
}

/// Post a notification synchronously. Used by `sinclair notify`, which must
/// wait for the helper before the process exits.
pub fn send(title: &str, body: &str) {
    #[cfg(target_os = "macos")]
    {
        if mac::send(title, body) {
            return;
        }
        osascript(title, body);
    }
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("notify-send")
            .args(["--app-name=Sinclair", "--icon=sinclair", title, body])
            .output();
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let _ = (title, body);
}

/// The fallback for macOS builds without a usable bundle identity (a dev build
/// run straight from `target/`). Shows Script Editor's icon, which is the whole
/// reason it isn't the primary path.
#[cfg(target_os = "macos")]
fn osascript(title: &str, body: &str) {
    let esc = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    let script =
        format!("display notification \"{}\" with title \"{}\"", esc(body), esc(title));
    let _ = std::process::Command::new("osascript").args(["-e", &script]).output();
}
