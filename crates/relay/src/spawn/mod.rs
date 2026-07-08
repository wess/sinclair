use crate::db;
use crate::state::App;
use anyhow::{bail, Result};
use std::fs;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use tokio::process::Command;

const MAX_RESTARTS: u32 = 20;

/// Cap on concurrently-running spawned workers, across the MCP `spawn` tool and
/// the CLI `--background` path (both funnel through [`launch`]). Bounds a
/// supervisor that would otherwise start unbounded headless agents (issue #8).
const MAX_WORKERS: usize = 8;

/// A generic command to run and monitor as a background worker.
#[derive(Clone)]
pub struct Spec {
    pub name: String,
    pub role: String,
    pub program: String,
    pub args: Vec<String>,
    pub cwd: String,
    pub keep_alive: bool,
    /// A fixed claude session id (uuid) for a resumable worker: passed as
    /// `--session-id` on the first launch, then `--resume` on every respawn, so
    /// context survives a crash or a daemon restart (issue #4). `None` for
    /// non-resumable / non-claude workers.
    pub session_id: Option<String>,
    /// Resume the session from the very first attempt (a rehydrated worker whose
    /// session already exists), rather than creating it with `--session-id`.
    pub resume: bool,
}

/// A tracked headless worker. Arc fields are shared with its monitor task.
#[derive(Clone)]
pub struct Worker {
    pub name: String,
    pub role: String,
    pub log: String,
    pub cwd: String,
    pub started: i64,
    pub keep_alive: bool,
    pub stop: Arc<AtomicBool>,
    pub pid: Arc<AtomicU32>,
    pub restarts: Arc<AtomicU32>,
    pub status: Arc<tokio::sync::Mutex<String>>,
}

/// Run `spec` as a monitored background process. Returns the log path.
pub async fn launch(app: &App, spec: Spec) -> Result<String> {
    {
        let workers = app.workers.lock().await;
        if let Some(w) = workers.get(&spec.name) {
            if !w.stop.load(Ordering::SeqCst) {
                bail!("worker '{}' already exists; stop it first", spec.name);
            }
        }
        let live = workers
            .values()
            .filter(|w| !w.stop.load(Ordering::SeqCst))
            .count();
        if live >= MAX_WORKERS {
            bail!(
                "worker cap reached ({MAX_WORKERS} running); stop one with stop_worker before spawning another"
            );
        }
    }

    let dir = std::env::current_dir()?.join(".relay");
    fs::create_dir_all(&dir)?;
    let log_path = dir
        .join(format!("{}.log", spec.name))
        .to_string_lossy()
        .into_owned();

    let worker = Worker {
        name: spec.name.clone(),
        role: spec.role.clone(),
        log: log_path.clone(),
        cwd: spec.cwd.clone(),
        started: crate::protocol::now(),
        keep_alive: spec.keep_alive,
        stop: Arc::new(AtomicBool::new(false)),
        pid: Arc::new(AtomicU32::new(0)),
        restarts: Arc::new(AtomicU32::new(0)),
        status: Arc::new(tokio::sync::Mutex::new("starting".into())),
    };
    app.workers
        .lock()
        .await
        .insert(spec.name.clone(), worker.clone());
    // Persist so a restarted daemon can bring this worker back (issue #4).
    let _ = db::save_worker(
        &app.db,
        &db::PersistedWorker {
            name: spec.name.clone(),
            role: spec.role.clone(),
            program: spec.program.clone(),
            args: spec.args.clone(),
            cwd: spec.cwd.clone(),
            keep_alive: spec.keep_alive,
            session_id: spec.session_id.clone(),
        },
    )
    .await;
    app.bump();

    tokio::spawn(monitor(app.clone(), spec, worker));
    Ok(log_path)
}

async fn monitor(app: App, spec: Spec, worker: Worker) {
    loop {
        if worker.stop.load(Ordering::SeqCst) {
            break;
        }

        let log = match std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&worker.log)
        {
            Ok(f) => f,
            Err(e) => {
                *worker.status.lock().await = format!("log open failed: {e}");
                let _ = db::delete_worker(&app.db, &worker.name).await;
                app.bump();
                return;
            }
        };
        let errlog = match log.try_clone() {
            Ok(f) => f,
            Err(e) => {
                *worker.status.lock().await = format!("log clone failed: {e}");
                let _ = db::delete_worker(&app.db, &worker.name).await;
                app.bump();
                return;
            }
        };

        let mut cmd = Command::new(&spec.program);
        cmd.args(&spec.args);
        // Resumable claude worker: fix the session on the first attempt, then
        // resume it on every respawn so context survives a crash (issue #4).
        if let Some(sid) = &spec.session_id {
            let attempt = worker.restarts.load(Ordering::SeqCst);
            if spec.resume || attempt > 0 {
                cmd.arg("--resume").arg(sid);
            } else {
                cmd.arg("--session-id").arg(sid);
            }
        }
        cmd.current_dir(&spec.cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(errlog));

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                *worker.status.lock().await =
                    format!("spawn failed: {e} (is `{}` on PATH?)", spec.program);
                let _ = db::delete_worker(&app.db, &worker.name).await;
                app.bump();
                return;
            }
        };
        let pid = child.id().unwrap_or(0);
        worker.pid.store(pid, Ordering::SeqCst);
        crate::cli::paths::record_worker_pid(pid);
        *worker.status.lock().await = "running".into();
        app.bump();

        let exit = child.wait().await;
        crate::cli::paths::forget_worker_pid(pid);

        if worker.stop.load(Ordering::SeqCst) {
            *worker.status.lock().await = "stopped".into();
            app.bump();
            break;
        }
        let code = exit.ok().and_then(|s| s.code()).unwrap_or(-1);
        *worker.status.lock().await = format!("exited({code})");
        app.bump();

        if !spec.keep_alive {
            // One-shot worker finished — it should not come back on restart.
            let _ = db::delete_worker(&app.db, &worker.name).await;
            break;
        }
        let n = worker.restarts.fetch_add(1, Ordering::SeqCst) + 1;
        if n > MAX_RESTARTS {
            *worker.status.lock().await = format!("gave up after {MAX_RESTARTS} restarts");
            let _ = db::delete_worker(&app.db, &worker.name).await;
            app.bump();
            break;
        }
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    }
    worker.pid.store(0, Ordering::SeqCst);
}

/// Stop a worker by name. Returns false if unknown.
pub async fn stop(app: &App, name: &str) -> bool {
    let workers = app.workers.lock().await;
    let Some(w) = workers.get(name) else {
        return false;
    };
    w.stop.store(true, Ordering::SeqCst);
    let pid = w.pid.load(Ordering::SeqCst);
    if pid != 0 {
        crate::proc::terminate(pid);
    }
    true
}

/// Stop every worker (used on server shutdown).
pub async fn stop_all(app: &App) {
    let names: Vec<String> = app.workers.lock().await.keys().cloned().collect();
    for n in names {
        stop(app, &n).await;
    }
}
