use std::path::Path;

use chrono::{Duration, Utc};
use jam_task_core::{ApplyOutcome, TaskAggregate, TaskEvent};
use rusqlite::{params, Connection, OptionalExtension};
use tracing::{debug, warn};

use crate::query::TaskSummary;
use crate::schema;

/// Errors from the event store.
#[derive(Debug, thiserror::Error)]
pub enum AppendError {
    /// Optimistic concurrency violation: expected version doesn't match.
    #[error("version conflict on task {task_id}: expected {expected}, found {actual}")]
    VersionConflict {
        /// Task ID.
        task_id: String,
        /// The version the caller expected.
        expected: u64,
        /// The version currently in the store.
        actual: u64,
    },

    /// Duplicate idempotency key — this event was already processed.
    #[error("duplicate idempotency key: {key}")]
    DuplicateIdempotencyKey {
        /// The duplicate key.
        key: String,
    },

    /// Domain error from the aggregate.
    #[error("aggregate: {0}")]
    Aggregate(#[from] jam_task_core::ApplyError),

    /// SQLite error.
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),

    /// Serialization error.
    #[error("serde: {0}")]
    Serde(#[from] serde_json::Error),
}

/// Durable event store for task aggregates.
///
/// All operations are transactional. The store maintains:
/// - An append-only event log with optimistic concurrency
/// - A materialized `task_state` projection updated on each append
/// - Idempotency key tracking with TTL
/// - Optional aggregate snapshots for fast rebuilds
pub struct TaskStore {
    conn: Connection,
}

impl TaskStore {
    /// Open or create a task store at the given path.
    pub fn open(path: &Path) -> Result<Self, AppendError> {
        let conn = Connection::open(path)?;
        schema::ensure_schema(&conn)?;
        Ok(Self { conn })
    }

    /// Create an in-memory store (for testing).
    pub fn in_memory() -> Result<Self, AppendError> {
        let conn = Connection::open_in_memory()?;
        schema::ensure_schema(&conn)?;
        Ok(Self { conn })
    }

    /// Append an event to a task's stream.
    ///
    /// - `expected_version`: the version the caller believes the aggregate is at.
    ///   Pass 0 for the first event. If the actual version differs, returns
    ///   `AppendError::VersionConflict`.
    /// - `idempotency_key`: optional dedup key. If the same key was already
    ///   used, returns `AppendError::DuplicateIdempotencyKey`.
    /// - Returns the `ApplyOutcome` from the aggregate.
    pub fn append(
        &mut self,
        task_id: &str,
        event: &TaskEvent,
        trace_id: &str,
        expected_version: u64,
        idempotency_key: Option<&str>,
    ) -> Result<ApplyOutcome, AppendError> {
        let tx = self.conn.transaction()?;

        // 1. Check idempotency key
        if let Some(key) = idempotency_key {
            let existing: Option<String> = tx
                .query_row(
                    "SELECT stream_id FROM idempotency_keys WHERE key = ?1",
                    params![key],
                    |row| row.get(0),
                )
                .optional()?;
            if existing.is_some() {
                return Err(AppendError::DuplicateIdempotencyKey {
                    key: key.to_owned(),
                });
            }
        }

        // 2. Check optimistic concurrency
        let actual_version: u64 = tx
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM task_events WHERE stream_id = ?1",
                params![task_id],
                |row| row.get(0),
            )?;
        if actual_version != expected_version {
            return Err(AppendError::VersionConflict {
                task_id: task_id.to_owned(),
                expected: expected_version,
                actual: actual_version,
            });
        }

        // 3. Load aggregate (from snapshot + events)
        let mut agg = load_aggregate_in_tx(&tx, task_id, trace_id)?;

        // 4. Apply event to aggregate
        let outcome = agg.apply(event)?;

        // 5. Persist event
        let new_version = actual_version + 1;
        let payload = serde_json::to_string(event)?;
        tx.execute(
            "INSERT INTO task_events (stream_id, version, event_type, payload, idempotency_key, trace_id, timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                task_id,
                new_version,
                event.event_type(),
                payload,
                idempotency_key,
                trace_id,
                event.timestamp().to_rfc3339(),
            ],
        )?;

        // 6. Record idempotency key
        if let Some(key) = idempotency_key {
            let expires = Utc::now() + Duration::hours(24);
            tx.execute(
                "INSERT INTO idempotency_keys (key, stream_id, version, expires_at)
                 VALUES (?1, ?2, ?3, ?4)",
                params![key, task_id, new_version, expires.to_rfc3339()],
            )?;
        }

