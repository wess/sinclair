use crate::spawn::Worker;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, Notify};

/// Shared application state. Cloned per request (cheap: pool + Arcs).
#[derive(Clone)]
pub struct App {
    pub db: sqlx::SqlitePool,
    /// Pinged whenever a new message is persisted, to wake parked `wait` calls.
    pub notify: Arc<Notify>,
    /// transport session id -> registered agent name.
    pub sessions: Arc<Mutex<HashMap<String, String>>>,
    /// Headless workers spawned by this server, keyed by name.
    pub workers: Arc<Mutex<HashMap<String, Worker>>>,
    /// This server's own MCP URL, handed to spawned workers.
    pub endpoint: String,
}

impl App {
    pub fn new(db: sqlx::SqlitePool, endpoint: String) -> Self {
        App {
            db,
            notify: Arc::new(Notify::new()),
            sessions: Arc::new(Mutex::new(HashMap::new())),
            workers: Arc::new(Mutex::new(HashMap::new())),
            endpoint,
        }
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

    /// Signal all parked waiters to re-check their inbox.
    pub fn wake(&self) {
        self.notify.notify_waiters();
    }
}
