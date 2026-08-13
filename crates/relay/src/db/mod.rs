use crate::protocol::{now, Message};
use anyhow::Result;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;

/// Messages kept for the dashboard feed once every registered reader has
/// consumed them.
const MESSAGE_RETENTION: i64 = 10_000;

/// Absolute backlog bound. A lagging reader's unread messages are preserved
/// past `MESSAGE_RETENTION`, but never beyond this: past it the prune floor
/// moves anyway and the lagging cursor is bumped to the floor (see
/// [`prune_floor`]).
const MAX_BACKLOG: i64 = 50_000;

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
            last_seen INTEGER NOT NULL DEFAULT 0,
            status    TEXT NOT NULL DEFAULT ''
        );
        "#,
    )
    .execute(pool)
    .await?;

    // Bring an older DB up to date: add the semantic work-state column if a
    // prior version created the table without it. A duplicate-column error just
    // means it is already there, so the result is ignored.
    let _ = sqlx::query("ALTER TABLE agents ADD COLUMN status TEXT NOT NULL DEFAULT ''")
        .execute(pool)
        .await;

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

/// Record an agent's self-reported semantic work state ("working", "idle",
/// "blocked", "done", or a custom label). Also bumps `last_seen`, since a report
/// proves the agent is alive. Pre-creates a placeholder row if the agent has not
/// registered yet, so a status set before `register` is not lost.
pub async fn set_status(pool: &SqlitePool, name: &str, status: &str) -> Result<()> {
    ensure_agent(pool, name).await?;
    sqlx::query("UPDATE agents SET status = ?2, last_seen = ?3 WHERE name = ?1")
        .bind(name)
        .bind(status)
        .bind(now())
        .execute(pool)
        .await?;
    Ok(())
}

/// An agent's last-reported semantic status, or empty when none/unknown.
pub async fn status_of(pool: &SqlitePool, name: &str) -> Result<String> {
    let row: Option<(String,)> = sqlx::query_as("SELECT status FROM agents WHERE name = ?1")
        .bind(name)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| r.0).unwrap_or_default())
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

/// Trim delivered history. Normally nothing above the slowest registered
/// reader's cursor is touched (unread messages survive past the feed
/// retention), but `MAX_BACKLOG` is a hard bound: when it forces the floor past
/// a cursor, that cursor is bumped to the floor so the reader's next drain sees
/// a consistent (if gappy) stream, and the gap is logged.
async fn prune_messages(pool: &SqlitePool) -> Result<()> {
    let max = max_message_id(pool).await?;
    if max <= MESSAGE_RETENTION {
        return Ok(());
    }
    let row: (Option<i64>,) = sqlx::query_as("SELECT MIN(cursor) FROM agents WHERE online = 1")
        .fetch_one(pool)
        .await?;
    let floor = prune_floor(max, row.0);
    if floor <= 0 {
        return Ok(());
    }
    let bumped = sqlx::query("UPDATE agents SET cursor = ?1 WHERE cursor < ?1")
        .bind(floor)
        .execute(pool)
        .await?;
    if bumped.rows_affected() > 0 {
        tracing::warn!(
            "relay: prune floor {floor} passed {} reader cursor(s); their older messages were dropped undelivered",
            bumped.rows_affected()
        );
    }
    sqlx::query("DELETE FROM messages WHERE id <= ?1")
        .bind(floor)
        .execute(pool)
        .await?;
    Ok(())
}

/// Highest message id safe to delete: everything the slowest registered reader
/// has consumed, keeping at least `MESSAGE_RETENTION` rows for the feed, but
/// never letting the backlog exceed `MAX_BACKLOG`.
fn prune_floor(max_id: i64, min_cursor: Option<i64>) -> i64 {
    let retention = max_id - MESSAGE_RETENTION;
    let soft = min_cursor.map_or(retention, |c| retention.min(c));
    soft.max(max_id - MAX_BACKLOG)
}

