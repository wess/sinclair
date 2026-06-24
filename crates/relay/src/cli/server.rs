use super::paths::{self, ServerInfo};
use super::{http, ServeArgs};
use crate::control;
use crate::db;
use crate::spawn;
use crate::state::App;
use crate::transport;
use anyhow::{anyhow, Result};
use axum::routing::{get, post};
use axum::Router;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::time::Duration;

fn resolve(args: &ServeArgs) -> (String, String) {
    let addr = args
        .addr
        .clone()
        .or_else(|| std::env::var("RELAY_ADDR").ok())
        .unwrap_or_else(|| "127.0.0.1:7777".into());
    let db = args
        .db
        .clone()
        .or_else(|| std::env::var("RELAY_DB").ok())
        .unwrap_or_else(|| ".relay/relay.db".into());
    (addr, db)
}

/// Run the server in the foreground (the actual daemon body).
pub async fn serve(args: ServeArgs) -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("relay=info")),
        )
        .init();

    let (addr, db_path) = resolve(&args);
    paths::ensure_dir()?;
    let pool = db::open(&db_path).await?;
    let app = App::new(pool, paths::endpoint(&addr));

    let router = Router::new()
        .route("/mcp", post(transport::handle))
        .route("/health", get(|| async { "ok" }))
        .merge(control::routes())
        .with_state(app.clone());

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    paths::write_info(&ServerInfo {
        pid: std::process::id(),
        addr: addr.clone(),
        db: db_path,
    })?;
    tracing::info!("relay listening on {}", paths::endpoint(&addr));

    let shutdown_app = app.clone();
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown(shutdown_app))
        .await?;
    paths::clear_info();
    Ok(())
}

async fn shutdown(app: App) {
    use tokio::signal::unix::{signal, SignalKind};
    let mut term = signal(SignalKind::terminate()).expect("sigterm");
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = term.recv() => {}
    }
    tracing::info!("shutting down; stopping workers");
    spawn::stop_all(&app).await;
}

/// Spawn the server as a detached background process.
pub fn start(args: ServeArgs) -> Result<()> {
    if let Ok(info) = paths::read_info() {
        if paths::alive(info.pid) {
            println!("relay already running (pid {}) on {}", info.pid, info.addr);
            return Ok(());
        }
        paths::clear_info();
    }

    let (addr, db_path) = resolve(&args);
    paths::ensure_dir()?;
    let exe = std::env::current_exe()?;
    let log = std::fs::File::create(paths::log_path())?;
    let errlog = log.try_clone()?;

    let mut cmd = Command::new(exe);
    cmd.arg("--home")
        .arg(paths::dir())
        .arg("serve")
        .arg("--addr")
        .arg(&addr)
        .arg("--db")
        .arg(&db_path)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(errlog));
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    cmd.spawn()?;

    // Wait for the daemon to bind.
    for _ in 0..40 {
        std::thread::sleep(Duration::from_millis(75));
        if http::get(&addr, "/health").is_ok() {
            println!("relay started on {}", paths::endpoint(&addr));
            return Ok(());
        }
    }
    Err(anyhow!(
        "server did not come up — see {}",
        paths::log_path().display()
    ))
}

pub fn stop() -> Result<()> {
    let info = paths::read_info()?;
    if !paths::alive(info.pid) {
        paths::clear_info();
        println!("relay was not running (cleaned stale record)");
        return Ok(());
    }
    unsafe {
        libc::kill(info.pid as i32, libc::SIGTERM);
    }
    for _ in 0..40 {
        if !paths::alive(info.pid) {
            paths::clear_info();
            println!("relay stopped");
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(75));
    }
    Err(anyhow!("server (pid {}) did not stop", info.pid))
}

pub fn restart(args: ServeArgs) -> Result<()> {
    let _ = stop();
    start(args)
}

pub fn status() -> Result<()> {
    match paths::read_info() {
        Ok(info) if paths::alive(info.pid) => {
            let health = http::get(&info.addr, "/health").is_ok();
            println!(
                "running · pid {} · {} · health {}",
                info.pid,
                paths::endpoint(&info.addr),
                if health { "ok" } else { "unreachable" }
            );
        }
        Ok(_) => println!("not running (stale record present)"),
        Err(_) => println!("not running"),
    }
    Ok(())
}
