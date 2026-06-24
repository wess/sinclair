//! Plain HTTP control plane used by the `relay` CLI (separate from the MCP bus).

use crate::db;
use crate::spawn;
use crate::state::App;
use axum::extract::{Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::atomic::Ordering;

pub fn routes() -> Router<App> {
    Router::new()
        .route("/control/state", get(state))
        .route("/control/feed", get(feed))
        .route("/control/spawn", post(spawn_worker))
        .route("/control/stop", post(stop_worker))
}

async fn state(State(app): State<App>) -> Json<Value> {
    let agents = db::list_agents(&app.db)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|(name, role, online, chans)| {
            json!({ "name": name, "role": role, "online": online, "channels": chans })
        })
        .collect::<Vec<_>>();

    let mut workers = Vec::new();
    for w in app.workers.lock().await.values() {
        workers.push(json!({
            "name": w.name,
            "role": w.role,
            "status": w.status.lock().await.clone(),
            "pid": w.pid.load(Ordering::SeqCst),
            "restarts": w.restarts.load(Ordering::SeqCst),
            "log": w.log,
        }));
    }

    Json(json!({ "agents": agents, "workers": workers }))
}

#[derive(Deserialize)]
struct Since {
    #[serde(default)]
    since: i64,
}

async fn feed(State(app): State<App>, Query(q): Query<Since>) -> Json<Value> {
    let msgs = db::since(&app.db, q.since, 50).await.unwrap_or_default();
    let last = msgs.last().map(|m| m.id).unwrap_or(q.since);
    Json(json!({ "messages": msgs, "last": last }))
}

#[derive(Deserialize)]
struct SpawnReq {
    name: String,
    #[serde(default = "worker_role")]
    role: String,
    program: String,
    #[serde(default)]
    args: Vec<String>,
    cwd: String,
    #[serde(default = "yes")]
    keep_alive: bool,
}

fn worker_role() -> String {
    "worker".into()
}
fn yes() -> bool {
    true
}

async fn spawn_worker(State(app): State<App>, Json(req): Json<SpawnReq>) -> Json<Value> {
    let spec = spawn::Spec {
        name: req.name,
        role: req.role,
        program: req.program,
        args: req.args,
        cwd: req.cwd,
        keep_alive: req.keep_alive,
    };
    match spawn::launch(&app, spec).await {
        Ok(log) => Json(json!({ "ok": true, "log": log })),
        Err(e) => Json(json!({ "ok": false, "error": e.to_string() })),
    }
}

#[derive(Deserialize)]
struct StopReq {
    name: String,
}

async fn stop_worker(State(app): State<App>, Json(req): Json<StopReq>) -> Json<Value> {
    let ok = spawn::stop(&app, &req.name).await;
    Json(json!({ "ok": ok }))
}
