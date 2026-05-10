//! `jam-task-lifecycle` - Tempyr task lifecycle reconciler (§4.6.2).
//!
//! Subscribes to lifecycle journal events, updates the canonical Tempyr task
//! node under `<canonical-worktree>/<graph-relpath>/tasks/<task-id>.md`, and
//! emits `journal.tempyr.task-updated`. Fine-grained operational history stays
//! in the append-only journal; these task nodes are only coarse durable state.

#![deny(missing_docs)]

use std::fs;
use std::path::{Component, Path, PathBuf};

use chrono::{DateTime, Utc};
use futures::StreamExt;
use jam_events::generated::{Event, TempyrTaskUpdated};
use jam_events::EventEnvelope;
use jam_nats::async_nats;
use jam_nats::JamNats;
use jam_trace::TraceCtx;
use serde::Deserialize;
use serde_yaml::{Mapping, Value};
use tracing::{debug, error, info, warn};

const SERVICE_NAME: &str = "jam-task-lifecycle";
const SERVICE_VERSION: &str = env!("CARGO_PKG_VERSION");
const DEFAULT_CANONICAL_WORKTREE: &str = "/home/caleb/blueberry-jam";
const DEFAULT_GRAPH_RELPATH: &str = "graph";
const TASK_ID_MAX_LEN: usize = 128;

#[derive(Debug, thiserror::Error)]
enum ServiceError {
    #[error("nats: {0}")]
    Nats(#[from] jam_nats::NatsError),

    #[error("subscribe: {0}")]
    Subscribe(String),
}

#[derive(Debug, thiserror::Error)]
enum LifecycleError {
    #[error("{kind}: {detail}")]
    Protocol {
        kind: &'static str,
        detail: String,
        remediation: &'static str,
        tracked_by: &'static str,
    },
}

impl LifecycleError {
    fn protocol(
        kind: &'static str,
        detail: impl Into<String>,
        remediation: &'static str,
        tracked_by: &'static str,
    ) -> Self {
        Self::Protocol {
            kind,
            detail: detail.into(),
            remediation,
            tracked_by,
        }
    }
}

#[derive(Clone)]
struct LifecycleState {
    config: LifecycleConfig,
}

#[derive(Debug, Clone)]
struct LifecycleConfig {
    canonical_worktree: PathBuf,
    graph_relpath: PathBuf,
}

impl LifecycleConfig {
    fn from_env() -> Self {
        let canonical_worktree = std::env::var_os("JAM_CANONICAL_TEMPYR_WORKTREE")
            .or_else(|| std::env::var_os("JAM_TEMPYR_WORKTREE"))
            .map_or_else(|| PathBuf::from(DEFAULT_CANONICAL_WORKTREE), PathBuf::from);
        let graph_relpath = std::env::var_os("JAM_GRAPH_RELPATH")
            .map_or_else(|| PathBuf::from(DEFAULT_GRAPH_RELPATH), PathBuf::from);
        Self {
            canonical_worktree,
            graph_relpath,
        }
    }

