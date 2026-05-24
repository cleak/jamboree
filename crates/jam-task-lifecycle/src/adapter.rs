//! Translate NATS journal events into domain events for the task aggregate.
//!
//! The journal uses a flat event model (events.toml); the domain model uses
//! typed `TaskEvent` variants. This adapter handles the mapping, including
//! deciding the continuation phase (pre-PR vs post-PR) from aggregate state.

use chrono::{DateTime, Utc};
use jam_task_core::{ContinuationPhase, Priority, TaskEvent};

use crate::JournalEnvelope;

/// Translate a NATS journal event into a domain event.
///
/// Returns `None` for event types that don't map to task lifecycle transitions
/// (e.g. `picker.first-output`, `picker.idle`, `skills.changed`).
///
/// `has_pr` is queried from the current aggregate state to determine the
/// continuation phase. Pass `false` if the aggregate doesn't exist yet.
pub(crate) fn translate(envelope: &JournalEnvelope, has_pr: bool) -> Option<TaskEvent> {
    match envelope.event_type.as_str() {
        "task.requested" => translate_requested(envelope),
        "picker.spawned" => translate_picker_spawned(envelope),
        "picker.exited" => translate_picker_exited(envelope),
        "pr.opened" => translate_pr_opened(envelope),
        "pr.merged" => translate_pr_merged(envelope),
        "pr.review-received" => translate_review_received(envelope),
        "pr.ci.status-changed" => translate_ci_status_changed(envelope),
        "picker.continuation-needed" => translate_continuation_needed(envelope, has_pr),
        "task.failed" => translate_task_failed(envelope),
        "task.abandoned" => translate_task_abandoned(envelope),
        _ => None,
    }
}

/// Build an idempotency key from the event type and trace ID.
///
/// Every NATS event carries a unique trace_id in its headers. Using
/// `{event_type}:{trace_id}` as the idempotency key guarantees that
/// reprocessing the same NATS event (e.g. after a restart) is a no-op.
pub(crate) fn idempotency_key(envelope: &JournalEnvelope) -> String {
    format!("{}:{}", envelope.event_type, envelope.trace_id)
}

fn translate_requested(env: &JournalEnvelope) -> Option<TaskEvent> {
    Some(TaskEvent::Requested {
        description: str_field(&env.payload, "description")?,
        project: str_field(&env.payload, "project").unwrap_or_default(),
        task_class: str_field(&env.payload, "task_class").unwrap_or_default(),
        priority: Priority::from_str_lossy(
            &str_field(&env.payload, "priority").unwrap_or_default(),
        ),
        requested_by: str_field(&env.payload, "requested_by").unwrap_or_default(),
        at: env.timestamp,
    })
}

fn translate_picker_spawned(env: &JournalEnvelope) -> Option<TaskEvent> {
    Some(TaskEvent::PickerAssigned {
        session_id: str_field(&env.payload, "session_id")?,
        harness: str_field(&env.payload, "harness").unwrap_or_default(),
        worktree_path: str_field(&env.payload, "worktree_path").unwrap_or_default(),
        picker_trace_id: str_field(&env.payload, "picker_trace_id").unwrap_or_default(),
        at: dt_field(&env.payload, "spawned_at").unwrap_or(env.timestamp),
    })
}

fn translate_picker_exited(env: &JournalEnvelope) -> Option<TaskEvent> {
    let session_id = str_field(&env.payload, "session_id")?;
    let exit_code = env.payload.get("exit_code")?.as_u64()? as u32;
    let duration_ms = env
        .payload
        .get("duration_ms")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let at = dt_field(&env.payload, "exited_at").unwrap_or(env.timestamp);

    if exit_code == 0 {
        Some(TaskEvent::PickerSucceeded {
            session_id,
            duration_ms,
            at,
        })
    } else {
        Some(TaskEvent::PickerFailed {
            session_id,
            exit_code,
            duration_ms,
            at,
        })
    }
}

fn translate_pr_opened(env: &JournalEnvelope) -> Option<TaskEvent> {
    Some(TaskEvent::PrOpened {
        pr_ref: str_field(&env.payload, "pr_ref")?,
        branch: str_field(&env.payload, "branch").unwrap_or_default(),
        title: str_field(&env.payload, "title").unwrap_or_default(),
        draft: env
            .payload
            .get("draft")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        at: dt_field(&env.payload, "opened_at").unwrap_or(env.timestamp),
    })
}

fn translate_pr_merged(env: &JournalEnvelope) -> Option<TaskEvent> {
    Some(TaskEvent::PrMerged {
        pr_ref: str_field(&env.payload, "pr_ref")?,
        merged_sha: str_field(&env.payload, "merged_sha").unwrap_or_default(),
        merged_by: str_field(&env.payload, "merged_by").unwrap_or_default(),
        at: dt_field(&env.payload, "merged_at").unwrap_or(env.timestamp),
    })
}

fn translate_review_received(env: &JournalEnvelope) -> Option<TaskEvent> {
    Some(TaskEvent::ReviewReceived {
        pr_ref: str_field(&env.payload, "pr_ref")?,
        reviewer: str_field(&env.payload, "reviewer").unwrap_or_default(),
        at: dt_field(&env.payload, "received_at").unwrap_or(env.timestamp),
    })
}

fn translate_ci_status_changed(env: &JournalEnvelope) -> Option<TaskEvent> {
    Some(TaskEvent::CiStatusChanged {
        pr_ref: str_field(&env.payload, "pr_ref")?,
        ci_status: str_field(&env.payload, "ci_status").unwrap_or_default(),
        at: dt_field(&env.payload, "changed_at").unwrap_or(env.timestamp),
    })
}

