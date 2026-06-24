use crate::state::App;
use anyhow::{bail, Result};
use std::fs;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use tokio::process::Command;

const MAX_RESTARTS: u32 = 20;

/// A generic command to run and monitor as a background worker.
#[derive(Clone)]
pub struct Spec {
    pub name: String,
    pub role: String,
    pub program: String,
    pub args: Vec<String>,
    pub cwd: String,
    pub keep_alive: bool,
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

    tokio::spawn(monitor(spec, worker));
    Ok(log_path)
}

async fn monitor(spec: Spec, worker: Worker) {
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
                return;
            }
        };
        let errlog = match log.try_clone() {
            Ok(f) => f,
            Err(e) => {
                *worker.status.lock().await = format!("log clone failed: {e}");
                return;
            }
        };

        let mut cmd = Command::new(&spec.program);
        cmd.args(&spec.args)
            .current_dir(&spec.cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(errlog));

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                *worker.status.lock().await =
                    format!("spawn failed: {e} (is `{}` on PATH?)", spec.program);
                return;
            }
        };
        worker.pid.store(child.id().unwrap_or(0), Ordering::SeqCst);
        *worker.status.lock().await = "running".into();

        let exit = child.wait().await;

        if worker.stop.load(Ordering::SeqCst) {
            *worker.status.lock().await = "stopped".into();
            break;
        }
        let code = exit.ok().and_then(|s| s.code()).unwrap_or(-1);
        *worker.status.lock().await = format!("exited({code})");

        if !spec.keep_alive {
            break;
        }
        let n = worker.restarts.fetch_add(1, Ordering::SeqCst) + 1;
        if n > MAX_RESTARTS {
            *worker.status.lock().await = format!("gave up after {MAX_RESTARTS} restarts");
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
        unsafe {
            libc::kill(pid as i32, libc::SIGTERM);
        }
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