    fn task_dir(&self) -> Result<PathBuf, LifecycleError> {
        validate_graph_relpath(&self.graph_relpath)?;
        Ok(self
            .canonical_worktree
            .join(&self.graph_relpath)
            .join("tasks"))
    }
}

#[derive(Debug, Deserialize)]
struct JournalEnvelope {
    event_type: String,
    timestamp: DateTime<Utc>,
    trace_id: String,
    #[serde(default)]
    parent_trace_id: Option<String>,
    payload: serde_json::Value,
}

#[derive(Debug)]
struct UpdateResult {
    task_id: String,
    status: String,
    task_path: PathBuf,
    source_event_type: String,
    updated_at: DateTime<Utc>,
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    if let Err(err) = run().await {
        error!("jam-task-lifecycle fatal: {err}");
        return std::process::ExitCode::FAILURE;
    }
    std::process::ExitCode::SUCCESS
}

async fn run() -> Result<(), ServiceError> {
    init_tracing();

    let nats_url = std::env::var("NATS_URL").unwrap_or_else(|_| "nats://127.0.0.1:4222".into());
    let nats_token = std::env::var("NATS_TOKEN").ok();
    let config = LifecycleConfig::from_env();

    info!(
        service = %SERVICE_NAME,
        version = %SERVICE_VERSION,
        nats = %nats_url,
        canonical_worktree = %config.canonical_worktree.display(),
        graph_relpath = %config.graph_relpath.display(),
        "starting",
    );

    let nats = JamNats::connect(&nats_url, nats_token).await?;
    info!("connected to NATS");

    let state = LifecycleState { config };
    let mut sub = nats
        .client()
        .subscribe("journal.>")
        .await
        .map_err(|e| ServiceError::Subscribe(e.to_string()))?;
    info!(subject = "journal.>", "subscribed");

    let shutdown = tokio::signal::ctrl_c();
    tokio::pin!(shutdown);

    loop {
        tokio::select! {
            _ = &mut shutdown => {
                info!("shutdown signal received");
                return Ok(());
            }
            msg = sub.next() => {
                let Some(message) = msg else {
                    warn!("subscriber stream closed");
                    return Ok(());
                };
                let nats = nats.clone();
                let state = state.clone();
                tokio::spawn(async move {
                    if let Err(err) = handle_message(&nats, &message, &state).await {
                        warn!(subject = %message.subject, "handle: {err}");
                    }
                });
            }
        }
    }
}

async fn handle_message(
    nats: &JamNats,
    msg: &async_nats::Message,
    state: &LifecycleState,
) -> Result<(), LifecycleError> {
    let Some(ctx) = msg
        .headers
        .as_ref()
        .and_then(jam_nats::extract_trace_from_headers)
    else {
        return Err(LifecycleError::protocol(
            "missing-trace",
            "lifecycle journal event arrived without Trace-Id headers",
            "Use traced publish wrappers for all journal events.",
            "principle-tracing-chains-end-to-end",
        ));
    };
    let envelope = parse_envelope(&msg.payload)?;
    debug!(event_type = %envelope.event_type, "received lifecycle event");

    let Some(result) = apply_lifecycle_event(&state.config, &envelope)? else {
        return Ok(());
    };
    publish_task_updated(nats, &result, &ctx).await?;
    Ok(())
}

fn parse_envelope(payload: &[u8]) -> Result<JournalEnvelope, LifecycleError> {
    serde_json::from_slice(payload).map_err(|err| {
        LifecycleError::protocol(
            "invalid-envelope",
            format!("journal payload is not a valid event envelope: {err}"),
            "Verify publishers use jam-events EventEnvelope.",
            "principle-failure-surfaces-immediately",
        )
    })
}

fn apply_lifecycle_event(
    config: &LifecycleConfig,
    envelope: &JournalEnvelope,
) -> Result<Option<UpdateResult>, LifecycleError> {
    let transition = match envelope.event_type.as_str() {
        "picker.spawned" => Some(Transition::PickerSpawned),
        "picker.exited" => Some(Transition::PickerExited),
        "pr.opened" => Some(Transition::PrOpened),
        "pr.merged" => Some(Transition::PrMerged),
        "task.failed" => Some(Transition::TaskFailed),
        "task.abandoned" => Some(Transition::TaskAbandoned),
        _ => None,
    };
    let Some(transition) = transition else {
        return Ok(None);
    };
    let task_id = value_string(&envelope.payload, "task_id").ok_or_else(|| {
        LifecycleError::protocol(
            "missing-task-id",
            format!("{} payload has no task_id", envelope.event_type),
            "Fix the event publisher so lifecycle events include task_id.",
            "comp-task-lifecycle-handler",
        )
    })?;
    validate_task_id(&task_id)?;

    let task_dir = config.task_dir()?;
    fs::create_dir_all(&task_dir).map_err(|err| {
        LifecycleError::protocol(
            "task-dir-create-failed",
            format!("failed to create {}: {err}", task_dir.display()),
            "Create the canonical Tempyr graph tasks directory with writable permissions.",
            "comp-task-lifecycle-handler",
        )
    })?;
    let task_path = task_dir.join(format!("{task_id}.md"));
    let mut node = TaskNode::load_or_new(&task_path, &task_id, envelope.timestamp)?;
    transition.apply(&mut node, envelope);
    node.set_str("updated", Utc::now().to_rfc3339());
    node.set_str("last-updated", Utc::now().to_rfc3339());
    node.write(&task_path)?;

    Ok(Some(UpdateResult {
        task_id,
        status: node.string("status").unwrap_or_else(|| "unknown".into()),
        task_path,
        source_event_type: envelope.event_type.clone(),
        updated_at: Utc::now(),
    }))
}

enum Transition {
    PickerSpawned,
    PickerExited,
    PrOpened,
    PrMerged,
    TaskFailed,
    TaskAbandoned,
}

impl Transition {
    fn apply(&self, node: &mut TaskNode, envelope: &JournalEnvelope) {
        match self {
            Self::PickerSpawned => {
                node.set_str("status", "in-progress");
                copy_string(node, &envelope.payload, "spawned_at", "spawned-at");
                copy_string(node, &envelope.payload, "session_id", "session-id");
                copy_string(node, &envelope.payload, "session_id", "picker-handle");
                copy_string(node, &envelope.payload, "worktree_path", "worktree-path");
                copy_string(node, &envelope.payload, "harness", "harness");
                copy_string(
                    node,
                    &envelope.payload,
                    "picker_trace_id",
                    "picker-trace-id",
                );
                copy_string(node, &envelope.payload, "maestro_trace_id", "trace-id");
                if let Some(parent) = &envelope.parent_trace_id {
                    node.set_str("parent-trace-id", parent);
                }
                node.set_str("journal-trace-id", &envelope.trace_id);
            }
            Self::PickerExited => {
                copy_string(node, &envelope.payload, "session_id", "session-id");
                copy_string(node, &envelope.payload, "exit_code", "exit-code");
                copy_string(node, &envelope.payload, "exited_at", "exited-at");
                copy_string(node, &envelope.payload, "duration_ms", "duration-ms");

                let status = node.string("status");
                if !matches!(status.as_deref(), Some("in-review" | "merged")) {
                    let exit_code = envelope
                        .payload
                        .get("exit_code")
                        .and_then(serde_json::Value::as_u64);
                    if exit_code == Some(0) {
                        node.set_str("status", "picker-completed");
                        node.set_str("outcome", "picker-exited-zero");
                    } else {
                        node.set_str("status", "failed");
                        node.set_str("outcome", "picker-exited-nonzero");
                    }
                }
            }
            Self::PrOpened => {
                node.set_str("status", "in-review");
                copy_string(node, &envelope.payload, "pr_ref", "pr-ref");
                copy_string(node, &envelope.payload, "branch", "pr-branch");
                copy_string(node, &envelope.payload, "title", "pr-title");
                copy_string(node, &envelope.payload, "opened_at", "pr-opened-at");
                copy_bool(node, &envelope.payload, "draft", "pr-draft");
            }
            Self::PrMerged => {
                node.set_str("status", "merged");
                node.set_str("outcome", "merged");
                copy_string(node, &envelope.payload, "pr_ref", "pr-ref");
                copy_string(node, &envelope.payload, "merged_sha", "merged-sha");
                copy_string(node, &envelope.payload, "merged_by", "merged-by");
                copy_string(node, &envelope.payload, "merged_at", "merged-at");
                copy_string(node, &envelope.payload, "touched_paths", "touched-paths");
            }
            Self::TaskFailed => {
                node.set_str("status", "failed");
                copy_string(node, &envelope.payload, "reason", "outcome");
                copy_string(node, &envelope.payload, "reason", "failure-reason");
                copy_string(node, &envelope.payload, "detail", "failure-detail");
                copy_string(node, &envelope.payload, "failed_at", "failed-at");
                copy_string(
                    node,
                    &envelope.payload,
                    "source_event_type",
                    "failure-source",
                );
            }
            Self::TaskAbandoned => {
                node.set_str("status", "abandoned");
                copy_string(node, &envelope.payload, "reason", "outcome");
                copy_string(node, &envelope.payload, "reason", "abandoned-reason");
                copy_string(node, &envelope.payload, "abandoned_at", "abandoned-at");
            }
        }
    }
}

struct TaskNode {
    frontmatter: Mapping,
    body: String,
}

impl TaskNode {
    fn load_or_new(
        path: &Path,
        task_id: &str,
        created_at: DateTime<Utc>,
    ) -> Result<Self, LifecycleError> {
        if path.exists() {
            return Self::load(path);
        }
        let mut frontmatter = Mapping::new();
        insert_str(&mut frontmatter, "id", task_id);
        insert_str(&mut frontmatter, "type", "task");
        insert_str(&mut frontmatter, "status", "backlog");
        insert_str(&mut frontmatter, "created", created_at.to_rfc3339());
        insert_str(&mut frontmatter, "updated", created_at.to_rfc3339());
        insert_value(&mut frontmatter, "edges", Value::Sequence(Vec::new()));
        Ok(Self {
            frontmatter,
            body: "Lifecycle task node maintained by jam-task-lifecycle from orchestrator journal events.\n".into(),
        })
    }

