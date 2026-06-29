//! Live Relay status, fed by a `relay watch` subprocess. The child streams
//! newline-delimited JSON snapshots (and a `null` line on disconnect); a reader
//! thread forwards them through a `futures` channel — the same shape as
//! `bridge.rs` — and a gpui task publishes them to the [`RelayStatus`] global so
//! every window's Relay sidebar panel updates live. Started once when Relay is
//! enabled; the child reconnects on its own across server restarts.

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};

use futures::StreamExt;
use gpui::App;
use serde_json::Value;

use crate::root::{AgentConn, RelayStatus, WorkerConn};

/// Marker so the watcher is spawned at most once per process.
struct Started;
impl gpui::Global for Started {}

/// Spawn the `relay watch` subprocess (once) and pipe its snapshots into the
/// [`RelayStatus`] global. No-op when Relay is disabled or already running.
pub fn start(opts: &config::Options, cx: &mut App) {
    if !crate::relay::enabled(opts) || cx.try_global::<Started>().is_some() {
        return;
    }
    let mut child = match Command::new(crate::relay::binary())
        .arg("watch")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return,
    };
    let Some(stdout) = child.stdout.take() else {
        return;
    };
    cx.set_global(Started);

    let (tx, mut rx) = futures::channel::mpsc::unbounded::<RelayStatus>();
    std::thread::Builder::new()
        .name("relaywatch".to_string())
        .spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                let Ok(line) = line else { break };
                if tx.unbounded_send(parse(&line)).is_err() {
                    break;
                }
            }
            // Hold the child until its stdout ends, then reap it.
            let _ = child.wait();
        })
        .ok();

    cx.spawn(async move |cx| {
        // Ends when the subprocess stdout closes and the sender drops.
        while let Some(status) = rx.next().await {
            cx.update(|cx| {
                cx.set_global(status);
                cx.refresh_windows();
            });
        }
    })
    .detach();
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
                    channels: x["channels"].as_i64().unwrap_or(0),
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