        // 7. Update materialized projection
        upsert_task_state(&tx, &agg)?;

        // 8. Optionally take snapshot (every 20 events)
        if new_version.is_multiple_of(20) {
            let state = serde_json::to_string(&agg)?;
            tx.execute(
                "INSERT OR REPLACE INTO task_snapshots (stream_id, version, state)
                 VALUES (?1, ?2, ?3)",
                params![task_id, new_version, state],
            )?;
            debug!(task = %task_id, version = new_version, "snapshot taken");
        }

        tx.commit()?;
        Ok(outcome)
    }

    /// Load the current aggregate for a task.
    ///
    /// Returns `None` if the task doesn't exist.
    pub fn load(&self, task_id: &str) -> Result<Option<TaskAggregate>, AppendError> {
        let has_events: bool = self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM task_events WHERE stream_id = ?1)",
            params![task_id],
            |row| row.get(0),
        )?;
        if !has_events {
            return Ok(None);
        }
        let trace_id = self
            .conn
            .query_row(
                "SELECT trace_id FROM task_events WHERE stream_id = ?1 ORDER BY version ASC LIMIT 1",
                params![task_id],
                |row| row.get::<_, String>(0),
            )?;
        Ok(Some(load_aggregate_in_tx(
            &self.conn,
            task_id,
            &trace_id,
        )?))
    }

    /// List all tasks matching a filter, using the materialized projection.
    pub fn list(&self, filter: &crate::query::TaskFilter) -> Result<Vec<TaskSummary>, AppendError> {
        crate::query::list_tasks(&self.conn, filter)
    }

    /// Get a single task summary from the materialized projection.
    pub fn get_summary(&self, task_id: &str) -> Result<Option<TaskSummary>, AppendError> {
        crate::query::get_task(&self.conn, task_id)
    }

    /// Get all events for a task, in version order.
    pub fn events(&self, task_id: &str) -> Result<Vec<StoredEvent>, AppendError> {
        let mut stmt = self.conn.prepare(
            "SELECT version, event_type, payload, trace_id, timestamp, idempotency_key
             FROM task_events WHERE stream_id = ?1 ORDER BY version ASC",
        )?;
        let rows = stmt.query_map(params![task_id], |row| {
            Ok(StoredEvent {
                version: row.get(0)?,
                event_type: row.get(1)?,
                payload: row.get(2)?,
                trace_id: row.get(3)?,
                timestamp: row.get(4)?,
                idempotency_key: row.get(5)?,
            })
        })?;
        let mut events = Vec::new();
        for row in rows {
            events.push(row?);
        }
        Ok(events)
    }

    /// Garbage-collect expired idempotency keys.
    pub fn gc_idempotency_keys(&self) -> Result<usize, AppendError> {
        let now = Utc::now().to_rfc3339();
        let deleted =
            self.conn
                .execute("DELETE FROM idempotency_keys WHERE expires_at < ?1", params![now])?;
        if deleted > 0 {
            debug!(deleted, "garbage-collected expired idempotency keys");
        }
        Ok(deleted)
    }
}

/// A persisted event record.
#[derive(Debug)]
pub struct StoredEvent {
    /// Event version within the stream.
    pub version: u64,
    /// Event type name.
    pub event_type: String,
    /// JSON-serialized event payload.
    pub payload: String,
    /// Trace ID.
    pub trace_id: String,
    /// Event timestamp (RFC 3339).
    pub timestamp: String,
    /// Idempotency key, if set.
    pub idempotency_key: Option<String>,
}