    fn load(path: &Path) -> Result<Self, LifecycleError> {
        let raw = fs::read_to_string(path).map_err(|err| {
            LifecycleError::protocol(
                "task-read-failed",
                format!("failed to read {}: {err}", path.display()),
                "Fix canonical Tempyr worktree permissions.",
                "comp-task-lifecycle-handler",
            )
        })?;
        let (frontmatter, body) = parse_frontmatter(&raw)?;
        Ok(Self { frontmatter, body })
    }

    fn write(&self, path: &Path) -> Result<(), LifecycleError> {
        let yaml = serde_yaml::to_string(&self.frontmatter).map_err(|err| {
            LifecycleError::protocol(
                "task-serialize-failed",
                format!("failed to serialize task frontmatter: {err}"),
                "Fix the task lifecycle field values.",
                "comp-task-lifecycle-handler",
            )
        })?;
        let rendered = format!("---\n{}---\n{}", yaml, self.body);
        fs::write(path, rendered).map_err(|err| {
            LifecycleError::protocol(
                "task-write-failed",
                format!("failed to write {}: {err}", path.display()),
                "Fix canonical Tempyr worktree permissions.",
                "comp-task-lifecycle-handler",
            )
        })
    }

    fn set_str(&mut self, key: &str, value: impl AsRef<str>) {
        insert_str(&mut self.frontmatter, key, value.as_ref());
    }