fn translate_continuation_needed(env: &JournalEnvelope, has_pr: bool) -> Option<TaskEvent> {
    let phase = if has_pr {
        ContinuationPhase::PostPr
    } else {
        ContinuationPhase::PrePr
    };
    Some(TaskEvent::ContinuationRequested {
        session_id: str_field(&env.payload, "session_id").unwrap_or_default(),
        reason: str_field(&env.payload, "reason").unwrap_or_default(),
        detail: str_field(&env.payload, "detail").unwrap_or_default(),
        phase,
        at: dt_field(&env.payload, "requested_at").unwrap_or(env.timestamp),
    })
}

fn translate_task_failed(env: &JournalEnvelope) -> Option<TaskEvent> {
    Some(TaskEvent::Failed {
        reason: str_field(&env.payload, "reason").unwrap_or_default(),
        detail: str_field(&env.payload, "detail").unwrap_or_default(),
        source: str_field(&env.payload, "source_event_type").unwrap_or_default(),
        at: dt_field(&env.payload, "failed_at").unwrap_or(env.timestamp),
    })
}

fn translate_task_abandoned(env: &JournalEnvelope) -> Option<TaskEvent> {
    Some(TaskEvent::Abandoned {
        reason: str_field(&env.payload, "reason").unwrap_or_default(),
        at: dt_field(&env.payload, "abandoned_at").unwrap_or(env.timestamp),
    })
}

fn str_field(payload: &serde_json::Value, field: &str) -> Option<String> {
    payload
        .get(field)?
        .as_str()
        .map(ToOwned::to_owned)
        .or_else(|| payload.get(field).map(ToString::to_string))
}

fn dt_field(payload: &serde_json::Value, field: &str) -> Option<DateTime<Utc>> {
    payload
        .get(field)?
        .as_str()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn envelope(event_type: &str, payload: serde_json::Value) -> JournalEnvelope {
        JournalEnvelope {
            event_type: event_type.into(),
            timestamp: DateTime::parse_from_rfc3339("2026-05-24T10:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
            trace_id: "01TRACE000".into(),
            parent_trace_id: None,
            payload,
        }
    }

    #[test]
    fn translates_task_requested() {
        let env = envelope(
            "task.requested",
            serde_json::json!({
                "task_id": "task-1",
                "description": "Add feature X",
                "project": "blueberry",
                "task_class": "light-edit",
                "priority": "high",
                "requested_by": "human:caleb"
            }),
        );
        let event = translate(&env, false).unwrap();
        assert!(matches!(event, TaskEvent::Requested { .. }));
        assert_eq!(event.event_type(), "requested");
    }

    #[test]
    fn translates_picker_spawned_to_assigned() {
        let env = envelope(
            "picker.spawned",
            serde_json::json!({
                "task_id": "task-1",
                "session_id": "codex-cli:01ABC",
                "harness": "codex-cli",
                "worktree_path": "/home/picker/workers/task-1",
                "spawned_at": "2026-05-24T10:01:00Z",
                "picker_trace_id": "01TRACE001",
                "maestro_trace_id": "01TRACE000"
            }),
        );
        let event = translate(&env, false).unwrap();
        assert!(matches!(event, TaskEvent::PickerAssigned { .. }));
    }

    #[test]
    fn translates_picker_exited_zero_to_succeeded() {
        let env = envelope(
            "picker.exited",
            serde_json::json!({
                "task_id": "task-1",
                "session_id": "codex-cli:01ABC",
                "exit_code": 0,
                "exited_at": "2026-05-24T10:05:00Z",
                "duration_ms": 60000
            }),
        );
        let event = translate(&env, false).unwrap();
        assert!(matches!(event, TaskEvent::PickerSucceeded { .. }));
    }

    #[test]
    fn translates_picker_exited_nonzero_to_failed() {
        let env = envelope(
            "picker.exited",
            serde_json::json!({
                "task_id": "task-1",
                "session_id": "codex-cli:01ABC",
                "exit_code": 1,
                "exited_at": "2026-05-24T10:05:00Z",
                "duration_ms": 60000
            }),
        );
        let event = translate(&env, false).unwrap();
        assert!(matches!(event, TaskEvent::PickerFailed { .. }));
    }

    #[test]
    fn continuation_phase_depends_on_pr_state() {
        let env = envelope(
            "picker.continuation-needed",
            serde_json::json!({
                "task_id": "task-1",
                "session_id": "codex-cli:01ABC",
                "reason": "no-commits",
                "detail": "no commits found",
                "prompt": "fix it",
                "attempt": 1,
                "requested_at": "2026-05-24T10:05:00Z"
            }),
        );

        let pre_pr = translate(&env, false).unwrap();
        assert!(matches!(
            pre_pr,
            TaskEvent::ContinuationRequested {
                phase: ContinuationPhase::PrePr,
                ..
            }
        ));

        let post_pr = translate(&env, true).unwrap();
        assert!(matches!(
            post_pr,
            TaskEvent::ContinuationRequested {
                phase: ContinuationPhase::PostPr,
                ..
            }
        ));
    }

    #[test]
    fn idempotency_key_format() {
        let env = envelope("picker.spawned", serde_json::json!({}));
        assert_eq!(idempotency_key(&env), "picker.spawned:01TRACE000");
    }

    #[test]
    fn unknown_event_returns_none() {
        let env = envelope("picker.idle", serde_json::json!({}));
        assert!(translate(&env, false).is_none());
    }
}
