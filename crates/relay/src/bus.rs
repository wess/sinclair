//! Core bus operations shared by the MCP tools and the plain-HTTP control plane
//! (used by non-MCP participants like the Ollama bridge).

use crate::db;
use crate::protocol::Message;
use crate::state::App;
use std::time::Duration;

/// Marks an agent parked for the lifetime of a blocking `await_messages` call.
/// `Drop` runs on normal return *and* when the future is cancelled (the agent's
/// `wait` HTTP stream is dropped because its process died), so the parked set is
/// always an honest picture of who is currently reachable.
struct ParkGuard<'a> {
    app: &'a App,
    name: &'a str,
}

impl<'a> ParkGuard<'a> {
    fn new(app: &'a App, name: &'a str) -> Self {
        app.enter_parked(name);
        Self { app, name }
    }
}

impl Drop for ParkGuard<'_> {
    fn drop(&mut self) {
        self.app.leave_parked(self.name);
    }
}

/// Park until messages addressed to `name` arrive (when `block`), then drain
/// them and advance the read cursor. Returns the delivered messages (empty on
/// timeout or when not blocking and the inbox is empty).
pub async fn await_messages(app: &App, name: &str, block: bool, max_wait: Duration) -> Vec<Message> {
    let deadline = tokio::time::Instant::now() + max_wait;
    let signal = app.waiter(name).await;
    // Present in the parked set for the whole blocking call; released on return
    // or cancellation so a dead agent stops looking alive immediately.
    let _park = block.then(|| ParkGuard::new(app, name));
    loop {
        let notified = signal.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();

        let cursor = db::cursor_of(&app.db, name).await.unwrap_or(0);
        let pending = db::pending_for(&app.db, name, cursor)
            .await
            .unwrap_or_default();
        if !pending.is_empty() {
            if let Some(last) = pending.last() {
                let _ = db::advance_cursor(&app.db, name, last.id).await;
            }
            return pending;
        }
        if !block {
            return Vec::new();
        }
        let Ok(_permit) = app.waits.try_acquire() else {
            return Vec::new();
        };
        tokio::select! {
            _ = &mut notified => continue,
            _ = tokio::time::sleep_until(deadline) => return Vec::new(),
        }
    }
}

/// Park until `target`'s reported status is one of `want` (when `block`), then
/// return the matching status. Returns the current status immediately when it
/// already matches, and the current status (matching or not) on timeout. Reuses
/// the same per-agent wake signal messages use: `set_status` wakes `target`'s
/// waiter, so a status change re-checks the predicate here. `want` is matched
/// case-insensitively; an empty `want` matches any non-empty status.
pub async fn await_status(
    app: &App,
    target: &str,
    want: &[String],
    block: bool,
    max_wait: Duration,
) -> String {
    let deadline = tokio::time::Instant::now() + max_wait;
    // Arm the watched agent's signal *before* the first read so a concurrent
    // `set_status` that runs after our read still wakes us (no lost wakeup).
    let signal = app.waiter(target).await;
    loop {
        let notified = signal.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();

        let status = db::status_of(&app.db, target).await.unwrap_or_default();
        if status_matches(&status, want) {
            return status;
        }
        if !block {
            return status;
        }
        let Ok(_permit) = app.waits.try_acquire() else {
            return status;
        };
        tokio::select! {
            _ = &mut notified => continue,
            _ = tokio::time::sleep_until(deadline) => return status,
        }
    }
}

/// Whether `status` satisfies the `want` set (case-insensitive). An empty `want`
/// means "any non-empty status".
fn status_matches(status: &str, want: &[String]) -> bool {
    if status.is_empty() {
        return false;
    }
    if want.is_empty() {
        return true;
    }
    want.iter().any(|w| w.eq_ignore_ascii_case(status))
}

/// Record an agent's semantic status and wake anyone parked on it (both a
/// message `wait` — which harmlessly re-checks — and any `await_status` watching
/// this agent). Bumps the event stream so `/control/events` re-emits the roster.
pub async fn report_status(app: &App, name: &str, status: &str) -> anyhow::Result<()> {
    db::set_status(&app.db, name, status).await?;
    app.wake_one(name).await;
    app.bump();
    Ok(())
}

