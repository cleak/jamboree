use crate::TaskStatus;

/// Errors from applying events to the task aggregate.
#[derive(Debug, Clone, thiserror::Error)]
pub enum ApplyError {
    /// Attempted to create a task that already exists.
    #[error("task {task_id} already initialized (version {version})")]
    AlreadyInitialized {
        /// The task ID.
        task_id: String,
        /// Current version.
        version: u64,
    },

    /// First event must be Requested.
    #[error("first event must be Requested, got {event_type}")]
    NotInitialized {
        /// The event type that was applied.
        event_type: &'static str,
    },
}

/// Describes what happened when an event was applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyOutcome {
    /// Status transitioned to a new state.
    Transitioned {
        /// Previous status.
        from: TaskStatus,
        /// New status.
        to: TaskStatus,
    },
    /// Event was recorded (metadata updated) but status did not change.
    /// This happens when a late event arrives for a terminal task, or when
    /// the event doesn't trigger a status change.
    Recorded {
        /// Current status (unchanged).
        status: TaskStatus,
        /// Why the status didn't change.
        reason: &'static str,
    },
    /// The aggregate was created (first event applied).
    Created,
}
