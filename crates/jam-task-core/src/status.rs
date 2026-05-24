use std::fmt;

use serde::{Deserialize, Serialize};

/// Task lifecycle status — the nodes in the state machine.
///
/// Transitions are enforced by [`super::TaskAggregate::apply`]; see the
/// transition table in `aggregate.rs` for the valid edges.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TaskStatus {
    /// Task created/requested but not yet assigned to a Picker.
    Queued,
    /// A Picker has been spawned and is actively working.
    Active,
    /// Picker exited successfully; post-picker coordination is deciding
    /// whether to open a PR or request a continuation.
    PostPicker,
    /// PR has been opened and is awaiting review/CI/merge.
    InReview,
    /// Terminal: PR merged to trunk.
    Merged,
    /// Terminal: task failed after exhausting retries or unrecoverable error.
    Failed,
    /// Terminal: task explicitly abandoned by human or system.
    Abandoned,
}

impl TaskStatus {
    /// Whether this status is terminal — no further state transitions allowed.
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Merged | Self::Failed | Self::Abandoned)
    }

    /// The legacy status string used in Tempyr task node frontmatter.
    /// Preserves backward compatibility with existing dashboards and queries.
    pub fn legacy_name(self) -> &'static str {
        match self {
            Self::Queued => "backlog",
            Self::Active => "in-progress",
            Self::PostPicker => "picker-completed",
            Self::InReview => "in-review",
            Self::Merged => "merged",
            Self::Failed => "failed",
            Self::Abandoned => "abandoned",
        }
    }

    /// Parse from legacy status string.
    pub fn from_legacy(s: &str) -> Option<Self> {
        match s {
            "backlog" => Some(Self::Queued),
            "in-progress" => Some(Self::Active),
            "picker-completed" => Some(Self::PostPicker),
            "in-review" => Some(Self::InReview),
            "merged" => Some(Self::Merged),
            "failed" => Some(Self::Failed),
            "abandoned" => Some(Self::Abandoned),
            _ => None,
        }
    }
}

impl fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.legacy_name())
    }
}

/// Task priority level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    /// Background work, can wait.
    Low,
    /// Default priority.
    Normal,
    /// Should be dispatched ahead of normal tasks.
    High,
}

impl Priority {
    /// Parse from string, defaulting to Normal for unrecognized values.
    pub fn from_str_lossy(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "low" => Self::Low,
            "high" => Self::High,
            _ => Self::Normal,
        }
    }
}

impl fmt::Display for Priority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Low => f.write_str("low"),
            Self::Normal => f.write_str("normal"),
            Self::High => f.write_str("high"),
        }
    }
}
