//! Core bus operations shared by the MCP tools and the plain-HTTP control plane
//! (used by non-MCP participants like the Ollama bridge).
//!
//! # Delivery contract
//!
//! Delivery is at-least-once. [`await_messages`] only *reads* an agent's
//! pending messages; the read cursor advances via [`ack`] once the response is
//! known to have reached the client — the MCP transport acks after the reply
//! event is handed to the connection, and control-plane clients ack in their
//! next `/control/wait` request. A drain whose connection died before the ack
//! is simply re-read by the next `wait`: duplicates are possible on that rare
//! path, silent loss is not. Acks are monotonic (the cursor never rewinds) and
//! the pending read is a single SQL statement, so concurrent drains for one
//! name can only duplicate, never skip. DB failures surface as errors instead
//! of masquerading as an empty inbox.

use crate::db;
use crate::protocol::Message;
use crate::state::App;
use anyhow::{anyhow, Result};
use std::time::Duration;

/// How long an over-cap blocking wait queues for a park slot before the caller
/// is told to back off (see [`BACKOFF`]) rather than being bounced instantly —
/// an instant empty return turns every over-cap agent into a hot retry loop.
const QUEUE_GRACE: Duration = Duration::from_secs(3);

/// Error text returned when the parked-wait pool stays saturated past
/// [`QUEUE_GRACE`]; callers should pause briefly and retry.
pub const BACKOFF: &str = "wait capacity exhausted — pause briefly, then call wait again";

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

/// Park until messages addressed to `name` arrive (when `block`), then return
/// them *without* advancing the read cursor — the caller acks with [`ack`] once
/// the delivery actually reached the client (see the module docs). Returns
/// empty on timeout or when not blocking and the inbox is empty; [`BACKOFF`]
/// when the park pool stays saturated past the queue grace.
pub async fn await_messages(
    app: &App,
    name: &str,
    block: bool,
    max_wait: Duration,
) -> Result<Vec<Message>> {
    let deadline = tokio::time::Instant::now() + max_wait;
    let signal = app.waiter(name);
    // Present in the parked set for the whole blocking call; released on return
    // or cancellation so a dead agent stops looking alive immediately.
    let _park = block.then(|| ParkGuard::new(app, name));
    loop {
        let notified = signal.notify().notified();
        tokio::pin!(notified);
        notified.as_mut().enable();

        let pending = db::pending_for(&app.db, name).await?;
        if !pending.is_empty() {
            return Ok(pending);
        }
        if !block {
            return Ok(Vec::new());
        }
        let Some(_permit) = park_slot(app, deadline).await? else {
            return Ok(Vec::new());
        };
        tokio::select! {
            _ = &mut notified => continue,
            _ = tokio::time::sleep_until(deadline) => return Ok(Vec::new()),
        }
    }
}

/// Acknowledge delivery of everything up to `last_id` for `name`, advancing its
/// read cursor. Call only once the response has reached the client; an unacked
/// drain is redelivered by the next `wait`.
pub async fn ack(app: &App, name: &str, last_id: i64) -> Result<()> {
    db::ack_delivered(&app.db, name, last_id).await
}

