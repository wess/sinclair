use crate::db;
use crate::state::App;
use anyhow::{bail, Result};
use std::fs;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use tokio::process::Command;

/// Consecutive *rapid* restarts tolerated before a worker is declared broken.
/// Only failures inside [`HEALTHY_UPTIME`] count: a worker that ran normally and
/// then exited starts over with a full budget, so a long-lived agent is never
/// retired for accumulating restarts across hours of healthy work.
const MAX_RESTARTS: u32 = 20;

/// How long a worker must stay up for its run to count as healthy, clearing the
/// consecutive-failure counter. Below this it is crash-looping — usually a bad
/// argv, a missing binary, or an agent exiting immediately after one turn.
const HEALTHY_UPTIME: std::time::Duration = std::time::Duration::from_secs(60);

/// Backoff between rapid restarts, doubling from [`RESTART_BACKOFF`] up to
/// [`RESTART_BACKOFF_MAX`]. A crash-looping worker used to respawn every 3s,
/// burning its whole budget in about a minute; backing off gives a transient
/// cause (a busy DB, a rate limit) time to clear.
const RESTART_BACKOFF: std::time::Duration = std::time::Duration::from_secs(3);
const RESTART_BACKOFF_MAX: std::time::Duration = std::time::Duration::from_secs(60);

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
    /// Extra environment for the child, on top of the inherited one.
    pub env: Vec<(String, String)>,
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
    // Logs live in the state dir (`--home`), never under the daemon's own cwd —
    // a Finder-launched app leaves that at `/`, where nothing can be created.
    let dir = crate::cli::paths::abs_dir();
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
    // Check the duplicate/cap invariants and reserve the name under one lock
    // hold: two concurrent spawns of the same name must not both pass and have
    // the second insert overwrite the first worker (orphaning its monitor —
    // the stop flag becomes unreachable and it respawns to MAX_RESTARTS).
    {
        let mut workers = app.workers.lock().await;
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
        workers.insert(spec.name.clone(), worker.clone());
    }

    // Filesystem setup happens after the reservation; give the slot back on
    // failure so the name is not left claimed by a worker that never ran.
    if let Err(e) = fs::create_dir_all(&dir) {
        app.workers.lock().await.remove(&spec.name);
        return Err(e.into());
    }
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
        for (k, v) in &spec.env {
            cmd.env(k, v);
        }
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

        let started = std::time::Instant::now();
        let exit = child.wait().await;
        let uptime = started.elapsed();
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
        // A run that lasted proves the worker basically works, so its failure
        // budget resets: only consecutive *rapid* exits retire it. Without this
        // an agent that exits once per work cycle — which `claude -p` does every
        // time the model ends a turn — burned all 20 restarts within a couple of
        // minutes and was then deleted permanently.
        let n = if uptime >= HEALTHY_UPTIME {
            worker.restarts.store(0, Ordering::SeqCst);
            0
        } else {
            worker.restarts.fetch_add(1, Ordering::SeqCst) + 1
        };
        if n > MAX_RESTARTS {
            *worker.status.lock().await =
                format!("gave up after {MAX_RESTARTS} restarts in under {HEALTHY_UPTIME:?} each");
            let _ = db::delete_worker(&app.db, &worker.name).await;
            app.bump();
            break;
        }
        tokio::time::sleep(backoff(n)).await;
    }
    worker.pid.store(0, Ordering::SeqCst);
}

