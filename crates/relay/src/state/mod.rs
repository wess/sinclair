use crate::spawn::Worker;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{watch, Mutex, Notify, Semaphore};

/// Most blocking `wait` calls allowed to park at once. Each holds a connection
/// and a task open, so an unbounded number is a trivial local DoS; beyond this
/// cap, `wait` returns immediately instead of parking.
pub const MAX_PARKED_WAITS: usize = 256;

/// Shared application state. Cloned per request (cheap: pool + Arcs).
#[derive(Clone)]
pub struct App {
    pub db: sqlx::SqlitePool,
    /// Per-agent wake signals. A parked `wait` arms its own agent's `Notify`
    /// before checking the inbox; `deliver` pings only the recipients of a
    /// message, so a flood of traffic no longer wakes every parked waiter.
    pub waiters: Arc<Mutex<HashMap<String, Arc<Notify>>>>,
    /// transport session id -> registered agent name.
    pub sessions: Arc<Mutex<HashMap<String, String>>>,
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
}

impl App {
    pub fn new(db: sqlx::SqlitePool, endpoint: String, token: String) -> Self {
        let (events, _) = watch::channel(0u64);
        App {
            db,
            waiters: Arc::new(Mutex::new(HashMap::new())),
            sessions: Arc::new(Mutex::new(HashMap::new())),
            workers: Arc::new(Mutex::new(HashMap::new())),
            endpoint,
            token,
            waits: Arc::new(Semaphore::new(MAX_PARKED_WAITS)),
            events: Arc::new(events),
        }
    }

    /// Signal that the live roster/worker state changed so `/control/events`
    /// re-emits a snapshot.
    pub fn bump(&self) {
        self.events.send_modify(|v| *v = v.wrapping_add(1));
    }

    pub async fn bind(&self, session: &str, name: &str) {
        self.sessions
            .lock()
            .await
            .insert(session.to_string(), name.to_string());
    }

    pub async fn name_of(&self, session: &str) -> Option<String> {
        self.sessions.lock().await.get(session).cloned()
    }

    /// Get (or create) the wake signal for `name`. The waiter inserts its
    /// `Notify` into the map here, *before* arming and querying, so any
    /// concurrent `deliver` that runs after the message is persisted will find
    /// this `Notify` in the map and wake it, no signal can be lost.
    pub async fn waiter(&self, name: &str) -> Arc<Notify> {
        self.waiters
            .lock()
            .await
            .entry(name.to_string())
            .or_insert_with(|| Arc::new(Notify::new()))
            .clone()
    }

    /// Wake a single agent's parked waiter, if any is registered.
    pub async fn wake_one(&self, name: &str) {
        if let Some(n) = self.waiters.lock().await.get(name) {
            n.notify_waiters();
        }
    }

    /// Wake every registered waiter (used for broadcasts).
    pub async fn wake_all(&self) {
        for n in self.waiters.lock().await.values() {
            n.notify_waiters();
        }
    }
}
