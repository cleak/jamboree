use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{ApplyError, ApplyOutcome};
use crate::event::{ContinuationPhase, TaskEvent};
use crate::status::{Priority, TaskStatus};
use crate::CONTINUATION_CAP;

/// PR information tracked by the aggregate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrInfo {
    /// GitHub PR reference ("owner/repo#123").
    pub pr_ref: String,
    /// Branch name.
    pub branch: String,
    /// PR title.
    pub title: String,
    /// Whether the PR is a draft.
    pub draft: bool,
    /// Last known CI status.
    pub ci_status: Option<String>,
    /// Last reviewer who left comments.
    pub last_reviewer: Option<String>,
    /// Merge commit SHA (set when merged).
    pub merged_sha: Option<String>,
    /// Who merged (set when merged).
    pub merged_by: Option<String>,
    /// When merged (set when merged).
    pub merged_at: Option<DateTime<Utc>>,
    /// When the PR was opened.
    pub opened_at: DateTime<Utc>,
}

/// The task aggregate — the single source of truth for a task's state.
///
/// State is derived by replaying domain events. The aggregate validates
/// transitions and maintains all the data needed for lifecycle decisions.
///
/// ## Transition table
///
/// ```text
/// Queued      + PickerAssigned          → Active
/// Active      + PickerSucceeded         → PostPicker
/// Active      + PickerFailed            → Active (continuation) | Failed (cap)
/// PostPicker  + PrOpened                → InReview
/// PostPicker  + ContinuationRequested   → Active (via dispatch)
/// InReview    + ReviewReceived          → Active (continuation dispatched)
/// InReview    + CiStatusChanged(fail)   → Active (continuation dispatched)
/// InReview    + PrMerged                → Merged (terminal)
/// Any(non-T)  + Failed                 → Failed (terminal)
/// Any(non-T)  + Abandoned              → Abandoned (terminal)
///
/// Terminal states (Merged, Failed, Abandoned) accept events for metadata
/// recording but never change status.
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskAggregate {
    id: String,
    version: u64,
    initialized: bool,
    status: TaskStatus,

    // Creation metadata
    description: String,
    project: String,
    task_class: String,
    priority: Priority,
    requested_by: String,
    requested_at: DateTime<Utc>,

    // Current session
    current_session_id: Option<String>,
    current_harness: Option<String>,
    worktree_path: Option<String>,

    // PR state
    pr: Option<PrInfo>,

    // Continuation tracking — separate caps for pre-PR and post-PR phases
    pre_pr_continuations: u32,
    post_pr_continuations: u32,

    // Terminal state metadata
    outcome: Option<String>,
    failure_reason: Option<String>,
    failure_detail: Option<String>,
    failure_source: Option<String>,
    abandoned_reason: Option<String>,

    // Trace chain
    trace_id: String,
    picker_trace_id: Option<String>,

    updated_at: DateTime<Utc>,
}

