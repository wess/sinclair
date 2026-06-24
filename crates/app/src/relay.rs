//! Manages the bundled `relay` sidecar — the agent mesh. Prompt never runs the
//! mesh in-process; it starts/stops the bundled binary as a detached daemon and
//! launches agents into splits. Every parameter comes from settings, passed
//! explicitly on the command line (no environment variables).

use std::path::PathBuf;

/// The bundled `relay` binary: prefer a sibling of the running executable
/// (inside the app bundle / target dir), else fall back to PATH.
fn binary() -> String {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let cand = dir.join("relay");
            if cand.exists() {
                return cand.to_string_lossy().into_owned();
            }
        }
    }
    "relay".to_string()
}

/// Fixed state directory for the mesh, beside the config file, so every relay
/// call shares one mesh regardless of the calling pane's working directory.
fn home() -> PathBuf {
    config::default_path()
        .and_then(|p| p.parent().map(|d| d.join("relay")))
        .unwrap_or_else(|| PathBuf::from(".relay"))
}

fn home_str() -> String {
    home().to_string_lossy().into_owned()
}

fn db_str() -> String {
    home().join("relay.db").to_string_lossy().into_owned()
}

/// Run a relay subcommand without blocking the UI thread.
fn run_bg(args: Vec<String>) {
    let bin = binary();
    std::thread::spawn(move || {
        let _ = std::process::Command::new(bin).args(&args).output();
    });
}

fn start_args(opts: &config::Options) -> Vec<String> {
    vec![
        "--home".into(),
        home_str(),
        "start".into(),
        "--addr".into(),
        opts.relay_address.clone(),
        "--db".into(),
        db_str(),
    ]
}

/// Whether the Relay menu and commands should be available.
pub fn enabled(opts: &config::Options) -> bool {
    opts.ai_enabled && opts.relay_enabled
}

/// Start the daemon at app launch, only if configured to auto-start.
pub fn on_launch(opts: &config::Options) {
    if enabled(opts) && opts.relay_start_on_launch {
        let _ = std::fs::create_dir_all(home());
        run_bg(start_args(opts));
    }
}

/// Reconcile the daemon with current settings after a config reload.
pub fn on_reload(opts: &config::Options) {
    let _ = std::fs::create_dir_all(home());
    if enabled(opts) {
        run_bg(start_args(opts));
    } else {
        run_bg(vec!["--home".into(), home_str(), "stop".into()]);
    }
}

/// Shell command to stream the bus in a split.
pub fn feed_command() -> String {
    format!("\"{}\" --home \"{}\" feed --follow", binary(), home_str())
}

/// Shell command to launch an agent in a split (relay prompts for a name).
pub fn launch_command(opts: &config::Options) -> String {
    format!(
        "\"{}\" --home \"{}\" launch --agent {}",
        binary(),
        home_str(),
        opts.relay_default_agent
    )
}
