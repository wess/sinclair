use crate::protocol::{now, Message};
use anyhow::Result;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;

/// Cap on retained messages. Readers track a cursor by id, so dropping the
/// oldest already-delivered rows keeps the on-disk file from growing forever.
const MESSAGE_RETENTION: i64 = 10_000;

pub async fn open(path: &str) -> Result<SqlitePool> {
    let opts = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .busy_timeout(std::time::Duration::from_secs(5))
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal);

    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .connect_with(opts)
        .await?;

    migrate(&pool).await?;
    Ok(pool)
}

async fn migrate(pool: &SqlitePool) -> Result<()> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS agents (
            name      TEXT PRIMARY KEY,
            role      TEXT NOT NULL DEFAULT '',
            caps      TEXT NOT NULL DEFAULT '',
            cursor    INTEGER NOT NULL DEFAULT 0,
            online    INTEGER NOT NULL DEFAULT 1,
            last_seen INTEGER NOT NULL DEFAULT 0
        );
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS subs (
            agent   TEXT NOT NULL,
            channel TEXT NOT NULL,
            PRIMARY KEY (agent, channel)
        );
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS messages (
            id      INTEGER PRIMARY KEY AUTOINCREMENT,
            sender  TEXT NOT NULL,
            kind    TEXT NOT NULL,
            target  TEXT,
            body    TEXT NOT NULL,
            created INTEGER NOT NULL
        );
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS workers (
            name       TEXT PRIMARY KEY,
            role       TEXT NOT NULL DEFAULT '',
            program    TEXT NOT NULL,
            args       TEXT NOT NULL,
            cwd        TEXT NOT NULL,
            keep_alive INTEGER NOT NULL DEFAULT 1,
            session_id TEXT,
            created    INTEGER NOT NULL
        );
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_messages_target ON messages(target, id)")
        .execute(pool)
        .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_subs_channel ON subs(channel)")
        .execute(pool)
        .await?;

    Ok(())
}

pub async fn max_message_id(pool: &SqlitePool) -> Result<i64> {
    let row: (i64,) = sqlx::query_as("SELECT COALESCE(MAX(id), 0) FROM messages")
        .fetch_one(pool)
        .await?;
    Ok(row.0)
}

/// Insert an agent, preserving its cursor on re-registration.
pub async fn upsert_agent(pool: &SqlitePool, name: &str, role: &str, caps: &str) -> Result<()> {
    let start = max_message_id(pool).await?;
    sqlx::query(
        r#"
        INSERT INTO agents (name, role, caps, cursor, online, last_seen)
        VALUES (?1, ?2, ?3, ?4, 1, ?5)
        ON CONFLICT(name) DO UPDATE SET
            role = excluded.role,
            caps = excluded.caps,
            online = 1,
            last_seen = excluded.last_seen
        "#,
    )
    .bind(name)
    .bind(role)
    .bind(caps)
    .bind(start)
    .bind(now())
    .execute(pool)
    .await?;
    Ok(())
}

/// Pre-create a placeholder row for a not-yet-registered agent so a direct
/// message queued *before* it registers is still delivered once it does. The
/// read cursor is seeded to the current message tip (captured before the
/// triggering message is inserted) and `online = 0` marks it as not-yet-present.
/// `INSERT OR IGNORE` never disturbs an already-registered agent, and
/// `upsert_agent` preserves the cursor on conflict, so the later real register
/// keeps this low cursor and delivers the queued message.
pub async fn ensure_agent(pool: &SqlitePool, name: &str) -> Result<()> {
    let start = max_message_id(pool).await?;
    sqlx::query(
        "INSERT OR IGNORE INTO agents (name, role, caps, cursor, online, last_seen) VALUES (?1, '', '', ?2, 0, ?3)",
    )
    .bind(name)
    .bind(start)
    .bind(now())
    .execute(pool)
    .await?;
    Ok(())
}