impl TaskAggregate {
    /// Create a new empty aggregate for the given task ID.
    ///
    /// The aggregate is not yet initialized — the first event applied must be
    /// `TaskEvent::Requested`.
    pub fn new(id: impl Into<String>, trace_id: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: id.into(),
            version: 0,
            initialized: false,
            status: TaskStatus::Queued,
            description: String::new(),
            project: String::new(),
            task_class: String::new(),
            priority: Priority::Normal,
            requested_by: String::new(),
            requested_at: now,
            current_session_id: None,
            current_harness: None,
            worktree_path: None,
            pr: None,
            pre_pr_continuations: 0,
            post_pr_continuations: 0,
            outcome: None,
            failure_reason: None,
            failure_detail: None,
            failure_source: None,
            abandoned_reason: None,
            trace_id: trace_id.into(),
            picker_trace_id: None,
            updated_at: now,
        }
    }

    /// Apply a domain event to the aggregate, updating state.
    ///
    /// Returns what happened: a state transition, metadata-only recording, or
    /// creation. Returns an error only for structural violations (e.g. applying
    /// `Requested` to an already-initialized aggregate).
    pub fn apply(&mut self, event: &TaskEvent) -> Result<ApplyOutcome, ApplyError> {
        if !self.initialized {
            return self.apply_first(event);
        }

        if let TaskEvent::Requested { .. } = event {
            return Err(ApplyError::AlreadyInitialized {
                task_id: self.id.clone(),
                version: self.version,
            });
        }

        let old_status = self.status;

        if old_status.is_terminal() {
            self.record_metadata(event);
            self.version += 1;
            self.updated_at = event.timestamp();
            return Ok(ApplyOutcome::Recorded {
                status: old_status,
                reason: "task is terminal",
            });
        }

        let new_status = self.compute_transition(event);
        self.apply_fields(event);

        if let Some(status) = new_status {
            self.status = status;
        }
        self.version += 1;
        self.updated_at = event.timestamp();

        let outcome = match new_status {
            Some(to) if to != old_status => ApplyOutcome::Transitioned {
                from: old_status,
                to,
            },
            _ => ApplyOutcome::Recorded {
                status: self.status,
                reason: "no status change",
            },
        };
        Ok(outcome)
    }

    fn apply_first(&mut self, event: &TaskEvent) -> Result<ApplyOutcome, ApplyError> {
        let TaskEvent::Requested {
            description,
            project,
            task_class,
            priority,
            requested_by,
            at,
        } = event
        else {
            return Err(ApplyError::NotInitialized {
                event_type: event.event_type(),
            });
        };
        self.initialized = true;
        self.status = TaskStatus::Queued;
        self.description = description.clone();
        self.project = project.clone();
        self.task_class = task_class.clone();
        self.priority = *priority;
        self.requested_by = requested_by.clone();
        self.requested_at = *at;
        self.updated_at = *at;
        self.version = 1;
        Ok(ApplyOutcome::Created)
    }

    /// Compute the new status from the event, or None if no transition.
    ///
    /// Each arm guards the source state — only valid origin states produce
    /// transitions. Invalid sources return None (event recorded, no status
    /// change). Terminal states are already filtered before this is called.
    fn compute_transition(&self, event: &TaskEvent) -> Option<TaskStatus> {
        match event {
            TaskEvent::PickerAssigned { .. } => {
                if matches!(self.status, TaskStatus::Queued | TaskStatus::Active) {
                    Some(TaskStatus::Active)
                } else {
                    None
                }
            }

            TaskEvent::PickerSucceeded { .. } => {
                if self.status == TaskStatus::Active {
                    Some(TaskStatus::PostPicker)
                } else {
                    None
                }
            }

            TaskEvent::PickerFailed { .. } | TaskEvent::ContinuationRequested { .. } => None,

            TaskEvent::ContinuationDispatched { .. } => {
                if matches!(
                    self.status,
                    TaskStatus::PostPicker | TaskStatus::InReview | TaskStatus::Active
                ) {
                    Some(TaskStatus::Active)
                } else {
                    None
                }
            }

            TaskEvent::PrOpened { .. } => {
                if matches!(self.status, TaskStatus::PostPicker | TaskStatus::Active) {
                    Some(TaskStatus::InReview)
                } else {
                    None
                }
            }

            TaskEvent::ReviewReceived { .. } | TaskEvent::CiStatusChanged { .. } => None,

            TaskEvent::PrMerged { .. } => {
                if self.status == TaskStatus::InReview {
                    Some(TaskStatus::Merged)
                } else {
                    None
                }
            }

            TaskEvent::Failed { .. } => Some(TaskStatus::Failed),

            TaskEvent::Abandoned { .. } => Some(TaskStatus::Abandoned),

            TaskEvent::Requested { .. } => None,
        }
    }

    /// Apply the event's fields to the aggregate (non-status updates).
    fn apply_fields(&mut self, event: &TaskEvent) {
        match event {
            TaskEvent::PickerAssigned {
                session_id,
                harness,
                worktree_path,
                picker_trace_id,
                ..
            } => {
                self.current_session_id = Some(session_id.clone());
                self.current_harness = Some(harness.clone());
                self.worktree_path = Some(worktree_path.clone());
                self.picker_trace_id = Some(picker_trace_id.clone());
            }

            TaskEvent::PickerSucceeded { session_id, .. }
            | TaskEvent::PickerFailed { session_id, .. } => {
                if self.current_session_id.as_deref() == Some(session_id) {
                    self.current_session_id = None;
                    self.current_harness = None;
                    self.worktree_path = None;
                }
            }

            TaskEvent::ContinuationRequested { phase, .. } => match phase {
                ContinuationPhase::PrePr => self.pre_pr_continuations += 1,
                ContinuationPhase::PostPr => self.post_pr_continuations += 1,
            },

            TaskEvent::ContinuationDispatched { new_session_id, .. } => {
                self.current_session_id = Some(new_session_id.clone());
            }

            TaskEvent::PrOpened {
                pr_ref,
                branch,
                title,
                draft,
                at,
                ..
            } => {
                // Reset post-PR continuation count on new PR
                self.post_pr_continuations = 0;
                self.pr = Some(PrInfo {
                    pr_ref: pr_ref.clone(),
                    branch: branch.clone(),
                    title: title.clone(),
                    draft: *draft,
                    ci_status: None,
                    last_reviewer: None,
                    merged_sha: None,
                    merged_by: None,
                    merged_at: None,
                    opened_at: *at,
                });
            }

            TaskEvent::ReviewReceived {
                reviewer, pr_ref, ..
            } => {
                if let Some(pr) = &mut self.pr {
                    pr.last_reviewer = Some(reviewer.clone());
                    pr.pr_ref = pr_ref.clone();
                }
            }

            TaskEvent::CiStatusChanged {
                ci_status, pr_ref, ..
            } => {
                if let Some(pr) = &mut self.pr {
                    pr.ci_status = Some(ci_status.clone());
                    pr.pr_ref = pr_ref.clone();
                }
            }

            TaskEvent::PrMerged {
                pr_ref,
                merged_sha,
                merged_by,
                at,
                ..
            } => {
                self.outcome = Some("merged".to_owned());
                if let Some(pr) = &mut self.pr {
                    pr.pr_ref = pr_ref.clone();
                    pr.merged_sha = Some(merged_sha.clone());
                    pr.merged_by = Some(merged_by.clone());
                    pr.merged_at = Some(*at);
                } else {
                    self.pr = Some(PrInfo {
                        pr_ref: pr_ref.clone(),
                        branch: String::new(),
                        title: String::new(),
                        draft: false,
                        ci_status: None,
                        last_reviewer: None,
                        merged_sha: Some(merged_sha.clone()),
                        merged_by: Some(merged_by.clone()),
                        merged_at: Some(*at),
                        opened_at: *at,
                    });
                }
            }

            TaskEvent::Failed {
                reason,
                detail,
                source,
                ..
            } => {
                self.outcome = Some(reason.clone());
                self.failure_reason = Some(reason.clone());
                self.failure_detail = Some(detail.clone());
                self.failure_source = Some(source.clone());
            }

            TaskEvent::Abandoned { reason, .. } => {
                self.outcome = Some(reason.clone());
                self.abandoned_reason = Some(reason.clone());
            }

            TaskEvent::Requested { .. } => {}
        }
    }

    /// Record metadata from a late event on a terminal task.
    fn record_metadata(&mut self, event: &TaskEvent) {
        match event {
            TaskEvent::PickerAssigned {
                session_id,
                harness,
                worktree_path,
                picker_trace_id,
                ..
            } => {
                self.current_session_id = Some(session_id.clone());
                self.current_harness = Some(harness.clone());
                self.worktree_path = Some(worktree_path.clone());
                self.picker_trace_id = Some(picker_trace_id.clone());
            }
            TaskEvent::PrOpened {
                pr_ref,
                branch,
                title,
                draft,
                at,
                ..
            } => {
                self.pr = Some(PrInfo {
                    pr_ref: pr_ref.clone(),
                    branch: branch.clone(),
                    title: title.clone(),
                    draft: *draft,
                    ci_status: self.pr.as_ref().and_then(|p| p.ci_status.clone()),
                    last_reviewer: self.pr.as_ref().and_then(|p| p.last_reviewer.clone()),
                    merged_sha: self.pr.as_ref().and_then(|p| p.merged_sha.clone()),
                    merged_by: self.pr.as_ref().and_then(|p| p.merged_by.clone()),
                    merged_at: self.pr.as_ref().and_then(|p| p.merged_at),
                    opened_at: *at,
                });
            }
            _ => {}
        }
    }

    // --- Accessors ---

    /// Task ID.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Current version (number of events applied).
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Whether the aggregate has been initialized with a Requested event.
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Current status.
    pub fn status(&self) -> TaskStatus {
        self.status
    }

    /// Whether the task is in a terminal state.
    pub fn is_terminal(&self) -> bool {
        self.status.is_terminal()
    }

    /// Task description.
    pub fn description(&self) -> &str {
        &self.description
    }

    /// Target project.
    pub fn project(&self) -> &str {
        &self.project
    }

    /// Task class (dispatch hint).
    pub fn task_class(&self) -> &str {
        &self.task_class
    }

    /// Priority.
    pub fn priority(&self) -> Priority {
        self.priority
    }

    /// Who requested the task.
    pub fn requested_by(&self) -> &str {
        &self.requested_by
    }

    /// When the task was requested.
    pub fn requested_at(&self) -> DateTime<Utc> {
        self.requested_at
    }

    /// Current active session ID, if any.
    pub fn current_session_id(&self) -> Option<&str> {
        self.current_session_id.as_deref()
    }

    /// Current harness.
    pub fn current_harness(&self) -> Option<&str> {
        self.current_harness.as_deref()
    }

    /// Worktree path.
    pub fn worktree_path(&self) -> Option<&str> {
        self.worktree_path.as_deref()
    }

    /// PR information.
    pub fn pr(&self) -> Option<&PrInfo> {
        self.pr.as_ref()
    }

    /// Whether a PR has been opened for this task.
    pub fn has_pr(&self) -> bool {
        self.pr.is_some()
    }

    /// Number of pre-PR continuations used.
    pub fn pre_pr_continuations(&self) -> u32 {
        self.pre_pr_continuations
    }

    /// Number of post-PR continuations used.
    pub fn post_pr_continuations(&self) -> u32 {
        self.post_pr_continuations
    }

    /// Whether the task can still accept continuations in the current phase.
    pub fn can_continue(&self) -> bool {
        if self.has_pr() {
            self.post_pr_continuations < CONTINUATION_CAP
        } else {
            self.pre_pr_continuations < CONTINUATION_CAP
        }
    }

    /// The current continuation phase based on PR state.
    pub fn continuation_phase(&self) -> ContinuationPhase {
        if self.has_pr() {
            ContinuationPhase::PostPr
        } else {
            ContinuationPhase::PrePr
        }
    }

    /// Total continuations across both phases.
    pub fn total_continuations(&self) -> u32 {
        self.pre_pr_continuations + self.post_pr_continuations
    }

    /// Outcome string (set for terminal states).
    pub fn outcome(&self) -> Option<&str> {
        self.outcome.as_deref()
    }

    /// Failure reason (set when status = Failed).
    pub fn failure_reason(&self) -> Option<&str> {
        self.failure_reason.as_deref()
    }

    /// Root trace ID.
    pub fn trace_id(&self) -> &str {
        &self.trace_id
    }

    /// Picker's trace ID from the most recent spawn.
    pub fn picker_trace_id(&self) -> Option<&str> {
        self.picker_trace_id.as_deref()
    }

    /// When the aggregate was last updated.
    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ts(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    fn requested_event() -> TaskEvent {
        TaskEvent::Requested {
            description: "Add feature X".into(),
            project: "blueberry".into(),
            task_class: "light-edit".into(),
            priority: Priority::Normal,
            requested_by: "human:caleb".into(),
            at: ts("2026-05-24T10:00:00Z"),
        }
    }

    fn picker_assigned_event(session_id: &str) -> TaskEvent {
        TaskEvent::PickerAssigned {
            session_id: session_id.into(),
            harness: "codex-cli".into(),
            worktree_path: "/home/picker/workers/task-1".into(),
            picker_trace_id: "01TRACE001".into(),
            at: ts("2026-05-24T10:01:00Z"),
        }
    }

    fn picker_succeeded_event(session_id: &str) -> TaskEvent {
        TaskEvent::PickerSucceeded {
            session_id: session_id.into(),
            duration_ms: 60_000,
            at: ts("2026-05-24T10:05:00Z"),
        }
    }

    fn pr_opened_event() -> TaskEvent {
        TaskEvent::PrOpened {
            pr_ref: "cleak/blueberry#42".into(),
            branch: "task/task-1".into(),
            title: "feat: add feature X".into(),
            draft: false,
            at: ts("2026-05-24T10:06:00Z"),
        }
    }

    fn pr_merged_event() -> TaskEvent {
        TaskEvent::PrMerged {
            pr_ref: "cleak/blueberry#42".into(),
            merged_sha: "abc123".into(),
            merged_by: "caleb".into(),
            at: ts("2026-05-24T10:10:00Z"),
        }
    }

    fn make_aggregate() -> TaskAggregate {
        let mut agg = TaskAggregate::new("task-1", "01TRACE000");
        agg.apply(&requested_event()).unwrap();
        agg
    }

    #[test]
    fn creation_sets_queued() {
        let agg = make_aggregate();
        assert_eq!(agg.status(), TaskStatus::Queued);
        assert_eq!(agg.version(), 1);
        assert!(agg.is_initialized());
        assert_eq!(agg.description(), "Add feature X");
        assert_eq!(agg.project(), "blueberry");
    }

    #[test]
    fn double_requested_is_error() {
        let mut agg = make_aggregate();
        let result = agg.apply(&requested_event());
        assert!(result.is_err());
    }

    #[test]
    fn picker_assigned_transitions_to_active() {
        let mut agg = make_aggregate();
        let outcome = agg.apply(&picker_assigned_event("session-1")).unwrap();
        assert_eq!(
            outcome,
            ApplyOutcome::Transitioned {
                from: TaskStatus::Queued,
                to: TaskStatus::Active,
            }
        );
        assert_eq!(agg.status(), TaskStatus::Active);
        assert_eq!(agg.current_session_id(), Some("session-1"));
        assert_eq!(agg.current_harness(), Some("codex-cli"));
    }

    #[test]
    fn picker_succeeded_transitions_to_post_picker() {
        let mut agg = make_aggregate();
        agg.apply(&picker_assigned_event("session-1")).unwrap();
        let outcome = agg.apply(&picker_succeeded_event("session-1")).unwrap();
        assert_eq!(
            outcome,
            ApplyOutcome::Transitioned {
                from: TaskStatus::Active,
                to: TaskStatus::PostPicker,
            }
        );
        assert_eq!(agg.status(), TaskStatus::PostPicker);
    }

    #[test]
    fn pr_opened_transitions_to_in_review() {
        let mut agg = make_aggregate();
        agg.apply(&picker_assigned_event("session-1")).unwrap();
        agg.apply(&picker_succeeded_event("session-1")).unwrap();
        let outcome = agg.apply(&pr_opened_event()).unwrap();
        assert_eq!(
            outcome,
            ApplyOutcome::Transitioned {
                from: TaskStatus::PostPicker,
                to: TaskStatus::InReview,
            }
        );
        assert!(agg.has_pr());
        assert_eq!(agg.pr().unwrap().pr_ref, "cleak/blueberry#42");
    }

    #[test]
    fn pr_merged_transitions_to_merged() {
        let mut agg = make_aggregate();
        agg.apply(&picker_assigned_event("session-1")).unwrap();
        agg.apply(&picker_succeeded_event("session-1")).unwrap();
        agg.apply(&pr_opened_event()).unwrap();
        let outcome = agg.apply(&pr_merged_event()).unwrap();
        assert_eq!(
            outcome,
            ApplyOutcome::Transitioned {
                from: TaskStatus::InReview,
                to: TaskStatus::Merged,
            }
        );
        assert!(agg.is_terminal());
        assert_eq!(agg.outcome(), Some("merged"));
    }

    #[test]
    fn terminal_task_rejects_status_change_but_records_metadata() {
        let mut agg = make_aggregate();
        agg.apply(&picker_assigned_event("session-1")).unwrap();
        agg.apply(&picker_succeeded_event("session-1")).unwrap();
        agg.apply(&pr_opened_event()).unwrap();
        agg.apply(&pr_merged_event()).unwrap();

        // Late picker.spawned on merged task
        let outcome = agg.apply(&picker_assigned_event("session-late")).unwrap();
        assert_eq!(
            outcome,
            ApplyOutcome::Recorded {
                status: TaskStatus::Merged,
                reason: "task is terminal",
            }
        );
        assert_eq!(agg.status(), TaskStatus::Merged);
        // Metadata still recorded
        assert_eq!(agg.current_session_id(), Some("session-late"));
    }

    #[test]
    fn terminal_task_rejects_late_task_failed() {
        let mut agg = make_aggregate();
        agg.apply(&picker_assigned_event("session-1")).unwrap();
        agg.apply(&picker_succeeded_event("session-1")).unwrap();
        agg.apply(&pr_opened_event()).unwrap();
        agg.apply(&pr_merged_event()).unwrap();

        let outcome = agg
            .apply(&TaskEvent::Failed {
                reason: "continuation-cap".into(),
                detail: "cap reached".into(),
                source: "picker.continuation-needed".into(),
                at: ts("2026-05-24T12:00:00Z"),
            })
            .unwrap();
        assert_eq!(
            outcome,
            ApplyOutcome::Recorded {
                status: TaskStatus::Merged,
                reason: "task is terminal",
            }
        );
        assert_eq!(agg.status(), TaskStatus::Merged);
    }

    #[test]
    fn terminal_task_rejects_late_pr_opened() {
        let mut agg = make_aggregate();
        agg.apply(&picker_assigned_event("session-1")).unwrap();
        agg.apply(&picker_succeeded_event("session-1")).unwrap();
        agg.apply(&pr_opened_event()).unwrap();
        agg.apply(&pr_merged_event()).unwrap();

        let outcome = agg
            .apply(&TaskEvent::PrOpened {
                pr_ref: "cleak/jamboree#25".into(),
                branch: "task/task-1".into(),
                title: "phantom PR".into(),
                draft: false,
                at: ts("2026-05-24T12:00:00Z"),
            })
            .unwrap();
        assert_eq!(agg.status(), TaskStatus::Merged);
        // PR metadata updated for visibility even on terminal task
        assert_eq!(agg.pr().unwrap().pr_ref, "cleak/jamboree#25");
        assert!(matches!(outcome, ApplyOutcome::Recorded { .. }));
    }

    #[test]
    fn continuation_tracking_pre_pr() {
        let mut agg = make_aggregate();
        agg.apply(&picker_assigned_event("session-1")).unwrap();
        agg.apply(&picker_succeeded_event("session-1")).unwrap();

        assert!(!agg.has_pr());
        assert_eq!(agg.continuation_phase(), ContinuationPhase::PrePr);

        for i in 0..CONTINUATION_CAP {
            assert!(agg.can_continue(), "should allow continuation {i}");
            agg.apply(&TaskEvent::ContinuationRequested {
                session_id: "session-1".into(),
                reason: "no-commits".into(),
                detail: "no commits found".into(),
                phase: ContinuationPhase::PrePr,
                at: ts("2026-05-24T10:10:00Z"),
            })
            .unwrap();
        }
        assert!(!agg.can_continue());
        assert_eq!(agg.pre_pr_continuations(), CONTINUATION_CAP);
    }

    #[test]
    fn continuation_tracking_post_pr_resets_on_pr_open() {
        let mut agg = make_aggregate();
        agg.apply(&picker_assigned_event("session-1")).unwrap();
        agg.apply(&picker_succeeded_event("session-1")).unwrap();

        // Burn some pre-PR continuations
        for _ in 0..3 {
            agg.apply(&TaskEvent::ContinuationRequested {
                session_id: "session-1".into(),
                reason: "no-commits".into(),
                detail: "no commits found".into(),
                phase: ContinuationPhase::PrePr,
                at: ts("2026-05-24T10:10:00Z"),
            })
            .unwrap();
        }
        assert_eq!(agg.pre_pr_continuations(), 3);

        // Open PR — post-PR counter starts at 0
        agg.apply(&pr_opened_event()).unwrap();
        assert_eq!(agg.post_pr_continuations(), 0);
        assert!(agg.can_continue());
        assert_eq!(agg.continuation_phase(), ContinuationPhase::PostPr);

        // Post-PR continuations have their own cap
        for i in 0..CONTINUATION_CAP {
            assert!(agg.can_continue(), "should allow post-PR continuation {i}");
            agg.apply(&TaskEvent::ContinuationRequested {
                session_id: "session-1".into(),
                reason: "ci-failed".into(),
                detail: "CI failure".into(),
                phase: ContinuationPhase::PostPr,
                at: ts("2026-05-24T11:00:00Z"),
            })
            .unwrap();
        }
        assert!(!agg.can_continue());
    }

    #[test]
    fn full_happy_path() {
        let mut agg = make_aggregate();
        assert_eq!(agg.status(), TaskStatus::Queued);

        agg.apply(&picker_assigned_event("session-1")).unwrap();
        assert_eq!(agg.status(), TaskStatus::Active);

        agg.apply(&picker_succeeded_event("session-1")).unwrap();
        assert_eq!(agg.status(), TaskStatus::PostPicker);

        agg.apply(&pr_opened_event()).unwrap();
        assert_eq!(agg.status(), TaskStatus::InReview);

        agg.apply(&TaskEvent::CiStatusChanged {
            pr_ref: "cleak/blueberry#42".into(),
            ci_status: "success".into(),
            at: ts("2026-05-24T10:08:00Z"),
        })
        .unwrap();
        assert_eq!(agg.status(), TaskStatus::InReview);

        agg.apply(&pr_merged_event()).unwrap();
        assert_eq!(agg.status(), TaskStatus::Merged);
        assert!(agg.is_terminal());
        assert_eq!(agg.pr().unwrap().merged_sha.as_deref(), Some("abc123"));
    }

    #[test]
    fn task_failed_marks_terminal() {
        let mut agg = make_aggregate();
        agg.apply(&TaskEvent::Failed {
            reason: "harness-version-drift".into(),
            detail: "codex-cli version drifted".into(),
            source: "maestro.spawn-picker-error".into(),
            at: ts("2026-05-24T10:00:00Z"),
        })
        .unwrap();
        assert_eq!(agg.status(), TaskStatus::Failed);
        assert!(agg.is_terminal());
        assert_eq!(agg.failure_reason(), Some("harness-version-drift"));
    }

    #[test]
    fn task_abandoned_marks_terminal() {
        let mut agg = make_aggregate();
        agg.apply(&picker_assigned_event("session-1")).unwrap();
        agg.apply(&TaskEvent::Abandoned {
            reason: "human decision".into(),
            at: ts("2026-05-24T10:05:00Z"),
        })
        .unwrap();
        assert_eq!(agg.status(), TaskStatus::Abandoned);
        assert!(agg.is_terminal());
    }

    #[test]
    fn review_received_updates_pr_metadata() {
        let mut agg = make_aggregate();
        agg.apply(&picker_assigned_event("session-1")).unwrap();
        agg.apply(&picker_succeeded_event("session-1")).unwrap();
        agg.apply(&pr_opened_event()).unwrap();

        agg.apply(&TaskEvent::ReviewReceived {
            pr_ref: "cleak/blueberry#42".into(),
            reviewer: "coderabbitai[bot]".into(),
            at: ts("2026-05-24T10:08:00Z"),
        })
        .unwrap();

        assert_eq!(
            agg.pr().unwrap().last_reviewer.as_deref(),
            Some("coderabbitai[bot]")
        );
    }

    #[test]
    fn ci_status_updates_pr_metadata() {
        let mut agg = make_aggregate();
        agg.apply(&picker_assigned_event("session-1")).unwrap();
        agg.apply(&picker_succeeded_event("session-1")).unwrap();
        agg.apply(&pr_opened_event()).unwrap();

        agg.apply(&TaskEvent::CiStatusChanged {
            pr_ref: "cleak/blueberry#42".into(),
            ci_status: "failure".into(),
            at: ts("2026-05-24T10:08:00Z"),
        })
        .unwrap();

        assert_eq!(agg.pr().unwrap().ci_status.as_deref(), Some("failure"));
    }

    #[test]
    fn version_increments_with_each_event() {
        let mut agg = make_aggregate();
        assert_eq!(agg.version(), 1);

        agg.apply(&picker_assigned_event("session-1")).unwrap();
        assert_eq!(agg.version(), 2);

        agg.apply(&picker_succeeded_event("session-1")).unwrap();
        assert_eq!(agg.version(), 3);
    }

    #[test]
    fn uninitialized_aggregate_rejects_non_requested() {
        let mut agg = TaskAggregate::new("task-1", "01TRACE000");
        let result = agg.apply(&picker_assigned_event("session-1"));
        assert!(matches!(result, Err(ApplyError::NotInitialized { .. })));
    }

    #[test]
    fn legacy_status_names() {
        assert_eq!(TaskStatus::Queued.legacy_name(), "backlog");
        assert_eq!(TaskStatus::Active.legacy_name(), "in-progress");
        assert_eq!(TaskStatus::PostPicker.legacy_name(), "picker-completed");
        assert_eq!(TaskStatus::InReview.legacy_name(), "in-review");
        assert_eq!(TaskStatus::Merged.legacy_name(), "merged");
        assert_eq!(TaskStatus::Failed.legacy_name(), "failed");
        assert_eq!(TaskStatus::Abandoned.legacy_name(), "abandoned");
    }

    #[test]
    fn from_legacy_round_trips() {
        for status in [
            TaskStatus::Queued,
            TaskStatus::Active,
            TaskStatus::PostPicker,
            TaskStatus::InReview,
            TaskStatus::Merged,
            TaskStatus::Failed,
            TaskStatus::Abandoned,
        ] {
            assert_eq!(TaskStatus::from_legacy(status.legacy_name()), Some(status));
        }
    }
}