/// Acquire a park slot, queueing up to [`QUEUE_GRACE`] (bounded by `deadline`)
/// when the pool is saturated instead of failing instantly. `Ok(None)` means
/// the overall deadline elapsed while queued — a normal empty timeout; the
/// [`BACKOFF`] error means the pool stayed full past the grace period.
async fn park_slot<'a>(
    app: &'a App,
    deadline: tokio::time::Instant,
) -> Result<Option<tokio::sync::SemaphorePermit<'a>>> {
    let grace = tokio::time::Instant::now() + QUEUE_GRACE;
    tokio::select! {
        permit = app.waits.acquire() => Ok(Some(permit.map_err(|_| anyhow!(BACKOFF))?)),
        _ = tokio::time::sleep_until(grace.min(deadline)) => {
            if deadline <= grace {
                Ok(None)
            } else {
                Err(anyhow!(BACKOFF))
            }
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
) -> Result<String> {
    let deadline = tokio::time::Instant::now() + max_wait;
    // Arm the watched agent's signal *before* the first read so a concurrent
    // `set_status` that runs after our read still wakes us (no lost wakeup).
    let signal = app.waiter(target);
    loop {
        let notified = signal.notify().notified();
        tokio::pin!(notified);
        notified.as_mut().enable();

        let status = db::status_of(&app.db, target).await?;
        if status_matches(&status, want) {
            return Ok(status);
        }
        if !block {
            return Ok(status);
        }
        let Some(_permit) = park_slot(app, deadline).await? else {
            return Ok(status);
        };
        tokio::select! {
            _ = &mut notified => continue,
            _ = tokio::time::sleep_until(deadline) => return Ok(status),
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
pub async fn report_status(app: &App, name: &str, status: &str) -> Result<()> {
    db::set_status(&app.db, name, status).await?;
    app.wake_one(name);
    app.bump();
    Ok(())
}

/// Insert a message, wake parked waiters, and bump the feed tip so live feed
/// streams push it out.
pub async fn deliver(
    app: &App,
    from: &str,
    kind: &str,
    target: Option<&str>,
    body: &str,
) -> Result<i64> {
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
    app.feed_tip.send_modify(|v| *v = (*v).max(id));
    match (kind, target) {
        ("direct", Some(to)) => app.wake_one(to),
        ("channel", Some(channel)) => {
            // A failed subscriber lookup used to be swallowed into an empty
            // list, waking nobody: the message was stored but every parked
            // subscriber slept out its full deadline. Falling back to waking
            // everyone is the safe read — a spurious wake just re-checks the
            // inbox, a missed one stalls the mesh.
            match db::channel_subs(&app.db, channel).await {
                Ok(subs) => {
                    for agent in subs {
                        app.wake_one(&agent);
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        "relay: subscriber lookup for #{channel} failed ({e}); waking all waiters"
                    );
                    app.wake_all();
                }
            }
        }
        _ => app.wake_all(),
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
        db::upsert_agent(&app.db, "backend", "backend", "")
            .await
            .unwrap();
        let msgs = await_messages(&app, "backend", false, Duration::from_millis(10))
            .await
            .unwrap();
        assert_eq!(
            msgs.len(),
            1,
            "queued task should survive late registration"
        );
        assert_eq!(msgs[0].body, "build the api");
        cleanup(&path);
    }

    /// The delivery contract: a drain does not consume — the same messages come
    /// back until the caller acks, and the ack ends the redelivery.
    #[tokio::test]
    async fn unacked_drain_is_redelivered_until_acked() {
        let (app, path) = app().await;
        db::upsert_agent(&app.db, "backend", "backend", "")
            .await
            .unwrap();
        deliver(&app, "manager", "direct", Some("backend"), "task 1")
            .await
            .unwrap();
        let first = await_messages(&app, "backend", false, Duration::from_millis(10))
            .await
            .unwrap();
        assert_eq!(first.len(), 1);
        // No ack (the response never reached the client): read again, same message.
        let again = await_messages(&app, "backend", false, Duration::from_millis(10))
            .await
            .unwrap();
        assert_eq!(again.len(), 1, "unacked delivery must be re-read, not lost");
        assert_eq!(again[0].id, first[0].id);
        ack(&app, "backend", again[0].id).await.unwrap();
        let after = await_messages(&app, "backend", false, Duration::from_millis(10))
            .await
            .unwrap();
        assert!(after.is_empty(), "acked messages stay consumed");
        cleanup(&path);
    }

    /// A stale ack (smaller id than the cursor) must not rewind the cursor and
    /// resurrect already-consumed history.
    #[tokio::test]
    async fn ack_never_rewinds_the_cursor() {
        let (app, path) = app().await;
        db::upsert_agent(&app.db, "backend", "backend", "")
            .await
            .unwrap();
        deliver(&app, "manager", "direct", Some("backend"), "one")
            .await
            .unwrap();
        let id2 = deliver(&app, "manager", "direct", Some("backend"), "two")
            .await
            .unwrap();
        ack(&app, "backend", id2).await.unwrap();
        ack(&app, "backend", 1).await.unwrap();
        let msgs = await_messages(&app, "backend", false, Duration::from_millis(10))
            .await
            .unwrap();
        assert!(
            msgs.is_empty(),
            "an out-of-order ack must not replay history"
        );
        cleanup(&path);
    }

    /// A DB failure must surface as an error, not as an empty inbox
    /// (the old `unwrap_or(0)` replayed full history and rewrote the cursor).
    #[tokio::test]
    async fn db_failure_is_reported_not_swallowed() {
        let (app, path) = app().await;
        app.db.close().await;
        let res = await_messages(&app, "backend", false, Duration::from_millis(10)).await;
        assert!(
            res.is_err(),
            "a closed pool must be an error, not an empty inbox"
        );
        cleanup(&path);
    }

    /// With the park pool saturated, a blocking wait queues for the
    /// grace period and then gets an explicit backoff error — never the old
    /// instant empty return that turned callers into hot retry loops.
    #[tokio::test]
    async fn saturated_pool_queues_then_backs_off() {
        let (app, path) = app().await;
        db::upsert_agent(&app.db, "backend", "backend", "")
            .await
            .unwrap();
        let _all = app
            .waits
            .acquire_many(crate::state::MAX_PARKED_WAITS as u32)
            .await
            .unwrap();
        let start = std::time::Instant::now();
        let res = await_messages(&app, "backend", true, Duration::from_secs(60)).await;
        let err = res.expect_err("saturation must be explicit");
        assert!(err.to_string().contains(BACKOFF));
        assert!(
            start.elapsed() >= QUEUE_GRACE,
            "over-cap waits queue for the grace period first"
        );
        cleanup(&path);
    }

    /// With the pool saturated but the caller's own deadline sooner than the
    /// grace, the wait ends as a normal empty timeout.
    #[tokio::test]
    async fn saturated_pool_respects_a_shorter_deadline() {
        let (app, path) = app().await;
        db::upsert_agent(&app.db, "backend", "backend", "")
            .await
            .unwrap();
        let _all = app
            .waits
            .acquire_many(crate::state::MAX_PARKED_WAITS as u32)
            .await
            .unwrap();
        let msgs = await_messages(&app, "backend", true, Duration::from_millis(50))
            .await
            .unwrap();
        assert!(msgs.is_empty());
        cleanup(&path);
    }

    /// `deliver` bumps the feed tip so `/control/feed/live` streams
    /// push new messages instead of the CLI polling.
    #[tokio::test]
    async fn deliver_bumps_the_feed_tip() {
        let (app, path) = app().await;
        let mut rx = app.feed_tip.subscribe();
        let id = deliver(&app, "manager", "broadcast", None, "hi")
            .await
            .unwrap();
        rx.changed().await.unwrap();
        assert_eq!(*rx.borrow(), id);
        cleanup(&path);
    }

    /// The waiters map holds an entry only while someone waits on the
    /// name; the last guard dropping evicts it, so the map cannot grow forever.
    #[tokio::test]
    async fn waiter_entries_are_evicted_after_the_wait() {
        let (app, path) = app().await;
        db::upsert_agent(&app.db, "backend", "backend", "")
            .await
            .unwrap();
        assert_eq!(app.waiter_count(), 0);
        let _ = await_messages(&app, "backend", false, Duration::from_millis(10))
            .await
            .unwrap();
        assert_eq!(
            app.waiter_count(),
            0,
            "a finished wait leaves no entry behind"
        );
        let g1 = app.waiter("backend");
        let g2 = app.waiter("backend");
        assert_eq!(app.waiter_count(), 1);
        drop(g1);
        assert_eq!(
            app.waiter_count(),
            1,
            "entry survives while a waiter remains"
        );
        drop(g2);
        assert_eq!(app.waiter_count(), 0);
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
        db::upsert_agent(&app.db, "worker", "worker", "")
            .await
            .unwrap();
        // Nothing reported yet: an empty `want` (any non-empty) does not match.
        let s = await_status(&app, "worker", &[], false, Duration::from_millis(10))
            .await
            .unwrap();
        assert_eq!(s, "", "no status reported yet");
        report_status(&app, "worker", "working").await.unwrap();
        let s = await_status(
            &app,
            "worker",
            &["done".into()],
            false,
            Duration::from_millis(10),
        )
        .await
        .unwrap();
        assert_eq!(
            s, "working",
            "returns current status when it does not match"
        );
        report_status(&app, "worker", "done").await.unwrap();
        let s = await_status(
            &app,
            "worker",
            &["done".into(), "blocked".into()],
            false,
            Duration::from_millis(10),
        )
        .await
        .unwrap();
        assert_eq!(
            s, "done",
            "matches once the target reaches the wanted state"
        );
        cleanup(&path);
    }

    /// A blocking `await_status` parked on one agent is woken when *that* agent
    /// reports the wanted state from another task.
    #[tokio::test]
    async fn wait_status_wakes_on_report() {
        let (app, path) = app().await;
        db::upsert_agent(&app.db, "builder", "worker", "")
            .await
            .unwrap();
        let waiter = {
            let app = app.clone();
            tokio::spawn(async move {
                await_status(
                    &app,
                    "builder",
                    &["done".into()],
                    true,
                    Duration::from_secs(5),
                )
                .await
                .unwrap()
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

    /// A channel post must wake a subscriber that is parked on `wait`. The
    /// subscriber lookup used to swallow errors into an empty list, waking
    /// nobody: the post landed in the DB but every parked subscriber slept out
    /// its full deadline before noticing.
    #[tokio::test]
    async fn a_channel_post_wakes_a_parked_subscriber() {
        let (app, path) = app().await;
        db::upsert_agent(&app.db, "backend", "worker", "")
            .await
            .unwrap();
        db::subscribe(&app.db, "backend", "devops").await.unwrap();
        let waiter = {
            let app = app.clone();
            tokio::spawn(async move {
                await_messages(&app, "backend", true, Duration::from_secs(5))
                    .await
                    .unwrap()
            })
        };
        // Let it park, then post to the channel it subscribes to.
        tokio::time::sleep(Duration::from_millis(50)).await;
        deliver(&app, "lead", "channel", Some("devops"), "deploy now")
            .await
            .unwrap();
        let msgs = tokio::time::timeout(Duration::from_secs(2), waiter)
            .await
            .expect("a channel post must wake its subscribers promptly")
            .unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].body, "deploy now");
        cleanup(&path);
    }

    /// A direct message wakes its recipient without waiting out the deadline.
    #[tokio::test]
    async fn a_direct_message_wakes_a_parked_recipient() {
        let (app, path) = app().await;
        db::upsert_agent(&app.db, "backend", "worker", "")
            .await
            .unwrap();
        let waiter = {
            let app = app.clone();
            tokio::spawn(async move {
                await_messages(&app, "backend", true, Duration::from_secs(5))
                    .await
                    .unwrap()
            })
        };
        tokio::time::sleep(Duration::from_millis(50)).await;
        deliver(&app, "lead", "direct", Some("backend"), "ship it")
            .await
            .unwrap();
        let msgs = tokio::time::timeout(Duration::from_secs(2), waiter)
            .await
            .expect("a direct message must wake its recipient promptly")
            .unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].body, "ship it");
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
        db::upsert_agent(&app.db, "late", "worker", "")
            .await
            .unwrap();
        let msgs = await_messages(&app, "late", false, Duration::from_millis(10))
            .await
            .unwrap();
        assert!(msgs.is_empty(), "a new agent sees no prior broadcast");
        cleanup(&path);
    }
}
