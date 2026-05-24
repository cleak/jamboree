use rusqlite::Connection;

/// Initialize the database schema, applying migrations as needed.
pub(crate) fn ensure_schema(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch("PRAGMA journal_mode = WAL;")?;
    conn.execute_batch("PRAGMA synchronous = NORMAL;")?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;

    let current = conn.pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))?;

    if current == 0 {
        create_v1(conn)?;
    }
    Ok(())
}

fn create_v1(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "
        -- Append-only event log, one row per domain event.
        -- (stream_id, version) is the optimistic concurrency key.
        CREATE TABLE IF NOT EXISTS task_events (
            stream_id       TEXT    NOT NULL,
            version         INTEGER NOT NULL,
            event_type      TEXT    NOT NULL,
            payload         TEXT    NOT NULL,
            idempotency_key TEXT,
            trace_id        TEXT    NOT NULL,
            timestamp       TEXT    NOT NULL,
            created_at      TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
            PRIMARY KEY (stream_id, version)
        );

        -- Fast lookup for idempotency checks.
        CREATE UNIQUE INDEX IF NOT EXISTS idx_task_events_idempotency
            ON task_events (idempotency_key)
            WHERE idempotency_key IS NOT NULL;

        -- Aggregate snapshots for fast rebuilds.
        CREATE TABLE IF NOT EXISTS task_snapshots (
            stream_id       TEXT    NOT NULL,
            version         INTEGER NOT NULL,
            state           TEXT    NOT NULL,
            created_at      TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
            PRIMARY KEY (stream_id, version)
        );

        -- Materialized projection: current state of every task.
        -- Updated synchronously on each append within the same transaction.
        CREATE TABLE IF NOT EXISTS task_state (
            task_id             TEXT    PRIMARY KEY,
            status              TEXT    NOT NULL,
            version             INTEGER NOT NULL,
            description         TEXT,
            project             TEXT,
            task_class          TEXT,
            priority            TEXT,
            current_session_id  TEXT,
            current_harness     TEXT,
            worktree_path       TEXT,
            pr_ref              TEXT,
            pr_branch           TEXT,
            pr_title            TEXT,
            pr_draft            INTEGER,
            ci_status           TEXT,
            last_reviewer       TEXT,
            continuation_count  INTEGER NOT NULL DEFAULT 0,
            post_pr_continuations INTEGER NOT NULL DEFAULT 0,
            outcome             TEXT,
            failure_reason      TEXT,
            requested_by        TEXT,
            trace_id            TEXT,
            requested_at        TEXT,
            updated_at          TEXT    NOT NULL
        );

        -- Idempotency key tracking with TTL for garbage collection.
        CREATE TABLE IF NOT EXISTS idempotency_keys (
            key         TEXT    PRIMARY KEY,
            stream_id   TEXT    NOT NULL,
            version     INTEGER NOT NULL,
            created_at  TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
            expires_at  TEXT    NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_idempotency_expires
            ON idempotency_keys (expires_at);

        -- Track schema version
        PRAGMA user_version = 1;
        ",
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_is_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        ensure_schema(&conn).unwrap();
        ensure_schema(&conn).unwrap();
        let ver = conn
            .pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))
            .unwrap();
        assert_eq!(ver, 1);
    }
}
