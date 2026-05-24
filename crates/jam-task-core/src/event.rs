use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::Priority;

/// Domain events for the task lifecycle.
///
/// Each variant is a fact about something that happened. Events are the source
/// of truth — task state is derived by replaying events through the aggregate.
///
/// These are internal domain events, not NATS journal events. The lifecycle
/// service translates NATS events into these before feeding the aggregate.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TaskEvent {
    /// Task was created via `jam task spawn`.
    Requested {
        /// Human-readable description/prompt.
        description: String,
        /// Target repository ("blueberry" | "jamboree").
        project: String,
        /// Dispatch hint for quota routing.
        task_class: String,
        /// Priority level.
        priority: Priority,
        /// Who requested ("human:caleb").
        requested_by: String,
        /// When the request was made.
        at: DateTime<Utc>,
    },

    /// A Picker was spawned to work on this task.
    PickerAssigned {
        /// Harness-qualified session ID (e.g. "codex-cli:01K...").
        session_id: String,
        /// Which harness ("codex-cli" | "claude-code" | "opencode-deepseek").
        harness: String,
        /// Filesystem path to the worktree.
        worktree_path: String,
        /// Picker's own trace ID.
        picker_trace_id: String,
        /// When the Picker was spawned.
        at: DateTime<Utc>,
    },

    /// Picker exited with code 0 (success).
    PickerSucceeded {
        /// Which session exited.
        session_id: String,
        /// Wall-clock duration of the Picker run.
        duration_ms: u64,
        /// When the Picker exited.
        at: DateTime<Utc>,
    },

    /// Picker exited with non-zero code.
    PickerFailed {
        /// Which session exited.
        session_id: String,
        /// Process exit code.
        exit_code: u32,
        /// Wall-clock duration.
        duration_ms: u64,
        /// When the Picker exited.
        at: DateTime<Utc>,
    },

    /// Post-picker coordination determined a continuation is needed.
    ContinuationRequested {
        /// Which session triggered the continuation.
        session_id: String,
        /// Why: "no-commits", "dirty-tree", "missing-pr-metadata", etc.
        reason: String,
        /// Human-readable detail.
        detail: String,
        /// Whether this is pre-PR or post-PR.
        phase: ContinuationPhase,
        /// When requested.
        at: DateTime<Utc>,
    },

    /// A continuation Picker was dispatched (new session spawned from parent).
    ContinuationDispatched {
        /// The parent session that triggered the continuation.
        parent_session_id: String,
        /// The new session that was spawned.
        new_session_id: String,
        /// When dispatched.
        at: DateTime<Utc>,
    },

    /// PR opened for this task's work.
    PrOpened {
        /// GitHub PR reference ("owner/repo#123").
        pr_ref: String,
        /// Branch name.
        branch: String,
        /// PR title.
        title: String,
        /// Whether the PR was opened as a draft.
        draft: bool,
        /// When opened.
        at: DateTime<Utc>,
    },

    /// Review activity received on the task's PR.
    ReviewReceived {
        /// Which PR.
        pr_ref: String,
        /// Who reviewed.
        reviewer: String,
        /// When received.
        at: DateTime<Utc>,
    },

    /// CI status changed on the task's PR.
    CiStatusChanged {
        /// Which PR.
        pr_ref: String,
        /// New CI status.
        ci_status: String,
        /// When changed.
        at: DateTime<Utc>,
    },

    /// PR merged to trunk.
    PrMerged {
        /// Which PR.
        pr_ref: String,
        /// Merge commit SHA.
        merged_sha: String,
        /// Who merged.
        merged_by: String,
        /// When merged.
        at: DateTime<Utc>,
    },

    /// Task explicitly failed (system or human decision).
    Failed {
        /// Why it failed.
        reason: String,
        /// Detailed explanation.
        detail: String,
        /// What triggered the failure (event type or component).
        source: String,
        /// When it failed.
        at: DateTime<Utc>,
    },

    /// Task explicitly abandoned.
    Abandoned {
        /// Why it was abandoned.
        reason: String,
        /// When abandoned.
        at: DateTime<Utc>,
    },
}

/// Whether a continuation is pre-PR (picker quality issues) or post-PR
/// (CI/review feedback). Tracked independently for cap purposes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ContinuationPhase {
    /// Before any PR has been opened for this task.
    PrePr,
    /// After a PR was opened (CI failure, review feedback).
    PostPr,
}

impl TaskEvent {
    /// The event type name, used for logging and store records.
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::Requested { .. } => "requested",
            Self::PickerAssigned { .. } => "picker_assigned",
            Self::PickerSucceeded { .. } => "picker_succeeded",
            Self::PickerFailed { .. } => "picker_failed",
            Self::ContinuationRequested { .. } => "continuation_requested",
            Self::ContinuationDispatched { .. } => "continuation_dispatched",
            Self::PrOpened { .. } => "pr_opened",
            Self::ReviewReceived { .. } => "review_received",
            Self::CiStatusChanged { .. } => "ci_status_changed",
            Self::PrMerged { .. } => "pr_merged",
            Self::Failed { .. } => "failed",
            Self::Abandoned { .. } => "abandoned",
        }
    }

    /// The timestamp of the event.
    pub fn timestamp(&self) -> DateTime<Utc> {
        match self {
            Self::Requested { at, .. }
            | Self::PickerAssigned { at, .. }
            | Self::PickerSucceeded { at, .. }
            | Self::PickerFailed { at, .. }
            | Self::ContinuationRequested { at, .. }
            | Self::ContinuationDispatched { at, .. }
            | Self::PrOpened { at, .. }
            | Self::ReviewReceived { at, .. }
            | Self::CiStatusChanged { at, .. }
            | Self::PrMerged { at, .. }
            | Self::Failed { at, .. }
            | Self::Abandoned { at, .. } => *at,
        }
    }
}