    fn set_bool(&mut self, key: &str, value: bool) {
        insert_value(&mut self.frontmatter, key, Value::Bool(value));
    }

    fn string(&self, key: &str) -> Option<String> {
        self.frontmatter
            .get(Value::String(key.into()))?
            .as_str()
            .map(ToOwned::to_owned)
    }
}

async fn publish_task_updated(
    nats: &JamNats,
    result: &UpdateResult,
    ctx: &TraceCtx,
) -> Result<(), LifecycleError> {
    let payload = TempyrTaskUpdated {
        task_id: result.task_id.clone(),
        status: result.status.clone(),
        task_path: result.task_path.to_string_lossy().into_owned(),
        source_event_type: result.source_event_type.clone(),
        updated_at: result.updated_at,
    };
    let envelope = EventEnvelope::new(
        TempyrTaskUpdated::EVENT_TYPE,
        TempyrTaskUpdated::EVENT_SUBTYPE_VERSION,
        0,
        ctx.trace_id.to_string(),
        SERVICE_NAME,
        payload,
    );
    nats.publish_traced("journal.tempyr.task-updated", &envelope, ctx)
        .await
        .map_err(|err| {
            LifecycleError::protocol(
                "journal-publish-failed",
                err.to_string(),
                "Verify NATS is running and the journal bridge is healthy.",
                "principle-failure-surfaces-immediately",
            )
        })
}

fn parse_frontmatter(raw: &str) -> Result<(Mapping, String), LifecycleError> {
    let Some(rest) = raw.strip_prefix("---\n") else {
        return Err(LifecycleError::protocol(
            "task-frontmatter-invalid",
            "task file does not start with YAML frontmatter",
            "Repair the task node or remove it so the reconciler can recreate it.",
            "comp-task-lifecycle-handler",
        ));
    };
    let Some((yaml, body)) = rest.split_once("\n---\n") else {
        return Err(LifecycleError::protocol(
            "task-frontmatter-invalid",
            "task file frontmatter is not closed",
            "Repair the task node or remove it so the reconciler can recreate it.",
            "comp-task-lifecycle-handler",
        ));
    };
    let frontmatter = serde_yaml::from_str::<Mapping>(yaml).map_err(|err| {
        LifecycleError::protocol(
            "task-frontmatter-invalid",
            format!("task frontmatter is invalid YAML: {err}"),
            "Repair the task node or remove it so the reconciler can recreate it.",
            "comp-task-lifecycle-handler",
        )
    })?;
    Ok((frontmatter, body.to_owned()))
}

fn copy_string(node: &mut TaskNode, payload: &serde_json::Value, from: &str, to: &str) {
    if let Some(value) = value_string(payload, from) {
        node.set_str(to, value);
    }
}

fn copy_bool(node: &mut TaskNode, payload: &serde_json::Value, from: &str, to: &str) {
    if let Some(value) = payload.get(from).and_then(serde_json::Value::as_bool) {
        node.set_bool(to, value);
    }
}

fn value_string(payload: &serde_json::Value, field: &str) -> Option<String> {
    payload
        .get(field)?
        .as_str()
        .map(ToOwned::to_owned)
        .or_else(|| payload.get(field).map(ToString::to_string))
}

fn insert_str(mapping: &mut Mapping, key: &str, value: impl AsRef<str>) {
    insert_value(mapping, key, Value::String(value.as_ref().into()));
}

fn insert_value(mapping: &mut Mapping, key: &str, value: Value) {
    mapping.insert(Value::String(key.into()), value);
}

fn validate_task_id(task_id: &str) -> Result<(), LifecycleError> {
    if task_id.is_empty() || task_id.len() > TASK_ID_MAX_LEN {
        return Err(LifecycleError::protocol(
            "invalid-task-id",
            "task_id must be 1-128 characters",
            "Use the task_id emitted by jam task spawn.",
            "comp-task-lifecycle-handler",
        ));
    }
    if task_id == "." || task_id == ".." || task_id.contains("..") {
        return Err(LifecycleError::protocol(
            "invalid-task-id",
            format!("task_id may not contain parent-directory segments: {task_id}"),
            "Use a slug-like task_id with letters, numbers, dots, underscores, and dashes.",
            "comp-task-lifecycle-handler",
        ));
    }
    if !task_id
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(LifecycleError::protocol(
            "invalid-task-id",
            format!("task_id contains unsafe characters: {task_id}"),
            "Use a slug-like task_id with letters, numbers, dots, underscores, and dashes.",
            "comp-task-lifecycle-handler",
        ));
    }
    Ok(())
}