/// Confirm delivery of everything up to `id`: advance the read cursor
/// monotonically (a stale ack can never rewind it) and refresh liveness. This
/// is the only writer of the cursor outside pruning — see the delivery
/// contract in [`crate::bus`].
pub async fn ack_delivered(pool: &SqlitePool, name: &str, id: i64) -> Result<()> {
    sqlx::query("UPDATE agents SET cursor = MAX(cursor, ?2), last_seen = ?3 WHERE name = ?1")
        .bind(name)
        .bind(id)
        .bind(now())
        .execute(pool)
        .await?;
    Ok(())
}

/// Messages addressed to `name` beyond its read cursor. Reads the cursor and
/// scans in one statement, so a concurrent [`ack_delivered`] cannot interleave
/// between them; because the cursor only moves on ack, an unacknowledged
/// delivery is simply read again here.
pub async fn pending_for(pool: &SqlitePool, name: &str) -> Result<Vec<Message>> {
    let rows = sqlx::query_as::<_, Message>(
        r#"
        SELECT id, sender, kind, target, body, created
        FROM messages
        WHERE id > COALESCE((SELECT cursor FROM agents WHERE name = ?1), 0)
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
        "SELECT id, sender, kind, target, body, created FROM messages WHERE id > ?1 ORDER BY id ASC LIMIT ?2",
    )
    .bind(since)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Earliest `last_seen` among agents still inside the liveness window (i.e.
/// counted online by recency), or `None` when every agent is already outside
/// it. `horizon` is `now - ACTIVE_WINDOW`; the caller sleeps until the returned
/// agent's window lapses and re-emits the roster then.
pub async fn next_expiry(pool: &SqlitePool, horizon: i64) -> Result<Option<i64>> {
    let row: (Option<i64>,) =
        sqlx::query_as("SELECT MIN(last_seen) FROM agents WHERE last_seen > ?1")
            .bind(horizon)
            .fetch_one(pool)
            .await?;
    Ok(row.0)
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
    let rows: Vec<(String,)> =
        sqlx::query_as("SELECT channel FROM subs WHERE agent = ?1 ORDER BY channel")
            .bind(agent)
            .fetch_all(pool)
            .await?;
    Ok(rows.into_iter().map(|r| r.0).collect())
}

/// (name, role, status, registered, channel_count, last_seen) for every known
/// agent. `registered` is the stored online bit — false for a not-yet-registered
/// placeholder (see [`ensure_agent`]); true liveness is computed by the caller
/// from `last_seen` plus the in-memory parked set (see [`crate::state::App`]).
/// `status` is the agent's last self-reported semantic work state (empty when
/// none).
pub async fn list_agents(
    pool: &SqlitePool,
) -> Result<Vec<(String, String, String, bool, i64, i64)>> {
    let rows: Vec<(String, String, String, i64, i64, i64)> = sqlx::query_as(
        r#"
        SELECT a.name, a.role, a.status, a.online,
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
        .map(|(n, r, st, o, c, ls)| (n, r, st, o != 0, c, ls))
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
        .map(
            |(name, role, program, args, cwd, ka, session_id)| PersistedWorker {
                name,
                role,
                program,
                args: serde_json::from_str(&args).unwrap_or_default(),
                cwd,
                keep_alive: ka != 0,
                session_id,
            },
        )
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

#[cfg(test)]
mod tests {
    use super::*;

    async fn pool() -> (SqlitePool, std::path::PathBuf) {
        static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!("relay-db-{}-{n}.db", std::process::id()));
        (open(path.to_str().unwrap()).await.unwrap(), path)
    }

    fn cleanup(path: &std::path::Path) {
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(path.with_extension("db-wal"));
        let _ = std::fs::remove_file(path.with_extension("db-shm"));
    }

    async fn insert_with_id(pool: &SqlitePool, id: i64) {
        sqlx::query(
            "INSERT INTO messages (id, sender, kind, target, body, created) VALUES (?1, 's', 'broadcast', NULL, 'm', 0)",
        )
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn cursor(pool: &SqlitePool, name: &str) -> i64 {
        let row: (i64,) = sqlx::query_as("SELECT cursor FROM agents WHERE name = ?1")
            .bind(name)
            .fetch_one(pool)
            .await
            .unwrap();
        row.0
    }

    async fn ids(pool: &SqlitePool) -> Vec<i64> {
        let rows: Vec<(i64,)> = sqlx::query_as("SELECT id FROM messages ORDER BY id")
            .fetch_all(pool)
            .await
            .unwrap();
        rows.into_iter().map(|r| r.0).collect()
    }

    /// `since` once hardcoded LIMIT 1000 and ignored the caller's limit.
    #[tokio::test]
    async fn since_binds_the_limit() {
        let (pool, path) = pool().await;
        for _ in 0..5 {
            insert_message(&pool, "s", "broadcast", None, "m")
                .await
                .unwrap();
        }
        let first = ids(&pool).await[0];
        let msgs = since(&pool, first, 2).await.unwrap();
        assert_eq!(
            msgs.len(),
            2,
            "the limit parameter must apply on the since>0 branch"
        );
        cleanup(&path);
    }

    /// The floor math: unread messages hold pruning below the feed retention,
    /// and the hard backlog cap wins over a hopeless straggler.
    #[test]
    fn prune_floor_respects_cursors_up_to_the_hard_cap() {
        // Everyone caught up: the feed retention rules.
        assert_eq!(
            prune_floor(20_000, Some(20_000)),
            20_000 - MESSAGE_RETENTION
        );
        // No registered readers: same.
        assert_eq!(prune_floor(20_000, None), 20_000 - MESSAGE_RETENTION);
        // A lagging reader holds the floor at its cursor.
        assert_eq!(prune_floor(20_000, Some(5)), 5);
        // ...but never past the absolute backlog bound.
        assert_eq!(prune_floor(100_000, Some(5)), 100_000 - MAX_BACKLOG);
    }

    /// Pruning once deleted below MAX(id)-retention regardless of reader
    /// cursors — a parked agent more than a retention behind lost its backlog.
    /// Its unread messages must survive now.
    #[tokio::test]
    async fn prune_preserves_a_lagging_readers_backlog() {
        let (pool, path) = pool().await;
        upsert_agent(&pool, "slow", "worker", "").await.unwrap();
        // upsert seeds the cursor at the current tip (0 here).
        for id in [1, 2, 3] {
            insert_with_id(&pool, id).await;
        }
        ack_delivered(&pool, "slow", 2).await.unwrap();
        // Jump the tip far past the retention window, then insert normally so
        // pruning runs.
        insert_with_id(&pool, MESSAGE_RETENTION + 10).await;
        insert_message(&pool, "s", "broadcast", None, "tip")
            .await
            .unwrap();
        let remaining = ids(&pool).await;
        assert!(
            remaining.contains(&3),
            "message 3 is unread by 'slow' and inside the backlog bound — it must survive"
        );
        assert!(
            !remaining.contains(&1),
            "consumed history below the cursor is pruned"
        );
        assert_eq!(
            cursor(&pool, "slow").await,
            2,
            "a preserved backlog leaves the cursor alone"
        );
        cleanup(&path);
    }

    /// When the hard backlog cap forces the floor past a cursor, the cursor is
    /// bumped to the floor so the reader's next drain is consistent (gappy, not
    /// silently rewound or replayed).
    #[tokio::test]
    async fn hard_cap_bumps_a_hopelessly_lagging_cursor() {
        let (pool, path) = pool().await;
        upsert_agent(&pool, "slow", "worker", "").await.unwrap();
        insert_with_id(&pool, 1).await;
        insert_with_id(&pool, MAX_BACKLOG + 100).await;
        insert_message(&pool, "s", "broadcast", None, "tip")
            .await
            .unwrap();
        let floor = prune_floor(MAX_BACKLOG + 101, Some(0));
        assert_eq!(
            cursor(&pool, "slow").await,
            floor,
            "the straggler's cursor moves to the floor"
        );
        assert!(
            !ids(&pool).await.contains(&1),
            "its lost backlog is actually pruned"
        );
        cleanup(&path);
    }
}
