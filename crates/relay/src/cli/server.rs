use super::paths::{self, ServerInfo};
use super::{http, ServeArgs};
use crate::control;
use crate::db;
use crate::spawn;
use crate::state::App;
use crate::transport;
use anyhow::{anyhow, Result};
use axum::extract::State;
use axum::http::{header::AUTHORIZATION, StatusCode};
use axum::response::{IntoResponse, Response};
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
    paths::reap_stray_workers();
    let pool = db::open(&db_path).await?;
    paths::lock_file(std::path::Path::new(&db_path));
    let token = gen_token();
    let app = App::new(pool, paths::endpoint(&addr), token.clone());

    let guarded = Router::new()
        .route("/mcp", post(transport::handle))
        .merge(control::routes())
        .layer(axum::middleware::from_fn_with_state(app.clone(), auth))
        .with_state(app.clone());
    let router = Router::new()
        .route("/health", get(|| async { "ok" }))
        .merge(guarded);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    paths::write_info(&ServerInfo {
        pid: std::process::id(),
        addr: addr.clone(),
        db: db_path,
        token,
    })?;
    tracing::info!("relay listening on {}", paths::endpoint(&addr));

    let shutdown_app = app.clone();
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown(shutdown_app))
        .await?;
    paths::clear_info();
    Ok(())
}

/// A fresh 256-bit token as hex (two v4 UUIDs concatenated).
fn gen_token() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

/// Reject any request whose `Authorization: Bearer …` doesn't match the token.
async fn auth(State(app): State<App>, req: axum::extract::Request, next: axum::middleware::Next) -> Response {
    let presented = req
        .headers()
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    match presented {
        Some(t) if constant_time_eq(t, &app.token) => next.run(req).await,
        _ => StatusCode::UNAUTHORIZED.into_response(),
    }
}

/// Length-independent byte comparison, so a wrong token leaks no timing signal.
fn constant_time_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b) {
        diff |= x ^ y;
    }
    diff == 0
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
    paths::lock_file(&paths::log_path());
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
