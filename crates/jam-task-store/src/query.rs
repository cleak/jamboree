use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use crate::store::AppendError;

/// Filter for querying the task list.
#[derive(Debug, Default)]
pub struct TaskFilter {
    /// Filter by status (legacy name).
    pub status: Option<String>,
    /// Filter by project.
    pub project: Option<String>,
    /// Limit results.
    pub limit: Option<u32>,
    /// Include only non-terminal tasks.
    pub active_only: bool,
}

/// Summary of a task from the materialized projection.
///
/// This maps directly to what the UI needs — no event replay required.
#[derive(Debug, Clone, Serialize)]
pub struct TaskSummary {
    /// Task ID.
    pub task_id: String,
    /// Current status (legacy name for compatibility).
    pub status: String,
    /// Aggregate version.
    pub version: u64,
    /// Task description.
    pub description: Option<String>,
    /// Target project.
    pub project: Option<String>,
    /// Task class (dispatch hint).
    pub task_class: Option<String>,
    /// Priority.
    pub priority: Option<String>,
    /// Current session ID.
    pub current_session_id: Option<String>,
    /// Current harness.
    pub current_harness: Option<String>,
    /// Worktree path.
    pub worktree_path: Option<String>,
    /// PR reference.
    pub pr_ref: Option<String>,
    /// PR branch.
    pub pr_branch: Option<String>,
    /// PR title.
    pub pr_title: Option<String>,
    /// Whether PR is a draft.
    pub pr_draft: Option<bool>,
    /// CI status.
    pub ci_status: Option<String>,
    /// Last reviewer.
    pub last_reviewer: Option<String>,
    /// Total continuation count.
    pub continuation_count: u32,
    /// Post-PR continuation count.
    pub post_pr_continuations: u32,
    /// Outcome (for terminal tasks).
    pub outcome: Option<String>,
    /// Failure reason.
    pub failure_reason: Option<String>,
    /// Who requested the task.
    pub requested_by: Option<String>,
    /// Root trace ID.
    pub trace_id: Option<String>,
    /// When the task was requested.
    pub requested_at: Option<String>,
    /// When last updated.
    pub updated_at: String,
}

pub(crate) fn list_tasks(
    conn: &Connection,
    filter: &TaskFilter,
) -> Result<Vec<TaskSummary>, AppendError> {
    let mut sql = String::from(
        "SELECT task_id, status, version, description, project, task_class, priority,
                current_session_id, current_harness, worktree_path,
                pr_ref, pr_branch, pr_title, pr_draft, ci_status, last_reviewer,
                continuation_count, post_pr_continuations, outcome, failure_reason,
                requested_by, trace_id, requested_at, updated_at
         FROM task_state WHERE 1=1",
    );
    let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

    if let Some(status) = &filter.status {
        param_values.push(Box::new(status.clone()));
        sql.push_str(&format!(" AND status = ?{}", param_values.len()));
    }
    if let Some(project) = &filter.project {
        param_values.push(Box::new(project.clone()));
        sql.push_str(&format!(" AND project = ?{}", param_values.len()));
    }
    if filter.active_only {
        sql.push_str(" AND status NOT IN ('merged', 'failed', 'abandoned')");
    }

    sql.push_str(" ORDER BY updated_at DESC");

    if let Some(limit) = filter.limit {
        param_values.push(Box::new(limit));
        sql.push_str(&format!(" LIMIT ?{}", param_values.len()));
    }

    let param_refs: Vec<&dyn rusqlite::types::ToSql> = param_values.iter().map(std::convert::AsRef::as_ref).collect();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(param_refs.as_slice(), row_to_summary)?;

    let mut results = Vec::new();
    for row in rows {
        results.push(row?);
    }
    Ok(results)
}

pub(crate) fn get_task(
    conn: &Connection,
    task_id: &str,
) -> Result<Option<TaskSummary>, AppendError> {
    let result = conn
        .query_row(
            "SELECT task_id, status, version, description, project, task_class, priority,
                    current_session_id, current_harness, worktree_path,
                    pr_ref, pr_branch, pr_title, pr_draft, ci_status, last_reviewer,
                    continuation_count, post_pr_continuations, outcome, failure_reason,
                    requested_by, trace_id, requested_at, updated_at
             FROM task_state WHERE task_id = ?1",
            params![task_id],
            row_to_summary,
        )
        .optional()?;
    Ok(result)
}

