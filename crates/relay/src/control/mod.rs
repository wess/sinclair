//! Plain HTTP control plane used by the `relay` CLI (separate from the MCP bus).

use crate::db;
use crate::spawn;
use crate::state::App;
use axum::extract::{Query, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use std::convert::Infallible;
use std::sync::atomic::Ordering;
use std::time::Duration;

pub fn routes() -> Router<App> {
    Router::new()
        .route("/control/state", get(state))
        .route("/control/events", get(events))
        .route("/control/feed", get(feed))
        .route("/control/spawn", post(spawn_worker))
        .route("/control/stop", post(stop_worker))
        .route("/control/register", post(register))
        .route("/control/wait", post(wait))
        .route("/control/send", post(send))
}


#[derive(Deserialize)]
struct RegisterReq {
    name: String,
    #[serde(default)]
    role: String,
    #[serde(default)]
    channels: Vec<String>,
}

async fn register(State(app): State<App>, Json(r): Json<RegisterReq>) -> Json<Value> {
    let ok = db::upsert_agent(&app.db, &r.name, &r.role, "").await.is_ok();
    for ch in &r.channels {
        let _ = db::subscribe(&app.db, &r.name, ch).await;
    }
    app.bump();
    Json(json!({ "ok": ok }))
}

#[derive(Deserialize)]
struct WaitReq {
    name: String,
    #[serde(default)]
    block: bool,
}

async fn wait(State(app): State<App>, Json(r): Json<WaitReq>) -> Json<Value> {
    let msgs =
        crate::bus::await_messages(&app, &r.name, r.block, std::time::Duration::from_secs(25)).await;
    Json(json!({ "messages": msgs }))
}

#[derive(Deserialize)]
struct SendReq {
    from: String,
    #[serde(default)]
    kind: String,
    target: Option<String>,
    body: String,
}

async fn send(State(app): State<App>, Json(r): Json<SendReq>) -> Json<Value> {
    let kind = if r.kind.is_empty() { "direct" } else { &r.kind };
    let ok = crate::bus::deliver(&app, &r.from, kind, r.target.as_deref(), &r.body)
        .await
        .is_ok();
    Json(json!({ "ok": ok }))
}

async fn state(State(app): State<App>) -> Json<Value> {
    Json(snapshot(&app).await)
}

/// The `{agents, workers}` roster, shared by `/control/state` (one-shot) and
/// `/control/events` (streamed).
async fn snapshot(app: &App) -> Value {
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

    json!({ "agents": agents, "workers": workers })
}

/// Live status stream: emits the current snapshot immediately, then a fresh one
/// every time the roster or a worker changes (see `App::bump`). Keepalive pings
/// hold the connection open through quiet periods.
async fn events(State(app): State<App>) -> Response {
    let mut rx = app.events.subscribe();
    let stream = async_stream::stream! {
        let data = serde_json::to_string(&snapshot(&app).await).unwrap_or_default();
        yield Ok::<Event, Infallible>(Event::default().event("state").data(data));
        while rx.changed().await.is_ok() {
            let data = serde_json::to_string(&snapshot(&app).await).unwrap_or_default();
            yield Ok::<Event, Infallible>(Event::default().event("state").data(data));
        }
    };
    Sse::new(stream)
        .keep_alive(
            KeepAlive::new()
                .interval(Duration::from_secs(15))
                .text("keepalive"),
        )
        .into_response()
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
