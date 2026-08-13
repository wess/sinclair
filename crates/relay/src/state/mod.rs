use crate::spawn::Worker;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{watch, Mutex, Notify, Semaphore};

/// Most blocking `wait` calls allowed to park at once. Each holds a connection
/// and a task open, so an unbounded number is a trivial local DoS; past this
/// cap new waits queue briefly and are then told to back off (see
/// `bus::park_slot`).
pub const MAX_PARKED_WAITS: usize = 256;

/// An agent that made a tool call within this many seconds counts as live even
/// when it is not currently parked on `wait` (i.e. actively working). Combined
/// with parked-set membership this gives a truthful "alive" signal: a dead
/// process leaves the parked set at once (its `wait` SSE stream is dropped) and
/// stops touching, so it ages out of the live set within this window.
pub const ACTIVE_WINDOW_SECS: i64 = 60;

/// Transport sessions idle longer than this are forgotten. A live agent parks
/// on `wait` and re-polls well inside the window (WAIT_MAX is 25 minutes), so
/// only sessions whose process is gone age out.
const SESSION_TTL_SECS: i64 = 8 * 3600;

/// One `Notify` shared by everyone currently waiting on a name, plus how many
/// [`WaiterGuard`]s reference it so the map entry can be dropped with the last.
struct WaiterSlot {
    notify: Arc<Notify>,
    refs: usize,
}

type WaiterMap = Arc<std::sync::Mutex<HashMap<String, WaiterSlot>>>;

/// A registered interest in a name's wake signal. Holding it keeps the shared
/// `Notify` in the waiters map; dropping the last guard for a name removes the
/// entry, so the map only tracks names someone is actually waiting on instead
/// of growing forever.
pub struct WaiterGuard {
    map: WaiterMap,
    name: String,
    notify: Arc<Notify>,
}

impl WaiterGuard {
    pub fn notify(&self) -> &Notify {
        &self.notify
    }
}

impl Drop for WaiterGuard {
    fn drop(&mut self) {
        let mut m = self.map.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(slot) = m.get_mut(&self.name) {
            slot.refs = slot.refs.saturating_sub(1);
            if slot.refs == 0 {
                m.remove(&self.name);
            }
        }
    }
}

/// A transport session bound to an agent name. `last_used` ages abandoned
/// sessions out (see [`SESSION_TTL_SECS`]).
struct Session {
    name: String,
    last_used: i64,
}

/// Shared application state. Cloned per request (cheap: pool + Arcs).
#[derive(Clone)]
pub struct App {
    pub db: sqlx::SqlitePool,
    /// Per-agent wake signals, present only while someone waits (see
    /// [`WaiterGuard`]). A parked `wait` arms its own agent's `Notify` before
    /// checking the inbox; `deliver` pings only the recipients of a message, so
    /// a flood of traffic no longer wakes every parked waiter. A `std` mutex so
    /// the guard's `Drop` can release it without awaiting.
    waiters: WaiterMap,
    /// transport session id -> registered agent name (+ last use).
    sessions: Arc<Mutex<HashMap<String, Session>>>,
    /// Headless workers spawned by this server, keyed by name.
    pub workers: Arc<Mutex<HashMap<String, Worker>>>,
    /// This server's own MCP URL, handed to spawned workers.
    pub endpoint: String,
    /// Bearer token every HTTP request must carry; gates the bus to processes
    /// that can read the 0600 `server.json` (i.e. the same user).
    pub token: String,
    /// Caps how many blocking waits can park concurrently.
    pub waits: Arc<Semaphore>,
    /// Monotonic counter bumped whenever roster/worker status changes, so the
    /// `/control/events` stream can re-emit a fresh snapshot. Subscribers call
    /// `.changed()` then re-query; `send_modify` never errors with no receivers.
    pub events: Arc<watch::Sender<u64>>,
    /// Highest delivered message id; `deliver` bumps it so `/control/feed/live`
    /// streams can push new messages instead of the CLI polling for them.
    pub feed_tip: Arc<watch::Sender<i64>>,
    /// Agents currently parked on a blocking `wait`, by name (ref-counted for
    /// overlapping calls). A parked agent is provably alive: if its process
    /// dies, axum drops the `wait` SSE future and the [`ParkGuard`] in
    /// `bus::await_messages` removes it here. A `std` mutex so the guard's
    /// `Drop` can release it without awaiting.
    pub parked: Arc<std::sync::Mutex<HashMap<String, u32>>>,
}