/// Delay before restart attempt `n`: [`RESTART_BACKOFF`] doubling per
/// consecutive rapid failure, capped at [`RESTART_BACKOFF_MAX`]. `n == 0` (the
/// worker had been up and healthy) restarts at the base delay.
fn backoff(n: u32) -> std::time::Duration {
    let shift = n.saturating_sub(1).min(16);
    RESTART_BACKOFF
        .saturating_mul(1u32 << shift)
        .min(RESTART_BACKOFF_MAX)
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

/// Stop a worker and forget its persisted row, so no future daemon resurrects
/// it. The row is deleted only when the stop was actually issued or the worker
/// is already gone from the live map (a stale row from an earlier daemon).
/// Shared by the MCP `stop_worker` tool and the `/control/stop` route so the
/// two planes keep identical semantics. Returns what [`stop`] returned.
pub async fn stop_and_forget(app: &App, name: &str) -> bool {
    let stopped = stop(app, name).await;
    let gone = !app.workers.lock().await.contains_key(name);
    if stopped || gone {
        let _ = db::delete_worker(&app.db, name).await;
    }
    stopped
}

/// Stop every worker (used on server shutdown).
pub async fn stop_all(app: &App) {
    let names: Vec<String> = app.workers.lock().await.keys().cloned().collect();
    for n in names {
        stop(app, &n).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn app() -> (App, std::path::PathBuf) {
        static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!("relay-spawn-{}-{n}.db", std::process::id()));
        let pool = db::open(path.to_str().unwrap()).await.unwrap();
        (
            App::new(pool, "http://127.0.0.1:0".into(), "t".into()),
            path,
        )
    }

    fn cleanup(path: &std::path::Path) {
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(path.with_extension("db-wal"));
        let _ = std::fs::remove_file(path.with_extension("db-shm"));
    }

    /// A live in-memory worker with no real process behind it, for exercising
    /// the reservation invariants without spawning anything.
    fn fake(name: &str) -> Worker {
        Worker {
            name: name.into(),
            role: "worker".into(),
            log: String::new(),
            cwd: ".".into(),
            started: 0,
            keep_alive: true,
            stop: Arc::new(AtomicBool::new(false)),
            pid: Arc::new(AtomicU32::new(0)),
            restarts: Arc::new(AtomicU32::new(0)),
            status: Arc::new(tokio::sync::Mutex::new("running".into())),
        }
    }

    fn spec(name: &str) -> Spec {
        Spec {
            name: name.into(),
            role: "worker".into(),
            program: "true".into(),
            args: Vec::new(),
            env: Vec::new(),
            cwd: ".".into(),
            keep_alive: false,
            session_id: None,
            resume: false,
        }
    }

    /// The duplicate check and the insert must share one lock hold —
    /// a name reserved by a live worker rejects a second spawn without ever
    /// touching its entry.
    #[tokio::test]
    async fn duplicate_name_is_rejected_without_overwriting() {
        let (app, path) = app().await;
        let original = fake("backend");
        app.workers
            .lock()
            .await
            .insert("backend".into(), original.clone());
        let err = launch(&app, spec("backend"))
            .await
            .expect_err("duplicate must bail");
        assert!(err.to_string().contains("already exists"));
        let held = app.workers.lock().await.get("backend").cloned().unwrap();
        assert!(
            Arc::ptr_eq(&held.stop, &original.stop),
            "the live worker's entry (and its stop flag) must survive the rejected spawn"
        );
        cleanup(&path);
    }

    /// Backoff grows with consecutive rapid failures and is capped, so a
    /// crash-looping worker stops hammering. `n == 0` means the last run was
    /// healthy, which restarts at the base delay.
    #[test]
    fn backoff_grows_and_caps() {
        assert_eq!(
            backoff(0),
            RESTART_BACKOFF,
            "a healthy run restarts promptly"
        );
        assert_eq!(backoff(1), RESTART_BACKOFF);
        assert_eq!(backoff(2), RESTART_BACKOFF * 2);
        assert_eq!(backoff(3), RESTART_BACKOFF * 4);
        assert_eq!(backoff(99), RESTART_BACKOFF_MAX, "and never runs away");
        for n in 0..64 {
            assert!(backoff(n) <= RESTART_BACKOFF_MAX, "n={n} exceeded the cap");
        }
    }

    /// The old fixed 3s delay burned all 20 restarts in about a minute. Backing
    /// off has to buy meaningfully more time than that before a worker is
    /// retired, so a transient cause has a chance to clear.
    #[test]
    fn the_restart_budget_spans_more_than_a_minute() {
        let total: std::time::Duration = (1..=MAX_RESTARTS).map(backoff).sum();
        assert!(
            total >= std::time::Duration::from_secs(600),
            "a full restart budget should span many minutes, got {total:?}"
        );
    }

    /// The concurrent-worker cap counts live entries under the same lock.
    #[tokio::test]
    async fn worker_cap_is_enforced() {
        let (app, path) = app().await;
        {
            let mut workers = app.workers.lock().await;
            for i in 0..MAX_WORKERS {
                workers.insert(format!("w{i}"), fake(&format!("w{i}")));
            }
        }
        let err = launch(&app, spec("overflow"))
            .await
            .expect_err("cap must bail");
        assert!(err.to_string().contains("cap reached"));
        assert!(!app.workers.lock().await.contains_key("overflow"));
        cleanup(&path);
    }

    /// Stop-and-forget parity between the control plane and the MCP
    /// tool — a live worker's row goes away on stop, and a stale persisted row
    /// with no live worker is cleaned up too.
    #[tokio::test]
    async fn stop_and_forget_deletes_the_row_consistently() {
        let (app, path) = app().await;
        let persisted = db::PersistedWorker {
            name: "backend".into(),
            role: "worker".into(),
            program: "true".into(),
            args: Vec::new(),
            cwd: ".".into(),
            keep_alive: true,
            session_id: None,
        };
        // Live worker: stop succeeds, row deleted.
        db::save_worker(&app.db, &persisted).await.unwrap();
        app.workers
            .lock()
            .await
            .insert("backend".into(), fake("backend"));
        assert!(stop_and_forget(&app, "backend").await);
        assert!(db::load_workers(&app.db).await.unwrap().is_empty());
        // Stale row, no live worker: reported as unknown but cleaned up.
        db::save_worker(&app.db, &persisted).await.unwrap();
        app.workers.lock().await.clear();
        assert!(!stop_and_forget(&app, "backend").await);
        assert!(db::load_workers(&app.db).await.unwrap().is_empty());
        cleanup(&path);
    }
}