fn row_to_summary(row: &rusqlite::Row<'_>) -> rusqlite::Result<TaskSummary> {
    Ok(TaskSummary {
        task_id: row.get(0)?,
        status: row.get(1)?,
        version: row.get(2)?,
        description: row.get(3)?,
        project: row.get(4)?,
        task_class: row.get(5)?,
        priority: row.get(6)?,
        current_session_id: row.get(7)?,
        current_harness: row.get(8)?,
        worktree_path: row.get(9)?,
        pr_ref: row.get(10)?,
        pr_branch: row.get(11)?,
        pr_title: row.get(12)?,
        pr_draft: row.get(13)?,
        ci_status: row.get(14)?,
        last_reviewer: row.get(15)?,
        continuation_count: row.get::<_, u32>(16)?,
        post_pr_continuations: row.get::<_, u32>(17)?,
        outcome: row.get(18)?,
        failure_reason: row.get(19)?,
        requested_by: row.get(20)?,
        trace_id: row.get(21)?,
        requested_at: row.get(22)?,
        updated_at: row.get(23)?,
    })
}

#[cfg(test)]
mod tests {
    use jam_task_core::{Priority, TaskEvent};

    use super::*;
    use crate::store::TaskStore;

    fn ts(s: &str) -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::parse_from_rfc3339(s)
            .unwrap()
            .with_timezone(&chrono::Utc)
    }

    fn seed_tasks(store: &mut TaskStore) {
        for i in 1..=5 {
            let project = if i <= 3 { "blueberry" } else { "jamboree" };
            store
                .append(
                    &format!("task-{i}"),
                    &TaskEvent::Requested {
                        description: format!("Task {i}"),
                        project: project.into(),
                        task_class: "light-edit".into(),
                        priority: Priority::Normal,
                        requested_by: "human:caleb".into(),
                        at: ts("2026-05-24T10:00:00Z"),
                    },
                    &format!("trace-{i}"),
                    0,
                    None,
                )
                .unwrap();
        }

        // Make task-1 active
        store
            .append(
                "task-1",
                &TaskEvent::PickerAssigned {
                    session_id: "session-1".into(),
                    harness: "codex-cli".into(),
                    worktree_path: "/home/picker/workers/task-1".into(),
                    picker_trace_id: "01TRACE001".into(),
                    at: ts("2026-05-24T10:01:00Z"),
                },
                "trace-10",
                1,
                None,
            )
            .unwrap();

        // Make task-2 merged
        store
            .append(
                "task-2",
                &TaskEvent::PickerAssigned {
                    session_id: "session-2".into(),
                    harness: "codex-cli".into(),
                    worktree_path: "/home/picker/workers/task-2".into(),
                    picker_trace_id: "01TRACE002".into(),
                    at: ts("2026-05-24T10:01:00Z"),
                },
                "trace-11",
                1,
                None,
            )
            .unwrap();
        store
            .append(
                "task-2",
                &TaskEvent::PickerSucceeded {
                    session_id: "session-2".into(),
                    duration_ms: 60_000,
                    at: ts("2026-05-24T10:05:00Z"),
                },
                "trace-12",
                2,
                None,
            )
            .unwrap();
        store
            .append(
                "task-2",
                &TaskEvent::PrOpened {
                    pr_ref: "cleak/blueberry#42".into(),
                    branch: "task/task-2".into(),
                    title: "feat: task 2".into(),
                    draft: false,
                    at: ts("2026-05-24T10:06:00Z"),
                },
                "trace-13",
                3,
                None,
            )
            .unwrap();
        store
            .append(
                "task-2",
                &TaskEvent::PrMerged {
                    pr_ref: "cleak/blueberry#42".into(),
                    merged_sha: "abc".into(),
                    merged_by: "caleb".into(),
                    at: ts("2026-05-24T10:10:00Z"),
                },
                "trace-14",
                4,
                None,
            )
            .unwrap();
    }

    #[test]
    fn list_all() {
        let mut store = TaskStore::in_memory().unwrap();
        seed_tasks(&mut store);

        let all = store.list(&TaskFilter::default()).unwrap();
        assert_eq!(all.len(), 5);
    }

    #[test]
    fn filter_by_status() {
        let mut store = TaskStore::in_memory().unwrap();
        seed_tasks(&mut store);

        let merged = store
            .list(&TaskFilter {
                status: Some("merged".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].task_id, "task-2");
    }

    #[test]
    fn filter_by_project() {
        let mut store = TaskStore::in_memory().unwrap();
        seed_tasks(&mut store);

        let jamboree = store
            .list(&TaskFilter {
                project: Some("jamboree".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(jamboree.len(), 2);
    }

    #[test]
    fn filter_active_only() {
        let mut store = TaskStore::in_memory().unwrap();
        seed_tasks(&mut store);

        let active = store
            .list(&TaskFilter {
                active_only: true,
                ..Default::default()
            })
            .unwrap();
        // task-2 is merged (terminal), so 4 active
        assert_eq!(active.len(), 4);
    }

    #[test]
    fn limit_results() {
        let mut store = TaskStore::in_memory().unwrap();
        seed_tasks(&mut store);

        let limited = store
            .list(&TaskFilter {
                limit: Some(2),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(limited.len(), 2);
    }
}