/// Bump an agent's `last_seen` to now — a heartbeat proving its process is alive
/// and looping. Called on every MCP tool call, so an agent that stops making
/// calls (and is not parked on `wait`) ages out of the live set.
pub async fn touch(pool: &SqlitePool, name: &str) -> Result<()> {
    sqlx::query("UPDATE agents SET last_seen = ?2 WHERE name = ?1")
        .bind(name)
        .bind(now())
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn insert_message(
    pool: &SqlitePool,
    sender: &str,
    kind: &str,
    target: Option<&str>,
    body: &str,
) -> Result<i64> {
    let res = sqlx::query(
        "INSERT INTO messages (sender, kind, target, body, created) VALUES (?1, ?2, ?3, ?4, ?5)",
    )
    .bind(sender)
    .bind(kind)
    .bind(target)
    .bind(body)
    .bind(now())
    .execute(pool)
    .await?;
    prune_messages(pool).await?;
    Ok(res.last_insert_rowid())
}

/// Trim the table to the most recent `MESSAGE_RETENTION` rows.
async fn prune_messages(pool: &SqlitePool) -> Result<()> {
    sqlx::query("DELETE FROM messages WHERE id <= (SELECT MAX(id) FROM messages) - ?1")
        .bind(MESSAGE_RETENTION)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn cursor_of(pool: &SqlitePool, name: &str) -> Result<i64> {
    let row: Option<(i64,)> = sqlx::query_as("SELECT cursor FROM agents WHERE name = ?1")
        .bind(name)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| r.0).unwrap_or(0))
}

pub async fn advance_cursor(pool: &SqlitePool, name: &str, id: i64) -> Result<()> {
    sqlx::query("UPDATE agents SET cursor = ?2, last_seen = ?3 WHERE name = ?1")
        .bind(name)
        .bind(id)
        .bind(now())
        .execute(pool)
        .await?;
    Ok(())
}