fn validate_graph_relpath(path: &Path) -> Result<(), LifecycleError> {
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::CurDir | Component::Prefix(_)
            )
        })
    {
        return Err(LifecycleError::protocol(
            "invalid-graph-relpath",
            format!(
                "graph relpath must be relative and native: {}",
                path.display()
            ),
            "Set JAM_GRAPH_RELPATH to graph for Blueberry.",
            "comp-task-lifecycle-handler",
        ));
    }
    Ok(())
}

fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("jam_task_lifecycle=info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn spawned_event_creates_task_node() {
        let tmp = TempDir::new().unwrap();
        let config = test_config(tmp.path());
        let envelope = envelope(
            "picker.spawned",
            serde_json::json!({
                "task_id": "task-1",
                "session_id": "codex-cli:abc",
                "harness": "codex-cli",
                "worktree_path": "/tmp/worktrees/task-1",
                "spawned_at": "2026-05-06T05:00:00Z",
                "picker_trace_id": "01HXKJVT2K8MN7P9R5SRZWB6JCN",
                "maestro_trace_id": "01HXKJVF7P4N6X5R8SRZWB6JCM"
            }),
        );

        let result = apply_lifecycle_event(&config, &envelope).unwrap().unwrap();
        let raw = fs::read_to_string(result.task_path).unwrap();

        assert!(raw.contains("status: in-progress"));
        assert!(raw.contains("picker-handle: codex-cli:abc"));
        assert!(raw.contains("worktree-path: /tmp/worktrees/task-1"));
    }

    #[test]
    fn pr_opened_updates_existing_task_node() {
        let tmp = TempDir::new().unwrap();
        let config = test_config(tmp.path());
        apply_lifecycle_event(
            &config,
            &envelope(
                "picker.spawned",
                serde_json::json!({
                    "task_id": "task-1",
                    "session_id": "codex-cli:abc",
                    "worktree_path": "/tmp/worktrees/task-1"
                }),
            ),
        )
        .unwrap();

        let result = apply_lifecycle_event(
            &config,
            &envelope(
                "pr.opened",
                serde_json::json!({
                    "task_id": "task-1",
                    "pr_ref": "cleak/blueberry#42",
                    "branch": "task/task-1",
                    "title": "Task 1",
                    "draft": true,
                    "opened_at": "2026-05-06T05:10:00Z"
                }),
            ),
        )
        .unwrap()
        .unwrap();
        let raw = fs::read_to_string(result.task_path).unwrap();

        assert!(raw.contains("status: in-review"));
        assert!(raw.contains("pr-ref: cleak/blueberry#42"));
        assert!(raw.contains("pr-draft: true"));
    }

    #[test]
    fn picker_exit_updates_task_node_when_no_pr_exists() {
        let tmp = TempDir::new().unwrap();
        let config = test_config(tmp.path());
        apply_lifecycle_event(
            &config,
            &envelope(
                "picker.spawned",
                serde_json::json!({
                    "task_id": "task-1",
                    "session_id": "codex-cli:abc",
                    "worktree_path": "/tmp/worktrees/task-1"
                }),
            ),
        )
        .unwrap();

        let result = apply_lifecycle_event(
            &config,
            &envelope(
                "picker.exited",
                serde_json::json!({
                    "task_id": "task-1",
                    "session_id": "codex-cli:abc",
                    "exit_code": 1,
                    "exited_at": "2026-05-06T05:10:00Z",
                    "duration_ms": 1000
                }),
            ),
        )
        .unwrap()
        .unwrap();
        let raw = fs::read_to_string(result.task_path).unwrap();

        assert!(raw.contains("status: failed"));
        assert!(raw.contains("outcome: picker-exited-nonzero"));
        assert!(raw.contains("exit-code: '1'"));
    }

    #[test]
    fn task_failed_marks_task_failed_before_picker_spawn() {
        let tmp = TempDir::new().unwrap();
        let config = test_config(tmp.path());
        let result = apply_lifecycle_event(
            &config,
            &envelope(
                "task.failed",
                serde_json::json!({
                    "task_id": "task-1",
                    "reason": "harness-version-drift",
                    "detail": "codex-cli version drifted",
                    "failed_at": "2026-05-09T07:06:26Z",
                    "source_event_type": "maestro.spawn-picker-error"
                }),
            ),
        )
        .unwrap()
        .unwrap();
        let raw = fs::read_to_string(result.task_path).unwrap();

        assert!(raw.contains("status: failed"));
        assert!(raw.contains("outcome: harness-version-drift"));
        assert!(raw.contains("failure-detail: codex-cli version drifted"));
        assert!(raw.contains("failure-source: maestro.spawn-picker-error"));
    }

    #[test]
    fn picker_exit_keeps_in_review_task_in_review() {
        let tmp = TempDir::new().unwrap();
        let config = test_config(tmp.path());
        apply_lifecycle_event(
            &config,
            &envelope(
                "pr.opened",
                serde_json::json!({
                    "task_id": "task-1",
                    "pr_ref": "cleak/blueberry#42"
                }),
            ),
        )
        .unwrap();

        let result = apply_lifecycle_event(
            &config,
            &envelope(
                "picker.exited",
                serde_json::json!({
                    "task_id": "task-1",
                    "session_id": "codex-cli:abc",
                    "exit_code": 0,
                    "exited_at": "2026-05-06T05:10:00Z",
                    "duration_ms": 1000
                }),
            ),
        )
        .unwrap()
        .unwrap();
        let raw = fs::read_to_string(result.task_path).unwrap();

        assert!(raw.contains("status: in-review"));
        assert!(raw.contains("exit-code: '0'"));
    }

    #[test]
    fn merge_updates_status_and_outcome() {
        let tmp = TempDir::new().unwrap();
        let config = test_config(tmp.path());
        let result = apply_lifecycle_event(
            &config,
            &envelope(
                "pr.merged",
                serde_json::json!({
                    "task_id": "task-1",
                    "pr_ref": "cleak/blueberry#42",
                    "merged_sha": "abc123",
                    "merged_by": "caleb",
                    "merged_at": "2026-05-06T05:20:00Z"
                }),
            ),
        )
        .unwrap()
        .unwrap();
        let raw = fs::read_to_string(result.task_path).unwrap();

        assert!(raw.contains("status: merged"));
        assert!(raw.contains("outcome: merged"));
        assert!(raw.contains("merged-sha: abc123"));
    }

    fn test_config(root: &Path) -> LifecycleConfig {
        LifecycleConfig {
            canonical_worktree: root.to_path_buf(),
            graph_relpath: PathBuf::from("graph"),
        }
    }

    fn envelope(event_type: &str, payload: serde_json::Value) -> JournalEnvelope {
        JournalEnvelope {
            event_type: event_type.into(),
            timestamp: DateTime::parse_from_rfc3339("2026-05-06T05:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
            trace_id: "01HXKJVF7P4N6X5R8SRZWB6JCM".into(),
            parent_trace_id: None,
            payload,
        }
    }
}