/// Insert a message and wake parked waiters.
pub async fn deliver(
    app: &App,
    from: &str,
    kind: &str,
    target: Option<&str>,
    body: &str,
) -> anyhow::Result<i64> {
    // A direct message may be addressed to an agent that has not registered yet
    // (the manager assigns a task the instant a team opens, racing the worker's
    // `register`). Pre-create the recipient's row *before* inserting the message
    // so its seeded read cursor sits just below this message's id — otherwise a
    // brand-new agent, whose cursor is initialized to the current tip at
    // register time, would never see the task and would stall "with nothing to
    // do" (issue #5).
    if let ("direct", Some(to)) = (kind, target) {
        db::ensure_agent(&app.db, to).await?;
    }
    let id = db::insert_message(&app.db, from, kind, target, body).await?;
    match (kind, target) {
        ("direct", Some(to)) => app.wake_one(to).await,
        ("channel", Some(channel)) => {
            for agent in db::channel_subs(&app.db, channel).await.unwrap_or_default() {
                app.wake_one(&agent).await;
            }
        }
        _ => app.wake_all().await,
    }
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::App;

    async fn app() -> (App, std::path::PathBuf) {
        // A unique temp DB per test; sqlite WAL needs a real file, not :memory:.
        static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!("relay-bus-{}-{n}.db", std::process::id()));
        let pool = db::open(path.to_str().unwrap()).await.unwrap();
        (App::new(pool, "http://127.0.0.1:0".into(), "t".into()), path)
    }

    fn cleanup(path: &std::path::Path) {
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(path.with_extension("db-wal"));
        let _ = std::fs::remove_file(path.with_extension("db-shm"));
    }

    /// Issue #5: a direct message sent *before* the recipient registers must
    /// still be delivered once it does (the manager assigns a task the instant a
    /// team opens, racing the worker's `register`).
    #[tokio::test]
    async fn direct_message_before_register_is_delivered() {
        let (app, path) = app().await;
        deliver(&app, "manager", "direct", Some("backend"), "build the api")
            .await
            .unwrap();
        // backend registers only afterwards.
        db::upsert_agent(&app.db, "backend", "backend", "").await.unwrap();
        let msgs = await_messages(&app, "backend", false, Duration::from_millis(10)).await;
        assert_eq!(msgs.len(), 1, "queued task should survive late registration");
        assert_eq!(msgs[0].body, "build the api");
        cleanup(&path);
    }

    /// Issue #4: a spawned worker round-trips through the DB so a restarted
    /// daemon can bring it back, and an explicit delete forgets it.
    #[tokio::test]
    async fn workers_persist_reload_and_delete() {
        let (app, path) = app().await;
        let args = vec!["-p".to_string(), "hi".to_string()];
        db::save_worker(
            &app.db,
            &db::PersistedWorker {
                name: "backend".into(),
                role: "backend".into(),
                program: "claude".into(),
                args: args.clone(),
                cwd: "/tmp".into(),
                keep_alive: true,
                session_id: Some("sess-1".into()),
            },
        )
        .await
        .unwrap();
        let loaded = db::load_workers(&app.db).await.unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name, "backend");
        assert_eq!(loaded[0].args, args);
        assert_eq!(loaded[0].session_id.as_deref(), Some("sess-1"));
        assert!(loaded[0].keep_alive);
        db::delete_worker(&app.db, "backend").await.unwrap();
        assert!(db::load_workers(&app.db).await.unwrap().is_empty());
        cleanup(&path);
    }

    /// A reported status is stored and returned; a non-blocking `await_status`
    /// matches once the state is set and reports the current state otherwise.
    #[tokio::test]
    async fn status_report_and_wait() {
        let (app, path) = app().await;
        db::upsert_agent(&app.db, "worker", "worker", "").await.unwrap();
        // Nothing reported yet: an empty `want` (any non-empty) does not match.
        let s = await_status(&app, "worker", &[], false, Duration::from_millis(10)).await;
        assert_eq!(s, "", "no status reported yet");
        report_status(&app, "worker", "working").await.unwrap();
        let s = await_status(&app, "worker", &["done".into()], false, Duration::from_millis(10)).await;
        assert_eq!(s, "working", "returns current status when it does not match");
        report_status(&app, "worker", "done").await.unwrap();
        let s = await_status(&app, "worker", &["done".into(), "blocked".into()], false, Duration::from_millis(10)).await;
        assert_eq!(s, "done", "matches once the target reaches the wanted state");
        cleanup(&path);
    }

    /// A blocking `await_status` parked on one agent is woken when *that* agent
    /// reports the wanted state from another task.
    #[tokio::test]
    async fn wait_status_wakes_on_report() {
        let (app, path) = app().await;
        db::upsert_agent(&app.db, "builder", "worker", "").await.unwrap();
        let waiter = {
            let app = app.clone();
            tokio::spawn(async move {
                await_status(&app, "builder", &["done".into()], true, Duration::from_secs(5)).await
            })
        };
        // Give the waiter a moment to park, then flip the status.
        tokio::time::sleep(Duration::from_millis(50)).await;
        report_status(&app, "builder", "done").await.unwrap();
        let got = tokio::time::timeout(Duration::from_secs(2), waiter)
            .await
            .expect("waiter should wake promptly")
            .unwrap();
        assert_eq!(got, "done");
        cleanup(&path);
    }

    /// A newcomer still must NOT see broadcast history from before it joined —
    /// the pre-create only applies to direct recipients, so this stays intact.
    #[tokio::test]
    async fn broadcast_history_not_replayed_to_newcomer() {
        let (app, path) = app().await;
        deliver(&app, "manager", "broadcast", None, "standup in 5")
            .await
            .unwrap();
        db::upsert_agent(&app.db, "late", "worker", "").await.unwrap();
        let msgs = await_messages(&app, "late", false, Duration::from_millis(10)).await;
        assert!(msgs.is_empty(), "a new agent sees no prior broadcast");
        cleanup(&path);
    }
}
