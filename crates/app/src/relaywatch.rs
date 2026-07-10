//! Live Relay status, fed by a `relay watch` subprocess. The child streams
//! newline-delimited JSON snapshots (and a `null` line on disconnect); a reader
//! thread forwards them through a `futures` channel — the same shape as
//! `bridge.rs` — and a gpui task publishes them to the [`RelayStatus`] global so
//! every window's Relay sidebar panel updates live. Started once when Relay is
//! enabled. If the child dies the thread publishes a disconnected status and
//! respawns it with bounded backoff, so the last snapshot can never stick
//! forever (and a crash-looping binary can't spin).

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use futures::StreamExt;
use gpui::App;
use serde_json::Value;

use crate::root::{AgentConn, RelayStatus, WorkerConn};

/// Marker so the watcher is spawned at most once per process.
struct Started;
impl gpui::Global for Started {}

/// Respawn backoff bounds. A watch that ran healthily for a while resets the
/// delay; a binary that dies immediately backs off up to the cap.
const BACKOFF_MIN: Duration = Duration::from_secs(2);
const BACKOFF_MAX: Duration = Duration::from_secs(60);
/// A run at least this long counts as healthy and resets the backoff.
const HEALTHY_RUN: Duration = Duration::from_secs(30);

/// Start the watcher (once): a background thread that owns the `relay watch`
/// child — spawning, reading, reaping, and respawning it — plus a gpui task
/// publishing its snapshots to the [`RelayStatus`] global. No-op when Relay is
/// disabled or already running.
pub fn start(opts: &config::Options, cx: &mut App) {
    if !crate::relay::available(opts) || cx.try_global::<Started>().is_some() {
        return;
    }
    cx.set_global(Started);

    let (tx, mut rx) = futures::channel::mpsc::unbounded::<RelayStatus>();
    std::thread::Builder::new()
        .name("relaywatch".to_string())
        .spawn(move || watch_loop(tx))
        .ok();

    cx.spawn(async move |cx| {
        let mut connected = false;
        while let Some(status) = rx.next().await {
            let flipped = status.connected != connected;
            connected = status.connected;
            cx.update(|cx| {
                cx.set_global(status);
                cx.refresh_windows();
                // The Relay menu shows the server state; rebuild menus when it
                // flips so the status line tracks start/stop without timers.
                if flipped {
                    for handle in cx.windows() {
                        if let Some(ws) = handle.downcast::<crate::root::WorkspaceView>() {
                            ws.update(cx, |view, _window, cx| view.setmenus(cx)).ok();
                        }
                    }
                }
            });
        }
    })
    .detach();
}

/// Spawn `relay watch`, stream its snapshots into `tx`, and respawn it when it
/// dies. Publishes a disconnected (default) status the moment a stream ends so
/// stale `connected: true` state can't outlive its source.
fn watch_loop(tx: futures::channel::mpsc::UnboundedSender<RelayStatus>) {
    let mut backoff = BACKOFF_MIN;
    loop {
        let started = Instant::now();
        if run_once(&tx).is_err() {
            return; // receiver gone: app shut down
        }
        if tx.unbounded_send(RelayStatus::default()).is_err() {
            return;
        }
        if started.elapsed() >= HEALTHY_RUN {
            backoff = BACKOFF_MIN;
        }
        std::thread::sleep(backoff);
        backoff = (backoff * 2).min(BACKOFF_MAX);
    }
}

/// One child lifetime: spawn, forward every line, reap. `Err` means the
/// receiver dropped (stop for good); `Ok` means the child ended (respawn).
fn run_once(tx: &futures::channel::mpsc::UnboundedSender<RelayStatus>) -> Result<(), ()> {
    let mut child = match Command::new(crate::relay::binary())
        .arg("watch")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return Ok(()),
    };
    let Some(stdout) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return Ok(());
    };
    let reader = BufReader::new(stdout);
    let mut receiver_gone = false;
    for line in reader.lines() {
        let Ok(line) = line else { break };
        if tx.unbounded_send(parse(&line)).is_err() {
            receiver_gone = true;
            break;
        }
    }
    if receiver_gone {
        let _ = child.kill();
    }
    let _ = child.wait();
    if receiver_gone {
        Err(())
    } else {
        Ok(())
    }
}

/// Parse one snapshot line. A blank/`null` line means the stream dropped, which
/// maps to a disconnected (default) status.
fn parse(line: &str) -> RelayStatus {
    let line = line.trim();
    if line.is_empty() || line == "null" {
        return RelayStatus::default();
    }
    let Ok(v) = serde_json::from_str::<Value>(line) else {
        return RelayStatus::default();
    };
    if v.is_null() {
        return RelayStatus::default();
    }
    let agents = v["agents"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|x| AgentConn {
                    name: x["name"].as_str().unwrap_or_default().to_string(),
                    role: x["role"].as_str().unwrap_or_default().to_string(),
                    online: x["online"].as_bool().unwrap_or(false),
                    registered: x["registered"].as_bool().unwrap_or(true),
                    channels: x["channels"].as_i64().unwrap_or(0),
                    last_seen: x["last_seen"].as_i64().unwrap_or(0),
                    status: x["status"].as_str().unwrap_or_default().to_string(),
                })
                .collect()
        })
        .unwrap_or_default();
    let workers = v["workers"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|x| WorkerConn {
                    name: x["name"].as_str().unwrap_or_default().to_string(),
                    role: x["role"].as_str().unwrap_or_default().to_string(),
                    status: x["status"].as_str().unwrap_or_default().to_string(),
                    restarts: x["restarts"].as_i64().unwrap_or(0),
                })
                .collect()
        })
        .unwrap_or_default();
    RelayStatus {
        connected: true,
        agents,
        workers,
    }
}