fn load_aggregate_in_tx(
    conn: &Connection,
    task_id: &str,
    trace_id: &str,
) -> Result<TaskAggregate, AppendError> {
    // Try to load from latest snapshot
    let snapshot: Option<(u64, String)> = conn
        .query_row(
            "SELECT version, state FROM task_snapshots
             WHERE stream_id = ?1 ORDER BY version DESC LIMIT 1",
            params![task_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;

    let (mut agg, replay_from) = if let Some((snap_version, state)) = snapshot {
        match serde_json::from_str::<TaskAggregate>(&state) {
            Ok(agg) => (agg, snap_version + 1),
            Err(err) => {
                warn!(task = %task_id, error = %err, "corrupt snapshot, replaying from scratch");
                (TaskAggregate::new(task_id, trace_id), 1)
            }
        }
    } else {
        (TaskAggregate::new(task_id, trace_id), 1)
    };

    // Replay events from snapshot version onward
    let mut stmt = conn.prepare(
        "SELECT payload FROM task_events
         WHERE stream_id = ?1 AND version >= ?2
         ORDER BY version ASC",
    )?;
    let rows = stmt.query_map(params![task_id, replay_from], |row| {
        row.get::<_, String>(0)
    })?;

    for row in rows {
        let payload = row?;
        let event: TaskEvent = serde_json::from_str(&payload)?;
        agg.apply(&event)?;
    }

    Ok(agg)
}

fn upsert_task_state(conn: &Connection, agg: &TaskAggregate) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO task_state (
            task_id, status, version, description, project, task_class, priority,
            current_session_id, current_harness, worktree_path,
            pr_ref, pr_branch, pr_title, pr_draft, ci_status, last_reviewer,
            continuation_count, post_pr_continuations, outcome, failure_reason,
            requested_by, trace_id, requested_at, updated_at
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
            ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20,
            ?21, ?22, ?23, ?24
        )
        ON CONFLICT (task_id) DO UPDATE SET
            status = excluded.status,
            version = excluded.version,
            description = excluded.description,
            project = excluded.project,
            task_class = excluded.task_class,
            priority = excluded.priority,
            current_session_id = excluded.current_session_id,
            current_harness = excluded.current_harness,
            worktree_path = excluded.worktree_path,
            pr_ref = excluded.pr_ref,
            pr_branch = excluded.pr_branch,
            pr_title = excluded.pr_title,
            pr_draft = excluded.pr_draft,
            ci_status = excluded.ci_status,
            last_reviewer = excluded.last_reviewer,
            continuation_count = excluded.continuation_count,
            post_pr_continuations = excluded.post_pr_continuations,
            outcome = excluded.outcome,
            failure_reason = excluded.failure_reason,
            requested_by = excluded.requested_by,
            trace_id = excluded.trace_id,
            requested_at = excluded.requested_at,
            updated_at = excluded.updated_at",
        params![
            agg.id(),
            agg.status().legacy_name(),
            agg.version(),
            agg.description(),
            agg.project(),
            agg.task_class(),
            agg.priority().to_string(),
            agg.current_session_id(),
            agg.current_harness(),
            agg.worktree_path(),
            agg.pr().map(|p| p.pr_ref.as_str()),
            agg.pr().map(|p| p.branch.as_str()),
            agg.pr().map(|p| p.title.as_str()),
            agg.pr().map(|p| p.draft),
            agg.pr().and_then(|p| p.ci_status.as_deref()),
            agg.pr().and_then(|p| p.last_reviewer.as_deref()),
            agg.total_continuations(),
            agg.post_pr_continuations(),
            agg.outcome(),
            agg.failure_reason(),
            agg.requested_by(),
            agg.trace_id(),
            agg.requested_at().to_rfc3339(),
            agg.updated_at().to_rfc3339(),
        ],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use jam_task_core::{Priority, TaskEvent, TaskStatus};

    use super::*;

    fn ts(s: &str) -> chrono::DateTime<Utc> {
        chrono::DateTime::parse_from_rfc3339(s)
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn append_and_load_round_trip() {
        let mut store = TaskStore::in_memory().unwrap();

        let event = TaskEvent::Requested {
            description: "Add feature X".into(),
            project: "blueberry".into(),
            task_class: "light-edit".into(),
            priority: Priority::Normal,
            requested_by: "human:caleb".into(),
            at: ts("2026-05-24T10:00:00Z"),
        };

        let outcome = store
            .append("task-1", &event, "trace-1", 0, Some("task.requested:trace-1"))
            .unwrap();
        assert!(matches!(outcome, ApplyOutcome::Created));

        let agg = store.load("task-1").unwrap().unwrap();
        assert_eq!(agg.id(), "task-1");
        assert_eq!(agg.status(), TaskStatus::Queued);
        assert_eq!(agg.version(), 1);
    }

    #[test]
    fn version_conflict_detected() {
        let mut store = TaskStore::in_memory().unwrap();

        let event = TaskEvent::Requested {
            description: "Add feature X".into(),
            project: "blueberry".into(),
            task_class: "light-edit".into(),
            priority: Priority::Normal,
            requested_by: "human:caleb".into(),
            at: ts("2026-05-24T10:00:00Z"),
        };

        store.append("task-1", &event, "trace-1", 0, None).unwrap();

        // Try to append with stale version
        let event2 = TaskEvent::PickerAssigned {
            session_id: "session-1".into(),
            harness: "codex-cli".into(),
            worktree_path: "/home/picker/workers/task-1".into(),
            picker_trace_id: "01TRACE001".into(),
            at: ts("2026-05-24T10:01:00Z"),
        };
        let result = store.append("task-1", &event2, "trace-2", 0, None);
        assert!(matches!(result, Err(AppendError::VersionConflict { .. })));

        // Correct version works
        store
            .append("task-1", &event2, "trace-2", 1, None)
            .unwrap();
    }

    #[test]
    fn idempotency_key_prevents_duplicates() {
        let mut store = TaskStore::in_memory().unwrap();

        let event = TaskEvent::Requested {
            description: "Add feature X".into(),
            project: "blueberry".into(),
            task_class: "light-edit".into(),
            priority: Priority::Normal,
            requested_by: "human:caleb".into(),
            at: ts("2026-05-24T10:00:00Z"),
        };

        store
            .append("task-1", &event, "trace-1", 0, Some("key-1"))
            .unwrap();

        // Same key on a different task should also fail
        let event2 = TaskEvent::Requested {
            description: "Other task".into(),
            project: "blueberry".into(),
            task_class: "light-edit".into(),
            priority: Priority::Normal,
            requested_by: "human:caleb".into(),
            at: ts("2026-05-24T10:01:00Z"),
        };
        let result = store.append("task-2", &event2, "trace-2", 0, Some("key-1"));
        assert!(matches!(
            result,
            Err(AppendError::DuplicateIdempotencyKey { .. })
        ));
    }

    #[test]
    fn full_lifecycle_through_store() {
        let mut store = TaskStore::in_memory().unwrap();

        // Requested
        store
            .append(
                "task-1",
                &TaskEvent::Requested {
                    description: "Add feature X".into(),
                    project: "blueberry".into(),
                    task_class: "light-edit".into(),
                    priority: Priority::Normal,
                    requested_by: "human:caleb".into(),
                    at: ts("2026-05-24T10:00:00Z"),
                },
                "trace-1",
                0,
                Some("requested:trace-1"),
            )
            .unwrap();

        // Picker assigned
        store
            .append(
                "task-1",
                &TaskEvent::PickerAssigned {
                    session_id: "codex-cli:01ABC".into(),
                    harness: "codex-cli".into(),
                    worktree_path: "/home/picker/workers/task-1".into(),
                    picker_trace_id: "01TRACE001".into(),
                    at: ts("2026-05-24T10:01:00Z"),
                },
                "trace-2",
                1,
                Some("spawned:trace-2"),
            )
            .unwrap();

        // Picker succeeded
        store
            .append(
                "task-1",
                &TaskEvent::PickerSucceeded {
                    session_id: "codex-cli:01ABC".into(),
                    duration_ms: 60_000,
                    at: ts("2026-05-24T10:05:00Z"),
                },
                "trace-3",
                2,
                Some("exited:trace-3"),
            )
            .unwrap();

        // PR opened
        store
            .append(
                "task-1",
                &TaskEvent::PrOpened {
                    pr_ref: "cleak/blueberry#42".into(),
                    branch: "task/task-1".into(),
                    title: "feat: add feature X".into(),
                    draft: false,
                    at: ts("2026-05-24T10:06:00Z"),
                },
                "trace-4",
                3,
                Some("pr-opened:trace-4"),
            )
            .unwrap();

        // PR merged
        store
            .append(
                "task-1",
                &TaskEvent::PrMerged {
                    pr_ref: "cleak/blueberry#42".into(),
                    merged_sha: "abc123".into(),
                    merged_by: "caleb".into(),
                    at: ts("2026-05-24T10:10:00Z"),
                },
                "trace-5",
                4,
                Some("pr-merged:trace-5"),
            )
            .unwrap();

        // Verify final state
        let agg = store.load("task-1").unwrap().unwrap();
        assert_eq!(agg.status(), TaskStatus::Merged);
        assert!(agg.is_terminal());
        assert_eq!(agg.pr().unwrap().merged_sha.as_deref(), Some("abc123"));

        // Verify materialized projection
        let summary = store.get_summary("task-1").unwrap().unwrap();
        assert_eq!(summary.status, "merged");
        assert_eq!(summary.pr_ref.as_deref(), Some("cleak/blueberry#42"));

        // Verify event history
        let events = store.events("task-1").unwrap();
        assert_eq!(events.len(), 5);
        assert_eq!(events[0].event_type, "requested");
        assert_eq!(events[4].event_type, "pr_merged");
    }

    #[test]
    fn materialized_projection_stays_in_sync() {
        let mut store = TaskStore::in_memory().unwrap();

        store
            .append(
                "task-1",
                &TaskEvent::Requested {
                    description: "Test task".into(),
                    project: "jamboree".into(),
                    task_class: "light-edit".into(),
                    priority: Priority::High,
                    requested_by: "human:caleb".into(),
                    at: ts("2026-05-24T10:00:00Z"),
                },
                "trace-1",
                0,
                None,
            )
            .unwrap();

        let summary = store.get_summary("task-1").unwrap().unwrap();
        assert_eq!(summary.status, "backlog");
        assert_eq!(summary.priority.as_deref(), Some("high"));

        store
            .append(
                "task-1",
                &TaskEvent::PickerAssigned {
                    session_id: "codex-cli:01ABC".into(),
                    harness: "codex-cli".into(),
                    worktree_path: "/home/picker/workers/task-1".into(),
                    picker_trace_id: "01TRACE001".into(),
                    at: ts("2026-05-24T10:01:00Z"),
                },
                "trace-2",
                1,
                None,
            )
            .unwrap();

        let summary = store.get_summary("task-1").unwrap().unwrap();
        assert_eq!(summary.status, "in-progress");
        assert_eq!(summary.current_session_id.as_deref(), Some("codex-cli:01ABC"));
        assert_eq!(summary.current_harness.as_deref(), Some("codex-cli"));
    }

    #[test]
    fn snapshot_accelerates_rebuild() {
        let mut store = TaskStore::in_memory().unwrap();

        // Create task and apply 20 events to trigger snapshot
        store
            .append(
                "task-1",
                &TaskEvent::Requested {
                    description: "Snapshot test".into(),
                    project: "blueberry".into(),
                    task_class: "light-edit".into(),
                    priority: Priority::Normal,
                    requested_by: "human:caleb".into(),
                    at: ts("2026-05-24T10:00:00Z"),
                },
                "trace-1",
                0,
                None,
            )
            .unwrap();

        // Apply 19 more events (picker assign/succeed cycles + continuations)
        for i in 1..20 {
            let session = format!("session-{i}");
            if i % 2 == 1 {
                store
                    .append(
                        "task-1",
                        &TaskEvent::PickerAssigned {
                            session_id: session,
                            harness: "codex-cli".into(),
                            worktree_path: "/home/picker/workers/task-1".into(),
                            picker_trace_id: format!("01TRACE{i:03}"),
                            at: ts("2026-05-24T10:01:00Z"),
                        },
                        &format!("trace-{}", i + 1),
                        i as u64,
                        None,
                    )
                    .unwrap();
            } else {
                store
                    .append(
                        "task-1",
                        &TaskEvent::PickerSucceeded {
                            session_id: format!("session-{}", i - 1),
                            duration_ms: 1000,
                            at: ts("2026-05-24T10:05:00Z"),
                        },
                        &format!("trace-{}", i + 1),
                        i as u64,
                        None,
                    )
                    .unwrap();
            }
        }

        // Verify snapshot exists
        let has_snapshot: bool = store
            .conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM task_snapshots WHERE stream_id = 'task-1')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(has_snapshot);

        // Load should still work correctly
        let agg = store.load("task-1").unwrap().unwrap();
        assert_eq!(agg.version(), 20);
    }

    #[test]
    fn gc_removes_expired_keys() {
        let mut store = TaskStore::in_memory().unwrap();

        store
            .append(
                "task-1",
                &TaskEvent::Requested {
                    description: "GC test".into(),
                    project: "blueberry".into(),
                    task_class: "light-edit".into(),
                    priority: Priority::Normal,
                    requested_by: "human:caleb".into(),
                    at: ts("2026-05-24T10:00:00Z"),
                },
                "trace-1",
                0,
                Some("key-1"),
            )
            .unwrap();

        // Manually expire the key
        store
            .conn
            .execute(
                "UPDATE idempotency_keys SET expires_at = '2020-01-01T00:00:00Z' WHERE key = 'key-1'",
                [],
            )
            .unwrap();

        let deleted = store.gc_idempotency_keys().unwrap();
        assert_eq!(deleted, 1);
    }

    #[test]
    fn nonexistent_task_returns_none() {
        let store = TaskStore::in_memory().unwrap();
        assert!(store.load("no-such-task").unwrap().is_none());
        assert!(store.get_summary("no-such-task").unwrap().is_none());
    }
}