/// Messages addressed to `name` with id greater than `cursor`.
pub async fn pending_for(pool: &SqlitePool, name: &str, cursor: i64) -> Result<Vec<Message>> {
    let rows = sqlx::query_as::<_, Message>(
        r#"
        SELECT id, sender, kind, target, body, created
        FROM messages
        WHERE id > ?2
          AND sender != ?1
          AND (
                (kind = 'direct'    AND target = ?1)
             OR (kind = 'broadcast')
             OR (kind = 'channel'   AND target IN (SELECT channel FROM subs WHERE agent = ?1))
          )
        ORDER BY id ASC
        "#,
    )
    .bind(name)
    .bind(cursor)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Messages after `since` (ascending); falls back to the last `limit` when since<=0.
pub async fn since(pool: &SqlitePool, since: i64, limit: i64) -> Result<Vec<Message>> {
    if since <= 0 {
        return recent(pool, limit).await;
    }
    let rows = sqlx::query_as::<_, Message>(
        "SELECT id, sender, kind, target, body, created FROM messages WHERE id > ?1 ORDER BY id ASC LIMIT 1000",
    )
    .bind(since)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Most recent messages (ascending), for the dashboard feed.
pub async fn recent(pool: &SqlitePool, limit: i64) -> Result<Vec<Message>> {
    let rows = sqlx::query_as::<_, Message>(
        r#"
        SELECT id, sender, kind, target, body, created FROM messages
        WHERE id > (SELECT COALESCE(MAX(id), 0) - ?1 FROM messages)
        ORDER BY id ASC
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn subscribe(pool: &SqlitePool, agent: &str, channel: &str) -> Result<()> {
    sqlx::query("INSERT OR IGNORE INTO subs (agent, channel) VALUES (?1, ?2)")
        .bind(agent)
        .bind(channel)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn unsubscribe(pool: &SqlitePool, agent: &str, channel: &str) -> Result<()> {
    sqlx::query("DELETE FROM subs WHERE agent = ?1 AND channel = ?2")
        .bind(agent)
        .bind(channel)
        .execute(pool)
        .await?;
    Ok(())
}

/// Agents currently subscribed to `channel` (for targeted wakeups).
pub async fn channel_subs(pool: &SqlitePool, channel: &str) -> Result<Vec<String>> {
    let rows: Vec<(String,)> = sqlx::query_as("SELECT agent FROM subs WHERE channel = ?1")
        .bind(channel)
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(|r| r.0).collect())
}

pub async fn subs_of(pool: &SqlitePool, agent: &str) -> Result<Vec<String>> {
    let rows: Vec<(String,)> = sqlx::query_as("SELECT channel FROM subs WHERE agent = ?1 ORDER BY channel")
        .bind(agent)
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(|r| r.0).collect())
}

/// (name, role, registered, channel_count, last_seen) for every known agent.
/// `registered` is the stored online bit — false for a not-yet-registered
/// placeholder (see [`ensure_agent`]); true liveness is computed by the caller
/// from `last_seen` plus the in-memory parked set (see [`crate::state::App`]).
pub async fn list_agents(pool: &SqlitePool) -> Result<Vec<(String, String, bool, i64, i64)>> {
    let rows: Vec<(String, String, i64, i64, i64)> = sqlx::query_as(
        r#"
        SELECT a.name, a.role, a.online,
               (SELECT COUNT(*) FROM subs s WHERE s.agent = a.name) AS chans,
               a.last_seen
        FROM agents a
        ORDER BY a.online DESC, a.name ASC
        "#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(n, r, o, c, ls)| (n, r, o != 0, c, ls))
        .collect())
}

/// A background worker persisted across daemon restarts (issue #4).
pub struct PersistedWorker {
    pub name: String,
    pub role: String,
    pub program: String,
    pub args: Vec<String>,
    pub cwd: String,
    pub keep_alive: bool,
    pub session_id: Option<String>,
}

type WorkerRow = (String, String, String, String, String, i64, Option<String>);

/// Persist a spawned worker so a restarted daemon can bring it back. `args` is
/// stored as a JSON array. `INSERT OR REPLACE` keeps it idempotent per name.
pub async fn save_worker(pool: &SqlitePool, w: &PersistedWorker) -> Result<()> {
    let args_json = serde_json::to_string(&w.args).unwrap_or_else(|_| "[]".into());
    sqlx::query(
        "INSERT OR REPLACE INTO workers (name, role, program, args, cwd, keep_alive, session_id, created) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )
    .bind(&w.name)
    .bind(&w.role)
    .bind(&w.program)
    .bind(args_json)
    .bind(&w.cwd)
    .bind(w.keep_alive as i64)
    .bind(&w.session_id)
    .bind(now())
    .execute(pool)
    .await?;
    Ok(())
}

/// Forget a worker (explicit stop, one-shot completion, or terminal failure).
pub async fn delete_worker(pool: &SqlitePool, name: &str) -> Result<()> {
    sqlx::query("DELETE FROM workers WHERE name = ?1")
        .bind(name)
        .execute(pool)
        .await?;
    Ok(())
}

/// Every persisted worker, for rehydration on daemon startup.
pub async fn load_workers(pool: &SqlitePool) -> Result<Vec<PersistedWorker>> {
    let rows: Vec<WorkerRow> = sqlx::query_as(
        "SELECT name, role, program, args, cwd, keep_alive, session_id FROM workers",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(name, role, program, args, cwd, ka, session_id)| PersistedWorker {
            name,
            role,
            program,
            args: serde_json::from_str(&args).unwrap_or_default(),
            cwd,
            keep_alive: ka != 0,
            session_id,
        })
        .collect())
}

/// (channel, subscriber_count) across the mesh.
pub async fn list_channels(pool: &SqlitePool) -> Result<Vec<(String, i64)>> {
    let rows: Vec<(String, i64)> =
        sqlx::query_as("SELECT channel, COUNT(*) FROM subs GROUP BY channel ORDER BY channel")
            .fetch_all(pool)
            .await?;
    Ok(rows)
}