impl App {
    pub fn new(db: sqlx::SqlitePool, endpoint: String, token: String) -> Self {
        let (events, _) = watch::channel(0u64);
        let (feed_tip, _) = watch::channel(0i64);
        App {
            db,
            waiters: Arc::new(std::sync::Mutex::new(HashMap::new())),
            sessions: Arc::new(Mutex::new(HashMap::new())),
            workers: Arc::new(Mutex::new(HashMap::new())),
            endpoint,
            token,
            waits: Arc::new(Semaphore::new(MAX_PARKED_WAITS)),
            events: Arc::new(events),
            feed_tip: Arc::new(feed_tip),
            parked: Arc::new(std::sync::Mutex::new(HashMap::new())),
        }
    }

    /// Mark `name` as parked on a blocking wait (ref-counted). Bumps the event
    /// stream so the roster's liveness refreshes on the transition.
    pub fn enter_parked(&self, name: &str) {
        {
            let mut m = self.parked.lock().unwrap_or_else(|e| e.into_inner());
            *m.entry(name.to_string()).or_insert(0) += 1;
        }
        self.bump();
    }

    /// Drop one park reference for `name`, removing it when the count hits zero.
    /// Runs when a `wait` returns *or* its future is cancelled (the agent died),
    /// so a dead agent leaves the live set at once.
    pub fn leave_parked(&self, name: &str) {
        {
            let mut m = self.parked.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(c) = m.get_mut(name) {
                *c = c.saturating_sub(1);
                if *c == 0 {
                    m.remove(name);
                }
            }
        }
        self.bump();
    }

    /// Whether `name` is currently parked on a blocking wait.
    pub fn is_parked(&self, name: &str) -> bool {
        self.parked
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(name)
            .is_some_and(|c| *c > 0)
    }

    /// Truthful liveness: parked on `wait`, or active (touched) within
    /// [`ACTIVE_WINDOW_SECS`]. A crashed agent satisfies neither.
    pub fn is_live(&self, name: &str, last_seen: i64) -> bool {
        self.is_parked(name)
            || crate::protocol::now().saturating_sub(last_seen) <= ACTIVE_WINDOW_SECS
    }

    /// Signal that the live roster/worker state changed so `/control/events`
    /// re-emits a snapshot.
    pub fn bump(&self) {
        self.events.send_modify(|v| *v = v.wrapping_add(1));
    }

    pub async fn bind(&self, session: &str, name: &str) {
        let now = crate::protocol::now();
        let mut m = self.sessions.lock().await;
        // A new binding is the cheap moment to age out sessions whose client
        // vanished without the DELETE — the map stays bounded by live use.
        m.retain(|_, s| now.saturating_sub(s.last_used) <= SESSION_TTL_SECS);
        m.insert(
            session.to_string(),
            Session {
                name: name.to_string(),
                last_used: now,
            },
        );
    }

    pub async fn name_of(&self, session: &str) -> Option<String> {
        let mut m = self.sessions.lock().await;
        let s = m.get_mut(session)?;
        s.last_used = crate::protocol::now();
        Some(s.name.clone())
    }

    /// Forget a transport session (the client DELETEd it on shutdown).
    pub async fn unbind(&self, session: &str) {
        self.sessions.lock().await.remove(session);
    }

    /// Register interest in `name`'s wake signal. The guard inserts the shared
    /// `Notify` into the map, *before* arming and querying, so any concurrent
    /// `deliver` that runs after the message is persisted will find it and wake
    /// it — no signal can be lost. Dropping the last guard evicts the entry.
    pub fn waiter(&self, name: &str) -> WaiterGuard {
        let notify = {
            let mut m = self.waiters.lock().unwrap_or_else(|e| e.into_inner());
            let slot = m.entry(name.to_string()).or_insert_with(|| WaiterSlot {
                notify: Arc::new(Notify::new()),
                refs: 0,
            });
            slot.refs += 1;
            slot.notify.clone()
        };
        WaiterGuard {
            map: self.waiters.clone(),
            name: name.to_string(),
            notify,
        }
    }

    /// How many names currently have a live waiter.
    #[cfg(test)]
    pub fn waiter_count(&self) -> usize {
        self.waiters.lock().unwrap_or_else(|e| e.into_inner()).len()
    }

    /// Wake a single agent's parked waiter, if any is registered.
    pub fn wake_one(&self, name: &str) {
        if let Some(slot) = self
            .waiters
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(name)
        {
            slot.notify.notify_waiters();
        }
    }

    /// Wake every registered waiter (used for broadcasts).
    pub fn wake_all(&self) {
        for slot in self
            .waiters
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
        {
            slot.notify.notify_waiters();
        }
    }
}
