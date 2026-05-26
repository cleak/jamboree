//! `jam-svc-session` - Picker session lifecycle service (§24.3).
//!
//! Picker session lifecycle service: traced NATS request-reply on
//! `tool.session.spawn-picker`, worktree creation delegated through
//! `jam-svc-worktree`, local and Docker sandbox backends, harness launch,
//! message delivery, and Picker lifecycle journal emission.

#![deny(missing_docs)]

use std::collections::HashMap;
use std::ffi::OsString;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::{Command as StdCommand, Output, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use chrono::{DateTime, Utc};
use futures::StreamExt;
use jam_events::generated::{
    Event, PickerExited, PickerFirstOutput, PickerKilled, PickerSpawned, QuotaExhausted,
    QuotaUsageObserved, TaskAbandoned,
};
use jam_events::EventEnvelope;
use jam_nats::async_nats;
use jam_nats::JamNats;
use jam_secrets::{ExposeSecret, FileBackend, PassBackend, SecretBackend, SecretKey, SecretString};
use jam_trace::TraceCtx;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

const SERVICE_NAME: &str = "jam-svc-session";
const SERVICE_VERSION: &str = env!("CARGO_PKG_VERSION");
const SUBJECT_PREFIX: &str = "tool.session";
const SUBJECT_PREFIX_ENV: &str = "JAM_SESSION_SUBJECT_PREFIX";
const DEFAULT_HARNESS: &str = "codex-cli";
const CLAUDE_HARNESS: &str = "claude-code";
const OPENCODE_HARNESS: &str = "opencode-deepseek";
const LIVE_HARNESSES: &[&str] = &[DEFAULT_HARNESS, CLAUDE_HARNESS, OPENCODE_HARNESS];
const DRY_RUN_HARNESSES: &[&str] = &["codex-cli", "claude-code", "opencode-deepseek"];
const DEFAULT_PROJECT: &str = "blueberry";
const JAMBOREE_PROJECT: &str = "jamboree";
const DEFAULT_SANDBOX_BACKEND: &str = "local";
const DOCKER_SANDBOX_BACKEND: &str = "docker";
const DEFAULT_SANDBOX_PROFILE: &str = "default";
const HARDENED_SANDBOX_PROFILE: &str = "hardened";
const DEFAULT_TASK_CLASS: &str = "light-edit";
const DEFAULT_WORKTREE_SUBJECT: &str = "tool.worktree.create";
const DEFAULT_REPO_OPEN_PR_SUBJECT: &str = "tool.repo.open-pr";
const DEFAULT_JAMBOREE_GITHUB_REPO: &str = "cleak/jamboree";
const DEFAULT_CODEX_BIN: &str = "codex";
const DEFAULT_CLAUDE_BIN: &str = "claude";
const DEFAULT_OPENCODE_BIN: &str = "opencode";
const DEFAULT_DOCKER_BIN: &str = "docker";
const DEFAULT_SHELL_BIN: &str = "/bin/sh";
const DEFAULT_DOCKER_IMAGE: &str = "jam-picker:latest";
const DEFAULT_SYSTEMD_RUN_BIN: &str = "systemd-run";
const DEFAULT_IONICE_BIN: &str = "ionice";
const DEFAULT_CODEX_MODEL: &str = "gpt-5.5";
const DEFAULT_CODEX_REASONING_EFFORT: &str = "medium";
const DEFAULT_OPENCODE_MODEL: &str = "deepseek/deepseek-v4-pro";
const DEFAULT_OPENCODE_SMALL_MODEL: &str = "deepseek/deepseek-v4-flash";
const DEFAULT_PICKER_HOME: &str = "/home/picker";
const DEFAULT_CODEX_HOME: &str = "/home/maestro/.codex";
const DEFAULT_SUDO_BIN: &str = "sudo";
const DEFAULT_TRUNK_BRANCH: &str = "master";
const DEFAULT_JAMBOREE_TRUNK_BRANCH: &str = "main";
const DOCKER_WORKTREE_PATH: &str = "/work";
const DOCKER_REPO_GIT_PATH: &str = "/repo.git";
const CLAUDE_SETTINGS_REL: &[&str] = &[".claude", "settings.json"];
const CLAUDE_MCP_CONFIG_REL: &[&str] = &[".jam", "claude-mcp.json"];
const CODEX_EVENTS_REL: &[&str] = &[".jam", "codex-events.jsonl"];
const CLAUDE_EVENTS_REL: &[&str] = &[".jam", "claude-events.jsonl"];
const OPENCODE_RUNNER_REL: &[&str] = &[".jam", "opencode-runner.sh"];
const OPENCODE_PROMPT_REL: &[&str] = &[".jam", "opencode-prompt.txt"];
const OPENCODE_CONFIG_REL: &[&str] = &[".jam", "opencode.json"];
const OPENCODE_EVENTS_REL: &[&str] = &[".jam", "opencode-events.jsonl"];
const PR_TITLE_REL: &[&str] = &[".jam", "pr-title.txt"];
const PR_BODY_REL: &[&str] = &[".jam", "pr-body.md"];
const TEMPYR_BOOTSTRAP_COMMAND: &str = "tempyr journal bootstrap --quiet";
const TEMPYR_CLAUDE_FINALIZE_COMMAND: &str = "tempyr journal finalize --agent claude --quiet";
const DEEPSEEK_SECRET_ENV: &str = "DEEPSEEK_API_KEY";
const TASK_ID_MAX_LEN: usize = 128;
const TOKEN_MAX_LEN: usize = 128;
const DEFAULT_REQUEST_TIMEOUT_SECS: u64 = 60;
const DEFAULT_KILL_GRACE_MS: u64 = 2_000;
const MESSAGE_TEXT_MAX_LEN: usize = 8_000;
const DEFAULT_LOCAL_MEMORY_MAX: &str = "8G";
const PICKER_OUTPUT_LINE_MAX_CHARS: usize = 8_000;
const PR_TITLE_MAX_LEN: usize = 240;
const PR_BODY_MAX_LEN: usize = 65_536;

#[derive(Debug, thiserror::Error)]
enum ServiceError {
    #[error("nats: {0}")]
    Nats(#[from] jam_nats::NatsError),

    #[error("subscribe: {0}")]
    Subscribe(String),

    #[error("reply: {0}")]
    Reply(String),
}

#[derive(Debug, thiserror::Error)]
enum SessionError {
    #[error("{kind}: {detail}")]
    Protocol {
        kind: &'static str,
        detail: String,
        remediation: &'static str,
        tracked_by: &'static str,
    },
}

impl SessionError {
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
struct SessionState {
    config: SessionConfig,
    active: Arc<Mutex<HashMap<String, PickerRecord>>>,
    routing: jam_nats::RoutingResolver,
}

#[derive(Debug, Clone)]
struct SessionConfig {
    worktree_subject: String,
    repo_open_pr_subject: String,
    lockfile_path: PathBuf,
    harness_lockfile_policy: HarnessLockfilePolicy,
    git_bin: PathBuf,
    codex_bin: PathBuf,
    claude_bin: PathBuf,
    opencode_bin: PathBuf,
    docker_bin: PathBuf,
    docker_image: String,
    systemd_run_bin: PathBuf,
    ionice_bin: PathBuf,
    opencode_model: String,
    opencode_small_model: String,
    project_config_path: PathBuf,
    session_log_root: PathBuf,
    secrets_file: Option<PathBuf>,
    picker_home: PathBuf,
    codex_home: PathBuf,
    picker_path: OsString,
    sudo_bin: PathBuf,
    use_sudo: bool,
    use_systemd_scope: bool,
    request_timeout: Duration,
    kill_grace: Duration,
    dry_run_command: Vec<String>,
    open_pr_on_success: bool,
    pr_draft: bool,
    trunk_branch: String,
    jamboree_github_repo: String,
    jamboree_trunk_branch: String,
}

impl SessionConfig {
    fn from_env() -> Self {
        let worktree_subject = std::env::var("JAM_WORKTREE_CREATE_SUBJECT")
            .unwrap_or_else(|_| DEFAULT_WORKTREE_SUBJECT.into());
        let repo_open_pr_subject = std::env::var("JAM_REPO_OPEN_PR_SUBJECT")
            .unwrap_or_else(|_| DEFAULT_REPO_OPEN_PR_SUBJECT.into());
        let lockfile_path = std::env::var_os("JAM_HARNESS_LOCKFILE")
            .map_or_else(default_harness_lockfile_path, PathBuf::from);
        let harness_lockfile_policy = HarnessLockfilePolicy::from_env();
        let git_bin = std::env::var_os("JAM_GIT_BIN").map_or_else(|| "git".into(), PathBuf::from);
        let codex_bin = std::env::var_os("JAM_CODEX_BIN")
            .map_or_else(|| DEFAULT_CODEX_BIN.into(), PathBuf::from);
        let claude_bin = std::env::var_os("JAM_CLAUDE_BIN")
            .map_or_else(|| DEFAULT_CLAUDE_BIN.into(), PathBuf::from);
        let opencode_bin = std::env::var_os("JAM_OPENCODE_BIN")
            .map_or_else(|| DEFAULT_OPENCODE_BIN.into(), PathBuf::from);
        let docker_bin = std::env::var_os("JAM_DOCKER_BIN")
            .map_or_else(|| DEFAULT_DOCKER_BIN.into(), PathBuf::from);
        let docker_image =
            std::env::var("JAM_DOCKER_IMAGE").unwrap_or_else(|_| DEFAULT_DOCKER_IMAGE.into());
        let systemd_run_bin = std::env::var_os("JAM_SYSTEMD_RUN_BIN")
            .map_or_else(|| DEFAULT_SYSTEMD_RUN_BIN.into(), PathBuf::from);
        let ionice_bin = std::env::var_os("JAM_IONICE_BIN")
            .map_or_else(|| DEFAULT_IONICE_BIN.into(), PathBuf::from);
        let opencode_model =
            std::env::var("JAM_OPENCODE_MODEL").unwrap_or_else(|_| DEFAULT_OPENCODE_MODEL.into());
        let opencode_small_model = std::env::var("JAM_OPENCODE_SMALL_MODEL")
            .unwrap_or_else(|_| DEFAULT_OPENCODE_SMALL_MODEL.into());
        let project_config_path = std::env::var_os("JAM_PROJECT_CONFIG")
            .map_or_else(default_project_config_path, PathBuf::from);
        let session_log_root = std::env::var_os("JAM_SESSION_LOG_ROOT")
            .map_or_else(default_session_log_root, PathBuf::from);
        let secrets_file = std::env::var_os("JAM_SECRETS_FILE").map(PathBuf::from);
        let picker_home = std::env::var_os("JAM_PICKER_HOME")
            .map_or_else(|| PathBuf::from(DEFAULT_PICKER_HOME), PathBuf::from);
        let codex_home = std::env::var_os("JAM_CODEX_HOME")
            .map_or_else(|| PathBuf::from(DEFAULT_CODEX_HOME), PathBuf::from);
        let picker_path = std::env::var_os("JAM_PICKER_PATH").unwrap_or_else(default_picker_path);
        let sudo_bin =
            std::env::var_os("JAM_SUDO_BIN").map_or_else(|| DEFAULT_SUDO_BIN.into(), PathBuf::from);
        let use_sudo = parse_bool_env("JAM_SESSION_USE_SUDO")
            .unwrap_or_else(|| std::env::var("USER").is_ok_and(|user| user == "maestro"));
        let use_systemd_scope = parse_bool_env("JAM_SESSION_USE_SYSTEMD_SCOPE").unwrap_or(true);
        let request_timeout = std::env::var("JAM_SESSION_REQUEST_TIMEOUT_SECS")
            .ok()
            .and_then(|raw| raw.parse().ok())
            .map_or(
                Duration::from_secs(DEFAULT_REQUEST_TIMEOUT_SECS),
                Duration::from_secs,
            );
        let kill_grace = std::env::var("JAM_SESSION_KILL_GRACE_MS")
            .ok()
            .and_then(|raw| raw.parse().ok())
            .map_or(
                Duration::from_millis(DEFAULT_KILL_GRACE_MS),
                Duration::from_millis,
            );
        let dry_run_command = std::env::var("JAM_SESSION_DRY_RUN_COMMAND")
            .map_or_else(|_| default_dry_run_command(), |raw| shell_words(&raw));
        let dry_run_command = if dry_run_command.is_empty() {
            default_dry_run_command()
        } else {
            dry_run_command
        };
        let open_pr_on_success = parse_bool_env("JAM_SESSION_OPEN_PR_ON_SUCCESS").unwrap_or(true);
        let pr_draft = parse_bool_env("JAM_SESSION_OPEN_PR_DRAFT").unwrap_or(false);
        let trunk_branch =
            std::env::var("JAM_TRUNK_BRANCH").unwrap_or_else(|_| DEFAULT_TRUNK_BRANCH.into());
        let jamboree_github_repo = std::env::var("JAM_JAMBOREE_GITHUB_REPO")
            .unwrap_or_else(|_| DEFAULT_JAMBOREE_GITHUB_REPO.into());
        let jamboree_trunk_branch = std::env::var("JAM_JAMBOREE_TRUNK_BRANCH")
            .unwrap_or_else(|_| DEFAULT_JAMBOREE_TRUNK_BRANCH.into());
        Self {
            worktree_subject,
            repo_open_pr_subject,
            lockfile_path,
            harness_lockfile_policy,
            git_bin,
            codex_bin,
            claude_bin,
            opencode_bin,
            docker_bin,
            docker_image,
            systemd_run_bin,
            ionice_bin,
            opencode_model,
            opencode_small_model,
            project_config_path,
            session_log_root,
            secrets_file,
            picker_home,
            codex_home,
            picker_path,
            sudo_bin,
            use_sudo,
            use_systemd_scope,
            request_timeout,
            kill_grace,
            dry_run_command,
            open_pr_on_success,
            pr_draft,
            trunk_branch,
            jamboree_github_repo,
            jamboree_trunk_branch,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HarnessLockfilePolicy {
    Strict,
    Warn,
    Off,
}

impl HarnessLockfilePolicy {
    fn from_env() -> Self {
        let raw = std::env::var("JAM_HARNESS_LOCKFILE_POLICY")
            .or_else(|_| std::env::var("JAM_HARNESS_DRIFT_POLICY"))
            .unwrap_or_else(|_| "warn".into());
        match raw.trim().to_ascii_lowercase().as_str() {
            "strict" | "fail" | "block" => Self::Strict,
            "warn" | "warning" | "" => Self::Warn,
            "off" | "disabled" | "disable" | "none" => Self::Off,
            other => {
                warn!(
                    policy = other,
                    "unknown harness lockfile policy; using warn",
                );
                Self::Warn
            }
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Strict => "strict",
            Self::Warn => "warn",
            Self::Off => "off",
        }
    }
}

fn default_harness_lockfile_path() -> PathBuf {
    jam_tools_core::paths::jam_home()
        .join("config")
        .join("projects")
        .join("blueberry-harnesses.lock")
}

fn default_project_config_path() -> PathBuf {
    jam_tools_core::paths::jam_home()
        .join("config")
        .join("projects")
        .join("blueberry.toml")
}

fn default_session_log_root() -> PathBuf {
    jam_tools_core::paths::jam_home().join("session-logs")
}

#[derive(Debug, Deserialize)]
struct SpawnPickerInput {
    task_id: String,
    project: Option<String>,
    harness: Option<String>,
    sandbox_backend: Option<String>,
    sandbox_profile: Option<String>,
    task_class: Option<String>,
    initial_prompt: Option<String>,
    model_override: Option<String>,
    reasoning_effort: Option<String>,
    budget_usd: Option<f64>,
    dry_run: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
struct WorktreeCreateInput {
    task_id: String,
    project: String,
    repo_path: Option<String>,
    worktree_root: Option<String>,
    trunk_branch: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WorktreeCreateOutput {
    task_id: String,
    project: String,
    worktree_path: String,
}

#[derive(Debug, Serialize)]
struct RepoOpenPrInput {
    task_id: String,
    branch: String,
    title: String,
    body: String,
    repo: Option<String>,
    draft: bool,
    base: String,
    worktree_path: String,
    push: bool,
}

struct PickerPrMetadata {
    title: String,
    body: String,
}

#[derive(Debug, Serialize, Clone)]
struct PickerHandle {
    session_id: String,
    task_id: String,
    project: String,
    harness: String,
    worktree_path: String,
    picker_trace_id: String,
    maestro_trace_id: String,
    sandbox_backend: String,
    sandbox_profile: String,
    task_class: String,
    picker_pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    resource_scope: Option<String>,
    spawned_at: DateTime<Utc>,
    dry_run: bool,
    /// session_id of the prior picker session this one resumes, when set.
    /// Surfaced on `picker.spawned` so the coordinator can chain continuation
    /// attempts and the loop-guard can count them.
    #[serde(skip_serializing_if = "Option::is_none")]
    parent_session_id: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
struct PickerRecord {
    #[serde(flatten)]
    handle: PickerHandle,
    status: PickerStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    exited_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    exit_code: Option<i32>,
}

#[derive(Debug, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum PickerStatus {
    Running,
    Killing,
    Killed,
    Exited,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum PickerMessageMode {
    Queue,
    Interrupt,
    FullStop,
}

impl PickerMessageMode {
    fn subject_token(self) -> &'static str {
        match self {
            Self::Queue => "queue",
            Self::Interrupt => "interrupt",
            Self::FullStop => "kill",
        }
    }
}

#[derive(Debug, Deserialize)]
struct PickerMessagePayload {
    message_id: String,
    session_id: String,
    mode: PickerMessageMode,
    text: Option<String>,
    from: String,
}

#[derive(Debug, Serialize)]
struct PickerMessageStatusPayload<'a> {
    message_id: &'a str,
    session_id: &'a str,
    mode: PickerMessageMode,
    status: &'static str,
    from: &'a str,
    detail: &'a serde_json::Value,
    updated_at: DateTime<Utc>,
}

struct PickerMessageStatusUpdate<'a> {
    session_id: &'a str,
    mode: PickerMessageMode,
    message_id: &'a str,
    status: &'static str,
    from: &'a str,
    detail: &'a serde_json::Value,
}

#[derive(Debug, Serialize, Clone)]
struct PickerOutputRecord {
    session_id: String,
    task_id: String,
    trace_id: String,
    stream: &'static str,
    line: String,
    ts: DateTime<Utc>,
    sequence: u64,
    truncated: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
struct QuotaUsageObservation {
    provider: Option<String>,
    model: Option<String>,
    input_tokens: u64,
    output_tokens: u64,
    cost_usd: Option<f64>,
    source: &'static str,
}

impl QuotaUsageObservation {
    fn has_usage(&self) -> bool {
        self.input_tokens > 0 || self.output_tokens > 0 || self.cost_usd.is_some()
    }

    fn merge(&mut self, other: Self) {
        if self.provider.is_none() {
            self.provider = other.provider;
        }
        if self.model.is_none() {
            self.model = other.model;
        }
        self.input_tokens = self.input_tokens.saturating_add(other.input_tokens);
        self.output_tokens = self.output_tokens.saturating_add(other.output_tokens);
        self.cost_usd = match (self.cost_usd, other.cost_usd) {
            (Some(left), Some(right)) => Some(left + right),
            (Some(left), None) => Some(left),
            (None, Some(right)) => Some(right),
            (None, None) => None,
        };
    }
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum Response {
    Ok(serde_json::Value),
    Error { error: ResponseError },
}

#[derive(Debug, Serialize)]
struct ResponseError {
    kind: String,
    detail: String,
    remediation: String,
    tracked_by: &'static str,
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    if let Err(err) = run().await {
        error!("jam-svc-session fatal: {err}");
        return std::process::ExitCode::FAILURE;
    }
    std::process::ExitCode::SUCCESS
}

async fn run() -> Result<(), ServiceError> {
    init_tracing();

    let nats_url = std::env::var("NATS_URL").unwrap_or_else(|_| "nats://127.0.0.1:4222".into());
    let nats_token = std::env::var("NATS_TOKEN").ok();
    let subject_prefix = configured_subject_prefix(SUBJECT_PREFIX_ENV, SUBJECT_PREFIX);
    let config = SessionConfig::from_env();

    info!(
        service = %SERVICE_NAME,
        version = %SERVICE_VERSION,
        nats = %nats_url,
        subject_prefix = %subject_prefix,
        worktree_subject = %config.worktree_subject,
        lockfile = %config.lockfile_path.display(),
        harness_lockfile_policy = %config.harness_lockfile_policy.as_str(),
        codex_bin = %config.codex_bin.display(),
        claude_bin = %config.claude_bin.display(),
        opencode_bin = %config.opencode_bin.display(),
        docker_bin = %config.docker_bin.display(),
        docker_image = %config.docker_image,
        systemd_run_bin = %config.systemd_run_bin.display(),
        opencode_model = %config.opencode_model,
        project_config = %config.project_config_path.display(),
        session_log_root = %config.session_log_root.display(),
        use_sudo = config.use_sudo,
        use_systemd_scope = config.use_systemd_scope,
        "starting",
    );

    let nats = JamNats::connect(&nats_url, nats_token).await?;
    info!("connected to NATS");

    let routing = jam_nats::RoutingResolver::new(nats.jetstream().clone());
    // Warm the cache so the first inter-service call doesn't pay the slow
    // path. Non-fatal if it fails (no manifest yet → fallback subjects).
    if let Err(err) = routing.refresh().await {
        info!(error = %err, "routing manifest refresh failed at startup; will retry on demand");
    }

    let state = SessionState {
        config,
        active: Arc::new(Mutex::new(HashMap::new())),
        routing,
    };

    let mut sub = nats
        .client()
        .subscribe(format!("{subject_prefix}.>"))
        .await
        .map_err(|e| ServiceError::Subscribe(e.to_string()))?;
    info!(subject = %format!("{subject_prefix}.>"), "subscribed");

    // Also subscribe to routing-manifest.updated so our cached subject
    // lookups for downstream services (worktree, repo, …) follow each
    // patch-agent hot-swap. Without this, sessions started before a
    // downstream redeploy would keep publishing to the previous version's
    // subject — which has no subscribers after drain.
    let mut routing_updates = nats
        .client()
        .subscribe(jam_nats::ROUTING_MANIFEST_UPDATED_SUBJECT)
        .await
        .map_err(|e| ServiceError::Subscribe(e.to_string()))?;
    info!(
        subject = jam_nats::ROUTING_MANIFEST_UPDATED_SUBJECT,
        "subscribed to routing manifest updates"
    );

    let shutdown = tokio::signal::ctrl_c();
    tokio::pin!(shutdown);
    let draining = Arc::new(AtomicBool::new(false));
    let active_requests = Arc::new(AtomicUsize::new(0));
    let mut drain_check = tokio::time::interval(Duration::from_millis(100));
    drain_check.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            _ = &mut shutdown => {
                info!("shutdown signal received");
                return Ok(());
            }
            _ = drain_check.tick(), if draining.load(Ordering::SeqCst) && active_requests.load(Ordering::SeqCst) == 0 => {
                info!("drain complete; exiting");
                return Ok(());
            }
            update = routing_updates.next() => {
                if update.is_none() {
                    warn!("routing-manifest.updated subscription closed");
                    continue;
                }
                if let Err(err) = state.routing.refresh().await {
                    warn!(error = %err, "routing manifest refresh failed; will retry on next update");
                }
            }
            msg = sub.next() => {
                let Some(message) = msg else {
                    warn!("subscriber stream closed");
                    return Ok(());
                };
                let nats = nats.clone();
                let state = state.clone();
                let draining = draining.clone();
                let active_requests = active_requests.clone();
                active_requests.fetch_add(1, Ordering::SeqCst);
                tokio::spawn(async move {
                    let result = handle_request(&nats, &message, &state, &draining).await;
                    active_requests.fetch_sub(1, Ordering::SeqCst);
                    if let Err(err) = result {
                        warn!(subject = %message.subject, "handle: {err}");
                    }
                });
            }
        }
    }
}

async fn handle_request(
    nats: &JamNats,
    msg: &async_nats::Message,
    state: &SessionState,
    draining: &Arc<AtomicBool>,
) -> Result<(), ServiceError> {
    let method = method_from_subject(msg.subject.as_str()).unwrap_or("");
    debug!(subject = %msg.subject, method = %method, "received request");

    let response_ctx = msg
        .headers
        .as_ref()
        .and_then(jam_nats::extract_trace_from_headers);

    let response = match &response_ctx {
        Some(ctx) => dispatch(method, &msg.payload, state, ctx, nats).await,
        None => Response::Error {
            error: ResponseError {
                kind: "missing-trace".into(),
                detail: "tool.session requests must include Trace-Id headers".into(),
                remediation: "Use JamNats::request_traced for tool calls.".into(),
                tracked_by: "principle-tracing-chains-end-to-end",
            },
        },
    };

    let Some(reply_subject) = msg.reply.as_ref() else {
        warn!(subject = %msg.subject, method = %method, "no reply subject");
        return Ok(());
    };

    let payload = serde_json::to_vec(&response).expect("response always serializes");
    if let Some(ctx) = response_ctx {
        nats.publish_bytes_traced(reply_subject.to_string(), Bytes::from(payload), &ctx)
            .await?;
    } else {
        return Err(ServiceError::Reply(
            "missing Trace-Id; refusing untraced reply publish".into(),
        ));
    }
    if method == "drain" {
        draining.store(true, Ordering::SeqCst);
    }
    Ok(())
}

async fn dispatch(
    method: &str,
    payload: &[u8],
    state: &SessionState,
    ctx: &TraceCtx,
    nats: &JamNats,
) -> Response {
    match method {
        "ping" => Response::Ok(serde_json::json!({
            "status": "ok",
            "service": SERVICE_NAME,
            "version": SERVICE_VERSION,
        })),
        "drain" => Response::Ok(serde_json::json!({
            "status": "draining",
            "service": SERVICE_NAME,
            "version": SERVICE_VERSION,
        })),
        "spawn-picker" => match spawn_picker(payload, state, ctx, nats).await {
            Ok(handle) => Response::Ok(serde_json::to_value(handle).expect("handle serializes")),
            Err(err) => error_response(err),
        },
        "resume-picker" => match resume_picker(payload, state, ctx, nats).await {
            Ok(handle) => Response::Ok(serde_json::to_value(handle).expect("handle serializes")),
            Err(err) => error_response(err),
        },
        "inspect-picker" => match inspect_picker(payload, state).await {
            Ok(record) => Response::Ok(serde_json::to_value(record).expect("record serializes")),
            Err(err) => error_response(err),
        },
        "full-stop" => match full_stop_picker(payload, state, ctx, nats).await {
            Ok(outcome) => Response::Ok(serde_json::to_value(outcome).expect("outcome serializes")),
            Err(err) => error_response(err),
        },
        "list-active" => {
            let records = list_active(state).await;
            Response::Ok(serde_json::json!({ "sessions": records }))
        }
        "archive-session" => match archive_session(payload, state, ctx, nats).await {
            Ok(output) => Response::Ok(serde_json::to_value(output).expect("output serializes")),
            Err(err) => error_response(err),
        },
        "purge-session" => match purge_session(payload, state, ctx, nats).await {
            Ok(output) => Response::Ok(serde_json::to_value(output).expect("output serializes")),
            Err(err) => error_response(err),
        },
        unknown => Response::Error {
            error: ResponseError {
                kind: "unknown-method".into(),
                detail: format!("{SUBJECT_PREFIX}.{unknown} is not a recognized session method"),
                remediation: "Use tool.session.spawn-picker.".into(),
                tracked_by: "graph/components/comp-jam-svc-session.md",
            },
        },
    }
}

async fn spawn_picker(
    payload: &[u8],
    state: &SessionState,
    ctx: &TraceCtx,
    nats: &JamNats,
) -> Result<PickerHandle, SessionError> {
    let input = parse_spawn_input(payload)?;
    let spec = SpawnSpec::from_input(input)?;

    if !spec.dry_run && state.config.harness_lockfile_policy != HarnessLockfilePolicy::Off {
        if let Err(err) = verify_harness_lockfile(
            &spec.harness,
            harness_bin(&state.config, &spec.harness),
            &state.config.lockfile_path,
        ) {
            if harness_lockfile_error_blocks(state.config.harness_lockfile_policy, &err) {
                return Err(err);
            }
            let SessionError::Protocol {
                kind,
                detail,
                remediation,
                tracked_by,
            } = &err;
            warn!(
                task_id = %spec.task_id,
                harness = %spec.harness,
                error_kind = *kind,
                detail = %detail,
                remediation = *remediation,
                tracked_by = *tracked_by,
                "harness lockfile drift detected; continuing because policy is warn",
            );
        }
    }

    let picker_trace = TraceCtx::child(
        ctx,
        "session.spawn-picker",
        format!("spawn Picker for {}", spec.task_id),
    );
    let worktree = create_worktree(nats, state, &spec, &picker_trace).await?;
    let worktree_path = validate_worktree_path(&worktree.worktree_path)?;

    if worktree.task_id != spec.task_id || worktree.project != spec.project {
        return Err(SessionError::protocol(
            "worktree-response-mismatch",
            format!(
                "worktree response was for task_id={} project={}, expected task_id={} project={}",
                worktree.task_id, worktree.project, spec.task_id, spec.project
            ),
            "Retry the spawn after verifying jam-svc-worktree is healthy.",
            "principle-failure-surfaces-immediately",
        ));
    }

    launch_picker(spec, worktree_path, picker_trace, state, ctx, nats).await
}

/// Resume an earlier picker session in the worktree associated with `task_id`.
/// Skips worktree creation and lockfile drift checks because the worktree
/// already exists. See
/// graph/decisions/dec-post-picker-coordination.md.
async fn resume_picker(
    payload: &[u8],
    state: &SessionState,
    ctx: &TraceCtx,
    nats: &JamNats,
) -> Result<PickerHandle, SessionError> {
    let input: ResumePickerInput = serde_json::from_slice(payload).map_err(|err| {
        SessionError::protocol(
            "invalid-input",
            format!("tool.session.resume-picker payload is invalid JSON: {err}"),
            "Send {\"task_id\":\"...\",\"prompt\":\"...\"}.",
            "comp-jam-svc-session-resume",
        )
    })?;
    let spec = SpawnSpec::for_resume(input)?;

    let picker_trace = TraceCtx::child(
        ctx,
        "session.resume-picker",
        format!("resume Picker for {}", spec.task_id),
    );

    // Derive worktree path from the standard convention. Refuse if the
    // directory is not readable by picker — that signals an inconsistency
    // (worktree purged, task_id misspelled, etc.) which should be loud.
    let worktree_root = std::env::var_os("JAM_WORKTREE_ROOT")
        .map_or_else(|| PathBuf::from("/home/picker/workers"), PathBuf::from);
    let raw_path = worktree_root.join(&spec.task_id);
    let worktree_path = validate_worktree_path(&raw_path.to_string_lossy())?;

    launch_picker(spec, worktree_path, picker_trace, state, ctx, nats).await
}

/// Body shared by `spawn_picker` and `resume_picker`. Writes picker metadata
/// into the worktree, builds the launch command for the configured harness,
/// spawns the process, and registers all the watchers + journal events.
async fn launch_picker(
    spec: SpawnSpec,
    worktree_path: PathBuf,
    picker_trace: TraceCtx,
    state: &SessionState,
    ctx: &TraceCtx,
    nats: &JamNats,
) -> Result<PickerHandle, SessionError> {
    write_picker_metadata(&worktree_path, &spec, &picker_trace, ctx)?;

    let session_id = format!("{}:{}", spec.harness, jam_trace::TraceId::new());
    prepare_harness_worktree(&state.config, &spec, &worktree_path, &session_id)?;
    let resource_scope = resource_scope_for_spec(&state.config, &spec, &session_id);
    let mut command = build_launch_command(
        &state.config,
        &spec,
        &worktree_path,
        &session_id,
        &picker_trace,
        ctx,
    )?;
    let output_log = open_picker_output_log(&state.config.session_log_root, &session_id)?;
    let mut child = command.spawn().map_err(|err| {
        SessionError::protocol(
            "picker-launch-failed",
            format!("failed to launch {}: {err}", spec.harness),
            "Verify Codex CLI is installed for the configured runtime user and sudoers allows the transition.",
            "task-jam-svc-session-codex-cli-only",
        )
    })?;
    let child_stdin = child.stdin.take();
    let child_stdout = child.stdout.take();
    let child_stderr = child.stderr.take();
    let picker_pid = child.id();
    let spawned_at = Utc::now();

    let handle = PickerHandle {
        session_id: session_id.clone(),
        task_id: spec.task_id.clone(),
        project: spec.project.clone(),
        harness: spec.harness.clone(),
        worktree_path: worktree_path.to_string_lossy().into_owned(),
        picker_trace_id: picker_trace.trace_id.to_string(),
        maestro_trace_id: ctx.trace_id.to_string(),
        sandbox_backend: spec.sandbox_backend.clone(),
        sandbox_profile: spec.sandbox_profile.clone(),
        task_class: spec.task_class.clone(),
        picker_pid,
        resource_scope,
        spawned_at,
        dry_run: spec.dry_run,
        parent_session_id: spec.parent_session_id.clone(),
    };

    publish_picker_spawned(nats, &handle, &picker_trace, ctx).await?;
    let quota_exhausted_flag = watch_picker_output(
        nats.clone(),
        handle.clone(),
        picker_trace.clone(),
        ctx.clone(),
        child_stdout,
        child_stderr,
        output_log,
    );
    state.active.lock().await.insert(
        session_id.clone(),
        PickerRecord {
            handle: handle.clone(),
            status: PickerStatus::Running,
            exited_at: None,
            exit_code: None,
        },
    );

    if let Some(stdin) = child_stdin {
        let stdin = Arc::new(Mutex::new(stdin));
        watch_picker_messages(nats, &state.active, &session_id, &stdin).await;
    } else {
        warn!(
            session_id = %session_id,
            "Picker stdin is unavailable; queue/interrupt delivery will fail"
        );
    }

    watch_picker_exit(
        child,
        state.active.clone(),
        nats.clone(),
        state.config.clone(),
        state.routing.clone(),
        handle.clone(),
        picker_trace,
        ctx.clone(),
        quota_exhausted_flag,
    );

    Ok(handle)
}

#[derive(Debug)]
struct SpawnSpec {
    task_id: String,
    project: String,
    harness: String,
    sandbox_backend: String,
    sandbox_profile: String,
    task_class: String,
    initial_prompt: String,
    model_override: Option<String>,
    reasoning_effort: Option<String>,
    budget_usd: Option<f64>,
    dry_run: bool,
    /// True when this spawn is a continuation of an earlier picker session
    /// in the same worktree. Causes Codex to use `exec resume --last` and
    /// Claude Code to use `--continue`; both are cwd-filtered by the worktree.
    /// Worktree creation and harness lockfile verification are skipped because
    /// the worktree must already exist from the original spawn.
    resume_from_last: bool,
    /// session_id of the picker we're resuming, journaled on the new
    /// `picker.spawned` event as `parent_session_id` so the coordinator
    /// can chain continuation attempts.
    parent_session_id: Option<String>,
}

impl SpawnSpec {
    fn from_input(input: SpawnPickerInput) -> Result<Self, SessionError> {
        validate_token("task_id", &input.task_id, TASK_ID_MAX_LEN)?;
        let project = input.project.unwrap_or_else(|| DEFAULT_PROJECT.into());
        if !supported_project(&project) {
            return Err(SessionError::protocol(
                "unsupported-project",
                format!("supported task targets are blueberry and jamboree, got {project}"),
                "Choose the explicit Blueberry or Jamboree target when creating the task.",
                "api-spawn-picker",
            ));
        }

        let dry_run = input.dry_run.unwrap_or(false);
        let harness = input.harness.unwrap_or_else(|| DEFAULT_HARNESS.into());
        validate_harness(&harness, dry_run)?;

        let sandbox_backend = input
            .sandbox_backend
            .unwrap_or_else(|| DEFAULT_SANDBOX_BACKEND.into());
        if !matches!(
            sandbox_backend.as_str(),
            DEFAULT_SANDBOX_BACKEND | DOCKER_SANDBOX_BACKEND
        ) {
            return Err(SessionError::protocol(
                "unsupported-sandbox-backend",
                format!("session spawn supports local and docker backends, got {sandbox_backend}"),
                "Use sandbox_backend=local or sandbox_backend=docker.",
                "task-vendor-hermes-docker-backend",
            ));
        }

        let sandbox_profile = input
            .sandbox_profile
            .unwrap_or_else(|| DEFAULT_SANDBOX_PROFILE.into());
        if !matches!(
            sandbox_profile.as_str(),
            DEFAULT_SANDBOX_PROFILE | HARDENED_SANDBOX_PROFILE
        ) {
            return Err(SessionError::protocol(
                "unsupported-sandbox-profile",
                format!(
                    "session spawn supports default and hardened profiles, got {sandbox_profile}"
                ),
                "Use sandbox_profile=default or sandbox_profile=hardened.",
                "task-hardened-profile",
            ));
        }
        if sandbox_backend == DEFAULT_SANDBOX_BACKEND && sandbox_profile == HARDENED_SANDBOX_PROFILE
        {
            return Err(SessionError::protocol(
                "unsupported-sandbox-combination",
                "hardened profile currently requires sandbox_backend=docker",
                "Use sandbox_backend=docker for hardened Pickers until the local allowlist proxy lands.",
                "task-hardened-profile",
            ));
        }

        let task_class = input
            .task_class
            .unwrap_or_else(|| DEFAULT_TASK_CLASS.into());
        validate_token("task_class", &task_class, TOKEN_MAX_LEN)?;
        if let Some(model) = input.model_override.as_deref() {
            validate_model_id("model_override", model, TOKEN_MAX_LEN)?;
        }
        if let Some(effort) = input.reasoning_effort.as_deref() {
            validate_token("reasoning_effort", effort, TOKEN_MAX_LEN)?;
        }
        if matches!(input.budget_usd, Some(budget) if !budget.is_finite() || budget < 0.0) {
            return Err(SessionError::protocol(
                "invalid-budget",
                "budget_usd must be a finite non-negative number",
                "Send a non-negative budget_usd or omit the field.",
                "task-jam-svc-session-codex-cli-only",
            ));
        }

        let initial_prompt = input.initial_prompt.unwrap_or_else(|| {
            format!(
                "Work on Jamboree task {} in the current worktree. Keep changes focused and open a PR when the task is complete.",
                input.task_id
            )
        });
        if initial_prompt.trim().is_empty() {
            return Err(SessionError::protocol(
                "invalid-prompt",
                "initial_prompt must not be empty",
                "Send a concise task prompt for the Picker.",
                "task-jam-svc-session-codex-cli-only",
            ));
        }
        let initial_prompt =
            prompt_with_pr_metadata_instructions(initial_prompt.trim(), &input.task_id);

        Ok(Self {
            task_id: input.task_id,
            project,
            harness,
            sandbox_backend,
            sandbox_profile,
            task_class,
            initial_prompt,
            model_override: input.model_override,
            reasoning_effort: input.reasoning_effort,
            budget_usd: input.budget_usd,
            dry_run,
            resume_from_last: false,
            parent_session_id: None,
        })
    }

    /// Build a SpawnSpec for resuming a previous session in the same worktree.
    /// `prompt` becomes the new user message; the harness decides how to find
    /// the prior conversation from the worktree.
    fn for_resume(input: ResumePickerInput) -> Result<Self, SessionError> {
        validate_token("task_id", &input.task_id, TASK_ID_MAX_LEN)?;
        if input.prompt.trim().is_empty() {
            return Err(SessionError::protocol(
                "invalid-prompt",
                "resume-picker prompt must not be empty",
                "Send a non-empty prompt for the resumed session.",
                "comp-jam-svc-session-resume",
            ));
        }
        let project = input.project.unwrap_or_else(|| DEFAULT_PROJECT.into());
        if !supported_project(&project) {
            return Err(SessionError::protocol(
                "unsupported-project",
                format!("supported task targets are blueberry and jamboree, got {project}"),
                "Resume with the explicit Blueberry or Jamboree project.",
                "comp-jam-svc-session-resume",
            ));
        }
        let harness = input
            .harness
            .or_else(|| {
                harness_from_session_id(input.parent_session_id.as_deref()).map(ToOwned::to_owned)
            })
            .unwrap_or_else(|| DEFAULT_HARNESS.into());
        validate_resume_harness(&harness)?;
        let task_class = input
            .task_class
            .unwrap_or_else(|| DEFAULT_TASK_CLASS.into());
        validate_token("task_class", &task_class, TOKEN_MAX_LEN)?;
        let prompt = prompt_with_pr_metadata_instructions(input.prompt.trim(), &input.task_id);
        Ok(Self {
            task_id: input.task_id,
            project,
            harness,
            sandbox_backend: DEFAULT_SANDBOX_BACKEND.into(),
            sandbox_profile: DEFAULT_SANDBOX_PROFILE.into(),
            task_class,
            initial_prompt: prompt,
            model_override: None,
            reasoning_effort: None,
            budget_usd: None,
            dry_run: false,
            resume_from_last: true,
            parent_session_id: input.parent_session_id,
        })
    }
}

#[derive(Debug, Deserialize)]
struct ResumePickerInput {
    task_id: String,
    prompt: String,
    /// Project target for the existing worktree.
    #[serde(default)]
    project: Option<String>,
    /// Harness to resume. Defaults from parent_session_id prefix, then Codex.
    #[serde(default)]
    harness: Option<String>,
    /// Original picker session_id we're resuming. Optional today (the
    /// coordinator supplies it when it has the picker.exited event in hand),
    /// journaled on the resumed `picker.spawned` as `parent_session_id`.
    #[serde(default)]
    parent_session_id: Option<String>,
    /// Optional task_class hint carried forward from the original spawn.
    #[serde(default)]
    task_class: Option<String>,
}

fn harness_from_session_id(session_id: Option<&str>) -> Option<&str> {
    let session_id = session_id?;
    session_id.split_once(':').map(|(harness, _)| harness)
}

fn validate_resume_harness(harness: &str) -> Result<(), SessionError> {
    validate_harness(harness, false)?;
    if harness == DEFAULT_HARNESS || harness == CLAUDE_HARNESS {
        return Ok(());
    }
    Err(SessionError::protocol(
        "unsupported-resume-harness",
        format!("resume-picker supports codex-cli and claude-code, got {harness}"),
        "Resume a Codex CLI or Claude Code Picker, or start a fresh Picker for this harness.",
        "comp-jam-svc-session-resume",
    ))
}

fn validate_harness(harness: &str, dry_run: bool) -> Result<(), SessionError> {
    validate_token("harness", harness, TOKEN_MAX_LEN)?;
    if !dry_run && !LIVE_HARNESSES.contains(&harness) {
        return Err(SessionError::protocol(
            "unsupported-harness",
            format!(
                "live spawn supports codex-cli, claude-code, and opencode-deepseek, got {harness}"
            ),
            "Use codex-cli, claude-code, or opencode-deepseek for live spawns.",
            if harness == OPENCODE_HARNESS {
                "task-opencode-deepseek-adapter-impl"
            } else {
                "task-claude-code-adapter-impl"
            },
        ));
    }
    if dry_run && !DRY_RUN_HARNESSES.contains(&harness) {
        return Err(SessionError::protocol(
            "unsupported-harness",
            format!("dry-run spawn does not recognize harness {harness}"),
            "Use codex-cli, claude-code, or opencode-deepseek.",
            "task-dispatch-policy-quota-skill-driven",
        ));
    }
    Ok(())
}

fn supported_project(project: &str) -> bool {
    matches!(project, DEFAULT_PROJECT | JAMBOREE_PROJECT)
}

fn prompt_with_pr_metadata_instructions(prompt: &str, task_id: &str) -> String {
    format!(
        "{prompt}\n\n\
Jamboree PR metadata requirements:\n\
- Before you finish, write .jam/pr-title.txt and .jam/pr-body.md in the worktree.\n\
- .jam/pr-title.txt must be one concise human-readable line describing what the PR changes. Do not include the task id, session id, raw log text, branch name, or the [jam] prefix; Jamboree adds [jam] deterministically.\n\
- .jam/pr-body.md must be Markdown for reviewers. Focus on what the PR does and why. Include a short Summary section and a Verification section with commands run and results. Mention risks or follow-ups only when they matter.\n\
- Do not use generic automation prose such as \"Automated Jamboree PR\". Do not paste logs or IDs as the description.\n\
- These metadata files are required for Jamboree to open the PR for task {task_id}."
    )
}

async fn create_worktree(
    nats: &JamNats,
    state: &SessionState,
    spec: &SpawnSpec,
    picker_trace: &TraceCtx,
) -> Result<WorktreeCreateOutput, SessionError> {
    let request = WorktreeCreateInput {
        task_id: spec.task_id.clone(),
        project: spec.project.clone(),
        repo_path: None,
        worktree_root: None,
        trunk_branch: None,
    };
    // Resolve via the routing manifest so we hit the currently-deployed
    // worktree's versioned subject, not the unversioned `tool.worktree.create`
    // (which has no subscriber once patch-agent has moved worktree to a
    // versioned prefix).
    let subject = state.routing.subject_for("worktree", "create").await;
    let value: serde_json::Value = nats
        .request_traced(subject, &request, picker_trace, state.config.request_timeout)
        .await
        .map_err(|err| {
            SessionError::protocol(
                "worktree-request-failed",
                err.to_string(),
                "Verify jam-svc-worktree is running and subscribed to its routing-manifest subject_prefix.",
                "task-jam-svc-session-codex-cli-only",
            )
        })?;
    if let Some(error) = value.get("error") {
        return Err(SessionError::protocol(
            "worktree-create-failed",
            error.to_string(),
            "Fix the worktree service error, then retry spawn-picker.",
            "task-jam-svc-worktree-creation-protocol",
        ));
    }
    serde_json::from_value(value).map_err(|err| {
        SessionError::protocol(
            "worktree-response-invalid",
            format!("worktree response did not match expected schema: {err}"),
            "Upgrade jam-svc-worktree and jam-svc-session together.",
            "principle-failure-surfaces-immediately",
        )
    })
}

fn build_launch_command(
    config: &SessionConfig,
    spec: &SpawnSpec,
    worktree_path: &Path,
    session_id: &str,
    picker_trace: &TraceCtx,
    parent_trace: &TraceCtx,
) -> Result<Command, SessionError> {
    let env = PickerEnv::new(
        config,
        spec,
        worktree_path,
        session_id,
        picker_trace,
        parent_trace,
    )?;
    if spec.sandbox_backend == DOCKER_SANDBOX_BACKEND {
        return build_docker_launch_command(config, spec, worktree_path, session_id, &env);
    }
    if config.use_sudo {
        let mut command = Command::new(&config.sudo_bin);
        command.arg("-n");
        command.arg("-u");
        command.arg("picker");
        command.arg(format!("--preserve-env={}", env.preserve_env()));
        command.arg("--");
        append_sudo_harness_args(&mut command, config, spec, worktree_path)?;
        let command = apply_local_resource_scope(command, config, spec, session_id);
        // The pre-exec chdir for `sudo` runs as maestro, which cannot traverse
        // picker-owned 0700 worktrees. Harnesses that need cwd under sudo must
        // enter the worktree after the user switch.
        let stdout_log = direct_stdout_log_path_for_harness(&spec.harness, worktree_path)
            .filter(|_| !spec.dry_run);
        let mut command = prepare_command(command, Path::new("/"), &env, stdout_log.as_deref())?;
        close_stdin_for_exec_harness(&mut command, spec);
        Ok(command)
    } else {
        let mut command = if spec.dry_run {
            Command::new(&config.dry_run_command[0])
        } else {
            Command::new(harness_launcher(config, spec, worktree_path))
        };
        if spec.dry_run {
            command.args(&config.dry_run_command[1..]);
        } else {
            append_live_harness_args(&mut command, config, spec, worktree_path)?;
        }
        let command = apply_local_resource_scope(command, config, spec, session_id);
        let stdout_log = direct_stdout_log_path_for_harness(&spec.harness, worktree_path)
            .filter(|_| !spec.dry_run);
        let mut command = prepare_command(command, worktree_path, &env, stdout_log.as_deref())?;
        close_stdin_for_exec_harness(&mut command, spec);
        Ok(command)
    }
}

fn close_stdin_for_exec_harness(command: &mut Command, spec: &SpawnSpec) {
    if spec.harness == DEFAULT_HARNESS && !spec.dry_run {
        command.stdin(Stdio::null());
    }
}

fn build_docker_launch_command(
    config: &SessionConfig,
    spec: &SpawnSpec,
    worktree_path: &Path,
    session_id: &str,
    env: &PickerEnv,
) -> Result<Command, SessionError> {
    let repo_git_path = git_common_dir_for_worktree(worktree_path, &spec.task_id)?;
    let container_worktree = Path::new(DOCKER_WORKTREE_PATH);
    let inner_argv = inner_launch_argv(config, spec, container_worktree)?;

    let mut command = Command::new(&config.docker_bin);
    command.arg("run");
    command.arg("--rm");
    command.arg("-i");
    command.arg("--init");
    command.arg("--read-only");
    command.arg("--tmpfs");
    command.arg("/tmp:rw,nosuid,nodev,size=1g");
    command.arg("--tmpfs");
    command.arg(format!(
        "{}:rw,nosuid,nodev,size=512m",
        docker_container_path(&config.picker_home, "HOME")?
    ));
    command.arg("--network");
    command.arg(docker_network_for_profile(&spec.sandbox_profile)?);
    command.arg("--workdir");
    command.arg(DOCKER_WORKTREE_PATH);
    command.arg("--entrypoint");
    command.arg("");
    command.arg("--label");
    command.arg(format!("org.jamboree.session={session_id}"));
    command.arg("--label");
    command.arg(format!("org.jamboree.task={}", spec.task_id));
    command.arg("--volume");
    command.arg(format!(
        "{}:{DOCKER_WORKTREE_PATH}:rw",
        docker_bind_path(worktree_path, "worktree")?
    ));
    command.arg("--volume");
    command.arg(format!(
        "{}:{DOCKER_REPO_GIT_PATH}:ro",
        docker_bind_path(&repo_git_path, "repo git dir")?
    ));
    for (key, _) in env.vars() {
        command.arg("--env");
        command.arg(key);
    }
    command.arg(&config.docker_image);
    command.args(inner_argv);

    let stdout_log =
        direct_stdout_log_path_for_harness(&spec.harness, worktree_path).filter(|_| !spec.dry_run);
    prepare_command(command, worktree_path, env, stdout_log.as_deref())
}

fn apply_local_resource_scope(
    command: Command,
    config: &SessionConfig,
    spec: &SpawnSpec,
    session_id: &str,
) -> Command {
    let Some(unit) = resource_scope_for_spec(config, spec, session_id) else {
        return command;
    };
    let limits = resource_limits_for_task_class(&spec.task_class);
    let std_command = command.as_std();

    let mut scoped = Command::new(&config.systemd_run_bin);
    scoped.arg("--user");
    scoped.arg("--scope");
    scoped.arg("--collect");
    scoped.arg("--quiet");
    scoped.arg("--unit");
    scoped.arg(unit.trim_end_matches(".scope"));
    scoped.arg(format!("--property=CPUQuota={}", limits.cpu_quota));
    scoped.arg(format!("--property=MemoryMax={}", limits.memory_max));
    scoped.arg(format!("--property=IOWeight={}", limits.io_weight));
    if limits.ionice_idle {
        scoped.arg(&config.ionice_bin);
        scoped.arg("-c");
        scoped.arg("3");
    }
    scoped.arg(std_command.get_program());
    scoped.args(std_command.get_args());
    scoped
}

fn resource_scope_for_spec(
    config: &SessionConfig,
    spec: &SpawnSpec,
    session_id: &str,
) -> Option<String> {
    if config.use_systemd_scope && spec.sandbox_backend == DEFAULT_SANDBOX_BACKEND {
        Some(format!("{}.scope", systemd_unit_for_session(session_id)))
    } else {
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ResourceLimits {
    cpu_quota: &'static str,
    memory_max: &'static str,
    io_weight: u16,
    ionice_idle: bool,
}

fn resource_limits_for_task_class(task_class: &str) -> ResourceLimits {
    let (cpu_quota, io_weight, ionice_idle) = match task_class {
        "compile-heavy-rust" | "gameplay-change" | "ecs-refactor" => ("800%", 100, false),
        "risky-architecture" => ("100%", 10, true),
        _ => ("200%", 100, false),
    };
    ResourceLimits {
        cpu_quota,
        memory_max: DEFAULT_LOCAL_MEMORY_MAX,
        io_weight,
        ionice_idle,
    }
}

fn systemd_unit_for_session(session_id: &str) -> String {
    let mut unit = String::from("jam-picker-");
    for ch in session_id.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
            unit.push(ch);
        } else {
            unit.push('-');
        }
        if unit.len() >= 80 {
            break;
        }
    }
    unit.trim_end_matches('-').to_owned()
}

fn inner_launch_argv(
    config: &SessionConfig,
    spec: &SpawnSpec,
    container_worktree: &Path,
) -> Result<Vec<OsString>, SessionError> {
    if spec.dry_run {
        return Ok(config.dry_run_command.iter().map(OsString::from).collect());
    }

    let launcher = if spec.harness == OPENCODE_HARNESS {
        opencode_runner_path(container_worktree)
    } else {
        harness_bin(config, &spec.harness).to_path_buf()
    };
    let mut command = Command::new(launcher);
    append_live_harness_args(&mut command, config, spec, container_worktree)?;
    let std_command = command.as_std();
    let mut argv = Vec::with_capacity(std_command.get_args().len() + 1);
    argv.push(std_command.get_program().to_os_string());
    argv.extend(std_command.get_args().map(OsString::from));
    Ok(argv)
}

fn docker_network_for_profile(profile: &str) -> Result<&'static str, SessionError> {
    match profile {
        DEFAULT_SANDBOX_PROFILE => Ok("bridge"),
        HARDENED_SANDBOX_PROFILE => Ok("none"),
        _ => Err(SessionError::protocol(
            "unsupported-sandbox-profile",
            format!("unsupported Docker sandbox profile {profile}"),
            "Use sandbox_profile=default or sandbox_profile=hardened.",
            "task-hardened-profile",
        )),
    }
}

fn docker_bind_path<'a>(path: &'a Path, label: &'static str) -> Result<&'a str, SessionError> {
    path.to_str().ok_or_else(|| {
        SessionError::protocol(
            "docker-path-invalid",
            format!("{label} path is not valid UTF-8: {}", path.display()),
            "Use Linux-native UTF-8 paths for Docker-backed Pickers.",
            "principle-native-fs-only",
        )
    })
}

fn docker_container_path<'a>(path: &'a Path, label: &'static str) -> Result<&'a str, SessionError> {
    let raw = docker_bind_path(path, label)?;
    if !raw.starts_with('/') || raw.contains(':') || raw.contains(',') {
        return Err(SessionError::protocol(
            "docker-path-invalid",
            format!("{label} path is not a safe absolute container path: {raw}"),
            "Set the path to an absolute Linux path without Docker option separators.",
            "principle-native-fs-only",
        ));
    }
    Ok(raw)
}

fn git_common_dir_for_worktree(
    worktree_path: &Path,
    task_id: &str,
) -> Result<PathBuf, SessionError> {
    let output = run_git_common_dir(worktree_path)?;
    if !output.status.success() {
        repair_broken_linked_worktree(worktree_path, task_id)?;
        let retry = run_git_common_dir(worktree_path)?;
        if retry.status.success() {
            let raw = String::from_utf8_lossy(&retry.stdout);
            return PathBuf::from(raw.trim()).canonicalize().map_err(|err| {
                worktree_gitdir_error(format!(
                    "failed to canonicalize git common dir for {} after repair: {err}",
                    worktree_path.display()
                ))
            });
        }
        return Err(worktree_gitdir_error(format!(
            "git -C {} rev-parse --git-common-dir failed after repair: {}",
            worktree_path.display(),
            String::from_utf8_lossy(&retry.stderr).trim()
        )));
    }
    let raw = String::from_utf8_lossy(&output.stdout);
    PathBuf::from(raw.trim()).canonicalize().map_err(|err| {
        worktree_gitdir_error(format!(
            "failed to canonicalize git common dir for {}: {err}",
            worktree_path.display()
        ))
    })
}

fn run_git_common_dir(worktree_path: &Path) -> Result<Output, SessionError> {
    StdCommand::new("git")
        .arg("-C")
        .arg(worktree_path)
        .args(["rev-parse", "--path-format=absolute", "--git-common-dir"])
        .output()
        .map_err(|err| {
            worktree_gitdir_error(format!(
                "failed to inspect git metadata for {}: {err}",
                worktree_path.display()
            ))
        })
}

fn harness_bin<'a>(config: &'a SessionConfig, harness: &str) -> &'a Path {
    match harness {
        CLAUDE_HARNESS => &config.claude_bin,
        OPENCODE_HARNESS => &config.opencode_bin,
        _ => &config.codex_bin,
    }
}

fn harness_launcher(config: &SessionConfig, spec: &SpawnSpec, worktree_path: &Path) -> PathBuf {
    if spec.harness == OPENCODE_HARNESS {
        opencode_runner_path(worktree_path)
    } else {
        harness_bin(config, &spec.harness).to_path_buf()
    }
}

fn append_harness_args(
    command: &mut Command,
    config: &SessionConfig,
    spec: &SpawnSpec,
    worktree_path: &Path,
) -> Result<(), SessionError> {
    if spec.dry_run {
        command.arg(&config.dry_run_command[0]);
        command.args(&config.dry_run_command[1..]);
    } else {
        command.arg(harness_launcher(config, spec, worktree_path));
        append_live_harness_args(command, config, spec, worktree_path)?;
    }
    Ok(())
}

fn append_sudo_harness_args(
    command: &mut Command,
    config: &SessionConfig,
    spec: &SpawnSpec,
    worktree_path: &Path,
) -> Result<(), SessionError> {
    if spec.harness != CLAUDE_HARNESS || spec.dry_run {
        return append_harness_args(command, config, spec, worktree_path);
    }

    command.arg(DEFAULT_SHELL_BIN);
    command.arg("-lc");
    command.arg("cd \"$1\" && shift && exec \"$@\"");
    command.arg("sh");
    command.arg(worktree_path);
    command.arg(harness_launcher(config, spec, worktree_path));
    append_live_harness_args(command, config, spec, worktree_path)?;
    Ok(())
}

fn append_live_harness_args(
    command: &mut Command,
    config: &SessionConfig,
    spec: &SpawnSpec,
    worktree_path: &Path,
) -> Result<(), SessionError> {
    match spec.harness.as_str() {
        CLAUDE_HARNESS => append_claude_args(command, spec, worktree_path),
        OPENCODE_HARNESS => append_opencode_args(command, config, spec, worktree_path)?,
        _ => append_codex_args(command, spec, worktree_path),
    }
    Ok(())
}

fn append_codex_args(command: &mut Command, spec: &SpawnSpec, worktree_path: &Path) {
    command.arg("--ask-for-approval");
    command.arg("never");
    command.arg("exec");
    command.arg("--cd");
    command.arg(worktree_path);
    // local x default relies on the picker OS account as the boundary; Codex's
    // managed workspace-write sandbox keeps linked worktree gitdirs read-only.
    command.arg("--dangerously-bypass-approvals-and-sandbox");
    command.arg("--json");
    command.arg("--model");
    command.arg(spec.model_override.as_deref().unwrap_or(DEFAULT_CODEX_MODEL));
    command.arg("--config");
    command.arg(format!(
        "model_reasoning_effort=\"{}\"",
        spec.reasoning_effort.as_deref().unwrap_or(DEFAULT_CODEX_REASONING_EFFORT)
    ));
    if spec.resume_from_last {
        command.arg("resume");
        command.arg("--last");
    }
    command.arg(&spec.initial_prompt);
}

fn append_claude_args(command: &mut Command, spec: &SpawnSpec, worktree_path: &Path) {
    command.arg("--print");
    if spec.resume_from_last {
        command.arg("--continue");
    }
    command.arg("--add-dir");
    command.arg(worktree_path);
    command.arg("--verbose");
    command.arg("--output-format");
    command.arg("stream-json");
    command.arg("--permission-mode");
    command.arg("bypassPermissions");
    command.arg("--dangerously-skip-permissions");
    command.arg("--settings");
    command.arg(claude_settings_path(worktree_path));
    command.arg("--mcp-config");
    command.arg(claude_mcp_config_path(worktree_path));
    command.arg("--strict-mcp-config");
    if let Some(model) = &spec.model_override {
        command.arg("--model");
        command.arg(model);
    }
    if let Some(effort) = &spec.reasoning_effort {
        command.arg("--effort");
        command.arg(effort);
    }
    if let Some(budget) = spec.budget_usd {
        command.arg("--max-budget-usd");
        command.arg(budget.to_string());
    }
    command.arg(&spec.initial_prompt);
}

fn append_opencode_args(
    command: &mut Command,
    config: &SessionConfig,
    spec: &SpawnSpec,
    worktree_path: &Path,
) -> Result<(), SessionError> {
    command.arg(harness_bin(config, OPENCODE_HARNESS));
    command.arg(opencode_prompt_path(worktree_path));
    command.arg(worktree_path);
    command.arg(opencode_model_for_spec(config, spec)?);
    if let Some(variant) = &spec.reasoning_effort {
        command.arg(variant);
    }
    Ok(())
}

fn prepare_harness_worktree(
    config: &SessionConfig,
    spec: &SpawnSpec,
    worktree_path: &Path,
    _session_id: &str,
) -> Result<(), SessionError> {
    if spec.dry_run {
        return Ok(());
    }
    match spec.harness.as_str() {
        CLAUDE_HARNESS => {
            write_claude_settings(worktree_path)?;
            write_claude_mcp_config(config, worktree_path)
        }
        OPENCODE_HARNESS => prepare_opencode_worktree(config, spec, worktree_path),
        _ => Ok(()),
    }
}

fn claude_settings_path(worktree_path: &Path) -> PathBuf {
    CLAUDE_SETTINGS_REL
        .iter()
        .fold(worktree_path.to_path_buf(), |path, segment| {
            path.join(segment)
        })
}

fn claude_mcp_config_path(worktree_path: &Path) -> PathBuf {
    CLAUDE_MCP_CONFIG_REL
        .iter()
        .fold(worktree_path.to_path_buf(), |path, segment| {
            path.join(segment)
        })
}

fn codex_events_path(worktree_path: &Path) -> PathBuf {
    CODEX_EVENTS_REL
        .iter()
        .fold(worktree_path.to_path_buf(), |path, segment| {
            path.join(segment)
        })
}

fn claude_events_path(worktree_path: &Path) -> PathBuf {
    CLAUDE_EVENTS_REL
        .iter()
        .fold(worktree_path.to_path_buf(), |path, segment| {
            path.join(segment)
        })
}

fn opencode_runner_path(worktree_path: &Path) -> PathBuf {
    OPENCODE_RUNNER_REL
        .iter()
        .fold(worktree_path.to_path_buf(), |path, segment| {
            path.join(segment)
        })
}

fn opencode_prompt_path(worktree_path: &Path) -> PathBuf {
    OPENCODE_PROMPT_REL
        .iter()
        .fold(worktree_path.to_path_buf(), |path, segment| {
            path.join(segment)
        })
}

fn opencode_config_path(worktree_path: &Path) -> PathBuf {
    OPENCODE_CONFIG_REL
        .iter()
        .fold(worktree_path.to_path_buf(), |path, segment| {
            path.join(segment)
        })
}

fn opencode_events_path(worktree_path: &Path) -> PathBuf {
    OPENCODE_EVENTS_REL
        .iter()
        .fold(worktree_path.to_path_buf(), |path, segment| {
            path.join(segment)
        })
}

fn pr_title_path(worktree_path: &Path) -> PathBuf {
    PR_TITLE_REL
        .iter()
        .fold(worktree_path.to_path_buf(), |path, segment| {
            path.join(segment)
        })
}

fn pr_body_path(worktree_path: &Path) -> PathBuf {
    PR_BODY_REL
        .iter()
        .fold(worktree_path.to_path_buf(), |path, segment| {
            path.join(segment)
        })
}

fn write_claude_settings(worktree_path: &Path) -> Result<(), SessionError> {
    let settings_path = claude_settings_path(worktree_path);
    if let Some(parent) = settings_path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            SessionError::protocol(
                "claude-settings-write-failed",
                format!("failed to create {}: {err}", parent.display()),
                "Verify the Picker worktree is writable before spawning Claude Code.",
                "task-claude-code-adapter-impl",
            )
        })?;
    }

    let mut value = if settings_path.exists() {
        let raw = fs::read_to_string(&settings_path).map_err(|err| {
            SessionError::protocol(
                "claude-settings-read-failed",
                format!("failed to read {}: {err}", settings_path.display()),
                "Fix the project .claude/settings.json permissions or JSON before spawning Claude Code.",
                "task-claude-code-adapter-impl",
            )
        })?;
        serde_json::from_str(&raw).map_err(|err| {
            SessionError::protocol(
                "claude-settings-invalid",
                format!("{} is not valid JSON: {err}", settings_path.display()),
                "Fix .claude/settings.json before spawning Claude Code.",
                "task-claude-code-adapter-impl",
            )
        })?
    } else {
        serde_json::json!({})
    };

    let root = value.as_object_mut().ok_or_else(|| {
        SessionError::protocol(
            "claude-settings-invalid",
            format!("{} must contain a JSON object", settings_path.display()),
            "Fix .claude/settings.json before spawning Claude Code.",
            "task-claude-code-adapter-impl",
        )
    })?;
    let hooks = root.entry("hooks").or_insert_with(|| serde_json::json!({}));
    let hooks = hooks.as_object_mut().ok_or_else(|| {
        SessionError::protocol(
            "claude-settings-invalid",
            format!(
                "{} field hooks must be a JSON object",
                settings_path.display()
            ),
            "Fix .claude/settings.json before spawning Claude Code.",
            "task-claude-code-adapter-impl",
        )
    })?;
    ensure_claude_hook(
        hooks,
        "SessionStart",
        TEMPYR_BOOTSTRAP_COMMAND,
        &settings_path,
    )?;
    ensure_claude_hook(
        hooks,
        "SessionEnd",
        TEMPYR_CLAUDE_FINALIZE_COMMAND,
        &settings_path,
    )?;

    let raw = serde_json::to_vec_pretty(&value).expect("settings JSON always serializes");
    fs::write(&settings_path, raw).map_err(|err| {
        SessionError::protocol(
            "claude-settings-write-failed",
            format!("failed to write {}: {err}", settings_path.display()),
            "Verify the Picker worktree is writable before spawning Claude Code.",
            "task-claude-code-adapter-impl",
        )
    })
}

fn ensure_claude_hook(
    hooks: &mut serde_json::Map<String, serde_json::Value>,
    event: &str,
    command: &str,
    settings_path: &Path,
) -> Result<(), SessionError> {
    let event_value = hooks
        .entry(event.to_owned())
        .or_insert_with(|| serde_json::json!([]));
    let entries = event_value.as_array_mut().ok_or_else(|| {
        SessionError::protocol(
            "claude-settings-invalid",
            format!(
                "{} hooks.{event} must be a JSON array",
                settings_path.display()
            ),
            "Fix .claude/settings.json before spawning Claude Code.",
            "task-claude-code-adapter-impl",
        )
    })?;
    if hook_entries_contain_command(entries, command) {
        return Ok(());
    }
    entries.push(serde_json::json!({
        "hooks": [
            {
                "type": "command",
                "command": command,
            }
        ]
    }));
    Ok(())
}

fn hook_entries_contain_command(entries: &[serde_json::Value], command: &str) -> bool {
    entries.iter().any(|entry| {
        entry
            .get("hooks")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|hooks| {
                hooks.iter().any(|hook| {
                    hook.get("command")
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(|existing| existing == command)
                })
            })
    })
}

#[derive(Debug, Deserialize)]
struct BlueberryProjectFile {
    #[serde(default, rename = "mcp-servers")]
    mcp_servers: HashMap<String, McpServerEntry>,
}

#[derive(Debug, Deserialize)]
struct McpServerEntry {
    url: String,
    enabled: Option<bool>,
    auth: Option<String>,
}

fn read_project_config(config: &SessionConfig) -> Result<BlueberryProjectFile, SessionError> {
    let raw = fs::read_to_string(&config.project_config_path).map_err(|err| {
        SessionError::protocol(
            "project-config-missing",
            format!(
                "failed to read {}: {err}",
                config.project_config_path.display()
            ),
            "Create ~/.jam/config/projects/blueberry.toml with [mcp-servers] before spawning non-Codex Pickers.",
            "task-per-project-mcp-config",
        )
    })?;
    toml::from_str(&raw).map_err(|err| {
        SessionError::protocol(
            "project-config-invalid",
            format!(
                "failed to parse {}: {err}",
                config.project_config_path.display()
            ),
            "Fix the Blueberry project config TOML before spawning non-Codex Pickers.",
            "task-per-project-mcp-config",
        )
    })
}

fn write_claude_mcp_config(
    config: &SessionConfig,
    worktree_path: &Path,
) -> Result<(), SessionError> {
    let project = read_project_config(config)?;
    let mut servers = serde_json::Map::new();
    let mut names: Vec<_> = project.mcp_servers.keys().cloned().collect();
    names.sort_unstable();
    for name in names {
        let server = &project.mcp_servers[&name];
        if !server.enabled.unwrap_or(true) {
            continue;
        }
        if let Some(auth) = &server.auth {
            return Err(SessionError::protocol(
                "mcp-auth-not-implemented",
                format!("enabled MCP server {name} declares auth={auth}"),
                "Disable this MCP server for Claude Pickers until the MCP secret-injection path lands.",
                "task-claude-code-adapter-impl",
            ));
        }
        let server_json = claude_mcp_server_json(&name, server)?;
        servers.insert(name, server_json);
    }

    if servers.is_empty() {
        return Err(SessionError::protocol(
            "mcp-config-empty",
            format!(
                "{} has no enabled unauthenticated MCP servers",
                config.project_config_path.display()
            ),
            "Enable at least the tempyr MCP server for Claude Pickers.",
            "task-claude-code-adapter-impl",
        ));
    }

    let mcp_path = claude_mcp_config_path(worktree_path);
    if let Some(parent) = mcp_path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            SessionError::protocol(
                "mcp-config-write-failed",
                format!("failed to create {}: {err}", parent.display()),
                "Verify the Picker worktree is writable before spawning Claude Code.",
                "task-claude-code-adapter-impl",
            )
        })?;
    }
    let config = serde_json::json!({ "mcpServers": servers });
    let raw = serde_json::to_vec_pretty(&config).expect("MCP JSON always serializes");
    fs::write(&mcp_path, raw).map_err(|err| {
        SessionError::protocol(
            "mcp-config-write-failed",
            format!("failed to write {}: {err}", mcp_path.display()),
            "Verify the Picker worktree is writable before spawning Claude Code.",
            "task-claude-code-adapter-impl",
        )
    })
}

fn claude_mcp_server_json(
    name: &str,
    server: &McpServerEntry,
) -> Result<serde_json::Value, SessionError> {
    if let Some(command) = server.url.strip_prefix("stdio:") {
        let parts = shell_words(command.trim());
        let Some((program, args)) = parts.split_first() else {
            return Err(SessionError::protocol(
                "mcp-config-invalid",
                format!("stdio MCP server {name} has empty command"),
                "Set the MCP server url to a command like stdio:tempyr --mcp.",
                "task-per-project-mcp-config",
            ));
        };
        return Ok(serde_json::json!({
            "command": program,
            "args": args,
        }));
    }
    if server.url.starts_with("https://") || server.url.starts_with("http://") {
        return Ok(serde_json::json!({
            "type": "http",
            "url": server.url,
        }));
    }
    Err(SessionError::protocol(
        "mcp-config-invalid",
        format!("MCP server {name} url is not stdio/http(s): {}", server.url),
        "Use stdio:<command> or an http(s) MCP URL.",
        "task-per-project-mcp-config",
    ))
}

fn prepare_opencode_worktree(
    config: &SessionConfig,
    spec: &SpawnSpec,
    worktree_path: &Path,
) -> Result<(), SessionError> {
    write_opencode_runner(worktree_path)?;
    write_opencode_prompt(worktree_path, &spec.initial_prompt)?;
    write_opencode_config(config, spec, worktree_path)
}

fn write_opencode_runner(worktree_path: &Path) -> Result<(), SessionError> {
    let runner_path = opencode_runner_path(worktree_path);
    if let Some(parent) = runner_path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            SessionError::protocol(
                "opencode-runner-write-failed",
                format!("failed to create {}: {err}", parent.display()),
                "Verify the Picker worktree is writable before spawning OpenCode.",
                "task-opencode-deepseek-adapter-impl",
            )
        })?;
    }
    fs::write(&runner_path, opencode_runner_script()).map_err(|err| {
        SessionError::protocol(
            "opencode-runner-write-failed",
            format!("failed to write {}: {err}", runner_path.display()),
            "Verify the Picker worktree is writable before spawning OpenCode.",
            "task-opencode-deepseek-adapter-impl",
        )
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = fs::metadata(&runner_path)
            .map_err(|err| {
                SessionError::protocol(
                    "opencode-runner-write-failed",
                    format!("failed to stat {}: {err}", runner_path.display()),
                    "Verify the Picker worktree is writable before spawning OpenCode.",
                    "task-opencode-deepseek-adapter-impl",
                )
            })?
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&runner_path, permissions).map_err(|err| {
            SessionError::protocol(
                "opencode-runner-write-failed",
                format!("failed to chmod {}: {err}", runner_path.display()),
                "Verify the Picker worktree is writable before spawning OpenCode.",
                "task-opencode-deepseek-adapter-impl",
            )
        })?;
    }
    Ok(())
}

fn opencode_runner_script() -> &'static str {
    r#"#!/usr/bin/env bash
set -euo pipefail

opencode_bin="${1:?opencode binary path required}"
prompt_path="${2:?prompt path required}"
worktree_path="${3:?worktree path required}"
model="${4:?model required}"
variant="${5:-}"
events_path="$worktree_path/.jam/opencode-events.jsonl"

bootstrap_done=0

cleanup() {
    status=$?
    if [[ "$bootstrap_done" -eq 1 ]]; then
        tempyr journal finalize --agent opencode --quiet || true
    fi
    exit "$status"
}

trap cleanup EXIT
trap 'exit 143' TERM INT HUP

tempyr journal bootstrap --quiet
bootstrap_done=1
tempyr journal log --agent opencode plan "OpenCode Picker session started for ${JAM_TASK_ID:-unknown-task}" --provisional

prompt="$(cat "$prompt_path")"
args=(
    run
    --dir "$worktree_path"
    --format json
    --dangerously-skip-permissions
    --model "$model"
    --title "${JAM_TASK_ID:-opencode-picker}"
)
if [[ -n "$variant" ]]; then
    args+=(--variant "$variant")
fi
args+=("$prompt")

: > "$events_path"
set +e
"$opencode_bin" "${args[@]}" | tee "$events_path"
status=${PIPESTATUS[0]}
set -e
exit "$status"
"#
}

fn write_opencode_prompt(worktree_path: &Path, prompt: &str) -> Result<(), SessionError> {
    let prompt_path = opencode_prompt_path(worktree_path);
    if let Some(parent) = prompt_path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            SessionError::protocol(
                "opencode-prompt-write-failed",
                format!("failed to create {}: {err}", parent.display()),
                "Verify the Picker worktree is writable before spawning OpenCode.",
                "task-opencode-deepseek-adapter-impl",
            )
        })?;
    }
    fs::write(&prompt_path, prompt).map_err(|err| {
        SessionError::protocol(
            "opencode-prompt-write-failed",
            format!("failed to write {}: {err}", prompt_path.display()),
            "Verify the Picker worktree is writable before spawning OpenCode.",
            "task-opencode-deepseek-adapter-impl",
        )
    })
}

fn write_opencode_config(
    config: &SessionConfig,
    spec: &SpawnSpec,
    worktree_path: &Path,
) -> Result<(), SessionError> {
    let project = read_project_config(config)?;
    let mut servers = serde_json::Map::new();
    let mut names: Vec<_> = project.mcp_servers.keys().cloned().collect();
    names.sort_unstable();
    for name in names {
        let server = &project.mcp_servers[&name];
        if !server.enabled.unwrap_or(true) {
            continue;
        }
        if let Some(auth) = &server.auth {
            return Err(SessionError::protocol(
                "mcp-auth-not-implemented",
                format!("enabled MCP server {name} declares auth={auth}"),
                "Disable this MCP server for OpenCode Pickers until the MCP secret-injection path lands.",
                "task-opencode-deepseek-adapter-impl",
            ));
        }
        servers.insert(name.clone(), opencode_mcp_server_json(&name, server)?);
    }
    if servers.is_empty() {
        return Err(SessionError::protocol(
            "mcp-config-empty",
            format!(
                "{} has no enabled unauthenticated MCP servers",
                config.project_config_path.display()
            ),
            "Enable at least the tempyr MCP server for OpenCode Pickers.",
            "task-opencode-deepseek-adapter-impl",
        ));
    }

    let model = opencode_model_for_spec(config, spec)?;
    let small_model = normalize_opencode_model(&config.opencode_small_model);
    validate_model_id("opencode_small_model", &small_model, TOKEN_MAX_LEN)?;
    let mut deepseek_models = serde_json::Map::new();
    deepseek_models.insert(
        "deepseek-v4-pro".into(),
        serde_json::json!({"name": "DeepSeek V4 Pro"}),
    );
    deepseek_models.insert(
        "deepseek-v4-flash".into(),
        serde_json::json!({"name": "DeepSeek V4 Flash"}),
    );
    let config_json = serde_json::json!({
        "$schema": "https://opencode.ai/config.json",
        "model": model,
        "small_model": small_model,
        "share": "disabled",
        "autoupdate": false,
        "enabled_providers": ["deepseek"],
        "provider": {
            "deepseek": {
                "models": deepseek_models,
                "options": {
                    "apiKey": "{env:DEEPSEEK_API_KEY}",
                },
            },
        },
        "mcp": servers,
    });

    let config_path = opencode_config_path(worktree_path);
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            SessionError::protocol(
                "opencode-config-write-failed",
                format!("failed to create {}: {err}", parent.display()),
                "Verify the Picker worktree is writable before spawning OpenCode.",
                "task-opencode-deepseek-adapter-impl",
            )
        })?;
    }
    let raw = serde_json::to_vec_pretty(&config_json).expect("OpenCode JSON always serializes");
    fs::write(&config_path, raw).map_err(|err| {
        SessionError::protocol(
            "opencode-config-write-failed",
            format!("failed to write {}: {err}", config_path.display()),
            "Verify the Picker worktree is writable before spawning OpenCode.",
            "task-opencode-deepseek-adapter-impl",
        )
    })
}

fn opencode_mcp_server_json(
    name: &str,
    server: &McpServerEntry,
) -> Result<serde_json::Value, SessionError> {
    if let Some(command) = server.url.strip_prefix("stdio:") {
        let parts = shell_words(command.trim());
        if parts.is_empty() {
            return Err(SessionError::protocol(
                "mcp-config-invalid",
                format!("stdio MCP server {name} has empty command"),
                "Set the MCP server url to a command like stdio:tempyr --mcp.",
                "task-per-project-mcp-config",
            ));
        }
        return Ok(serde_json::json!({
            "type": "local",
            "command": parts,
            "enabled": true,
        }));
    }
    if server.url.starts_with("https://") || server.url.starts_with("http://") {
        return Ok(serde_json::json!({
            "type": "remote",
            "url": server.url,
            "enabled": true,
        }));
    }
    Err(SessionError::protocol(
        "mcp-config-invalid",
        format!("MCP server {name} url is not stdio/http(s): {}", server.url),
        "Use stdio:<command> or an http(s) MCP URL.",
        "task-per-project-mcp-config",
    ))
}

fn opencode_model_for_spec(
    config: &SessionConfig,
    spec: &SpawnSpec,
) -> Result<String, SessionError> {
    let raw = spec
        .model_override
        .as_deref()
        .unwrap_or(&config.opencode_model);
    let model = normalize_opencode_model(raw);
    validate_model_id("opencode_model", &model, TOKEN_MAX_LEN)?;
    Ok(model)
}

fn normalize_opencode_model(raw: &str) -> String {
    if raw.contains('/') {
        raw.to_owned()
    } else {
        format!("deepseek/{raw}")
    }
}

fn direct_stdout_log_path_for_harness(harness: &str, worktree_path: &Path) -> Option<PathBuf> {
    match harness {
        DEFAULT_HARNESS => Some(codex_events_path(worktree_path)),
        CLAUDE_HARNESS => Some(claude_events_path(worktree_path)),
        _ => None,
    }
}

fn prepare_command(
    mut command: Command,
    worktree_path: &Path,
    env: &PickerEnv,
    stdout_log_path: Option<&Path>,
) -> Result<Command, SessionError> {
    command.current_dir(worktree_path);
    #[cfg(unix)]
    command.process_group(0);
    command.env_clear();
    for (key, value) in env.vars() {
        command.env(key, value);
    }
    command.stdin(Stdio::piped());
    if let Some(path) = stdout_log_path {
        prepare_usage_stdout_log(path)?;
    }
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    Ok(command)
}

fn prepare_usage_stdout_log(path: &Path) -> Result<(), SessionError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            SessionError::protocol(
                "usage-log-write-failed",
                format!("failed to create {}: {err}", parent.display()),
                "Verify the Picker worktree is writable before spawning the harness.",
                "task-quota-tracker-three-shapes",
            )
        })?;
    }
    fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(path)
        .map_err(|err| {
            SessionError::protocol(
                "usage-log-write-failed",
                format!("failed to open {}: {err}", path.display()),
                "Verify the Picker worktree is writable before spawning the harness.",
                "task-quota-tracker-three-shapes",
            )
        })?;
    Ok(())
}

fn picker_output_log_path(root: &Path, session_id: &str) -> PathBuf {
    root.join(format!("{session_id}.jsonl"))
}

fn open_picker_output_log(
    root: &Path,
    session_id: &str,
) -> Result<Arc<Mutex<tokio::fs::File>>, SessionError> {
    fs::create_dir_all(root).map_err(|err| {
        SessionError::protocol(
            "session-log-write-failed",
            format!("failed to create {}: {err}", root.display()),
            "Verify JAM_SESSION_LOG_ROOT is writable by jam-svc-session.",
            "comp-jam-svc-session",
        )
    })?;
    let path = picker_output_log_path(root, session_id);
    let file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&path)
        .map_err(|err| {
            SessionError::protocol(
                "session-log-write-failed",
                format!("failed to open {}: {err}", path.display()),
                "Verify JAM_SESSION_LOG_ROOT is writable by jam-svc-session.",
                "comp-jam-svc-session",
            )
        })?;
    Ok(Arc::new(Mutex::new(tokio::fs::File::from_std(file))))
}

struct PickerEnv {
    values: Vec<(&'static str, OsString)>,
}

impl PickerEnv {
    fn new(
        config: &SessionConfig,
        spec: &SpawnSpec,
        worktree_path: &Path,
        session_id: &str,
        picker_trace: &TraceCtx,
        parent_trace: &TraceCtx,
    ) -> Result<Self, SessionError> {
        let mut values = vec![
            ("HOME", config.picker_home.clone().into_os_string()),
            ("CODEX_HOME", config.codex_home.clone().into_os_string()),
            ("PATH", config.picker_path.clone()),
            ("JAM_TRACE_ID", picker_trace.trace_id.to_string().into()),
            (
                "JAM_PARENT_TRACE_ID",
                parent_trace.trace_id.to_string().into(),
            ),
            ("JAM_TASK_ID", spec.task_id.clone().into()),
            ("JAM_SESSION_ID", session_id.into()),
            ("JAM_TASK_CLASS", spec.task_class.clone().into()),
            ("JAM_SANDBOX_BACKEND", spec.sandbox_backend.clone().into()),
            ("JAM_SANDBOX_PROFILE", spec.sandbox_profile.clone().into()),
        ];
        if let Some(budget) = spec.budget_usd {
            values.push(("JAM_BUDGET_USD", budget.to_string().into()));
        }
        if config.use_systemd_scope && spec.sandbox_backend == DEFAULT_SANDBOX_BACKEND {
            for key in ["DBUS_SESSION_BUS_ADDRESS", "XDG_RUNTIME_DIR"] {
                if let Ok(value) = std::env::var(key) {
                    values.push((key, value.into()));
                }
            }
        }
        if let Ok(token) = std::env::var("JAM_GITHUB_TOKEN") {
            values.push(("GITHUB_TOKEN", token.into()));
        }
        if spec.harness == OPENCODE_HARNESS && !spec.dry_run {
            let api_key = load_deepseek_api_key(config)?;
            values.push((DEEPSEEK_SECRET_ENV, api_key.expose_secret().into()));
            values.push((
                "OPENCODE_CONFIG",
                opencode_config_path(worktree_path).into_os_string(),
            ));
            values.push(("OPENCODE_DISABLE_AUTOUPDATE", "1".into()));
            values.push(("OPENCODE_CLIENT", "jam-svc-session".into()));
        }
        Ok(Self { values })
    }

    fn vars(&self) -> impl Iterator<Item = (&'static str, &OsString)> {
        self.values.iter().map(|(key, value)| (*key, value))
    }

    fn preserve_env(&self) -> String {
        self.values
            .iter()
            .map(|(key, _)| *key)
            .collect::<Vec<_>>()
            .join(",")
    }
}

fn load_deepseek_api_key(config: &SessionConfig) -> Result<SecretString, SessionError> {
    if let Ok(raw) = std::env::var(DEEPSEEK_SECRET_ENV) {
        return secret_from_env(DEEPSEEK_SECRET_ENV, raw);
    }
    if let Ok(raw) = std::env::var("JAM_DEEPSEEK_API_KEY") {
        return secret_from_env("JAM_DEEPSEEK_API_KEY", raw);
    }
    if let Some(path) = &config.secrets_file {
        let backend = FileBackend::new(path);
        return backend
            .get(&SecretKey::new("jam/pickers/deepseek-api-key"))
            .map_err(|err| {
                SessionError::protocol(
                    "opencode-secret-missing",
                    format!("DeepSeek API key not available from {}: {err}", path.display()),
                    "Set DEEPSEEK_API_KEY for local smoke tests or add jam/pickers/deepseek-api-key to the configured secrets file.",
                    "task-opencode-deepseek-adapter-impl",
                )
            });
    }
    let backend = PassBackend::new("jam");
    backend
        .get(&SecretKey::new("pickers/deepseek-api-key"))
        .map_err(|err| {
            SessionError::protocol(
                "opencode-secret-missing",
                format!("DeepSeek API key not available from pass: {err}"),
                "Insert jam/pickers/deepseek-api-key into maestro's pass store or set DEEPSEEK_API_KEY for local smoke tests.",
                "task-opencode-deepseek-adapter-impl",
            )
        })
}

fn secret_from_env(name: &'static str, raw: String) -> Result<SecretString, SessionError> {
    if raw.trim().is_empty() {
        return Err(SessionError::protocol(
            "opencode-secret-empty",
            format!("{name} is set but empty"),
            "Unset the variable or set it to a non-empty DeepSeek API key.",
            "task-opencode-deepseek-adapter-impl",
        ));
    }
    Ok(SecretString::from(raw))
}

async fn publish_picker_spawned(
    nats: &JamNats,
    handle: &PickerHandle,
    picker_trace: &TraceCtx,
    parent_trace: &TraceCtx,
) -> Result<(), SessionError> {
    let payload = PickerSpawned {
        task_id: handle.task_id.clone(),
        harness: handle.harness.clone(),
        session_id: handle.session_id.clone(),
        worktree_path: handle.worktree_path.clone(),
        spawned_at: handle.spawned_at,
        picker_pid: handle.picker_pid,
        picker_trace_id: picker_trace.trace_id,
        maestro_trace_id: parent_trace.trace_id,
        sandbox_backend: handle.sandbox_backend.clone(),
        sandbox_profile: handle.sandbox_profile.clone(),
        task_class: handle.task_class.clone(),
        codex_conversation_id: None,
        parent_session_id: handle.parent_session_id.clone(),
    };
    let envelope = EventEnvelope::new(
        PickerSpawned::EVENT_TYPE,
        PickerSpawned::EVENT_SUBTYPE_VERSION,
        0,
        picker_trace.trace_id.to_string(),
        SERVICE_NAME,
        payload,
    )
    .with_parent_trace(parent_trace.trace_id.to_string());
    nats.publish_traced("journal.picker.spawned", &envelope, picker_trace)
        .await
        .map_err(|err| {
            SessionError::protocol(
                "journal-publish-failed",
                err.to_string(),
                "Verify NATS is running and the journal bridge is healthy.",
                "principle-failure-surfaces-immediately",
            )
        })
}

async fn publish_picker_exited(
    nats: &JamNats,
    handle: &PickerHandle,
    exit_code: u32,
    exited_at: DateTime<Utc>,
    picker_trace: &TraceCtx,
    parent_trace: &TraceCtx,
) -> Result<(), SessionError> {
    let payload = PickerExited {
        session_id: handle.session_id.clone(),
        task_id: handle.task_id.clone(),
        exit_code,
        exited_at,
        duration_ms: picker_duration_ms(handle.spawned_at, exited_at),
    };
    let envelope = EventEnvelope::new(
        PickerExited::EVENT_TYPE,
        PickerExited::EVENT_SUBTYPE_VERSION,
        0,
        picker_trace.trace_id.to_string(),
        SERVICE_NAME,
        payload,
    )
    .with_parent_trace(parent_trace.trace_id.to_string());
    nats.publish_traced("journal.picker.exited", &envelope, picker_trace)
        .await
        .map_err(|err| {
            SessionError::protocol(
                "journal-publish-failed",
                err.to_string(),
                "Verify NATS is running and the journal bridge is healthy.",
                "principle-failure-surfaces-immediately",
            )
        })
}

async fn publish_picker_first_output(
    nats: &JamNats,
    handle: &PickerHandle,
    ts: DateTime<Utc>,
    picker_trace: &TraceCtx,
    parent_trace: &TraceCtx,
) -> Result<(), SessionError> {
    let payload = PickerFirstOutput {
        session_id: handle.session_id.clone(),
        task_id: handle.task_id.clone(),
        ts,
    };
    let envelope = EventEnvelope::new(
        PickerFirstOutput::EVENT_TYPE,
        PickerFirstOutput::EVENT_SUBTYPE_VERSION,
        0,
        picker_trace.trace_id.to_string(),
        SERVICE_NAME,
        payload,
    )
    .with_parent_trace(parent_trace.trace_id.to_string());
    nats.publish_traced("journal.picker.first-output", &envelope, picker_trace)
        .await
        .map_err(|err| {
            SessionError::protocol(
                "journal-publish-failed",
                err.to_string(),
                "Verify NATS is running and the journal bridge is healthy.",
                "principle-failure-surfaces-immediately",
            )
        })
}

async fn publish_quota_usage_observed(
    nats: &JamNats,
    handle: &PickerHandle,
    usage: QuotaUsageObservation,
    observed_at: DateTime<Utc>,
    picker_trace: &TraceCtx,
    parent_trace: &TraceCtx,
) -> Result<(), SessionError> {
    let payload = QuotaUsageObserved {
        harness: handle.harness.clone(),
        window_kind: quota_window_kind_for_harness(&handle.harness).into(),
        session_id: handle.session_id.clone(),
        task_id: handle.task_id.clone(),
        provider: usage.provider,
        model: usage.model,
        input_tokens: (usage.input_tokens > 0).then_some(usage.input_tokens),
        output_tokens: (usage.output_tokens > 0).then_some(usage.output_tokens),
        cost_usd: usage.cost_usd,
        source: usage.source.into(),
        observed_at,
    };
    let envelope = EventEnvelope::new(
        QuotaUsageObserved::EVENT_TYPE,
        QuotaUsageObserved::EVENT_SUBTYPE_VERSION,
        0,
        picker_trace.trace_id.to_string(),
        SERVICE_NAME,
        payload,
    )
    .with_parent_trace(parent_trace.trace_id.to_string());
    nats.publish_traced("journal.quota.usage-observed", &envelope, picker_trace)
        .await
        .map_err(|err| {
            SessionError::protocol(
                "journal-publish-failed",
                err.to_string(),
                "Verify NATS is running and the journal bridge is healthy.",
                "principle-failure-surfaces-immediately",
            )
        })
}

fn quota_usage_for_handle(
    handle: &PickerHandle,
    _observed_at: DateTime<Utc>,
) -> Option<QuotaUsageObservation> {
    if handle.dry_run {
        return None;
    }
    let worktree_path = Path::new(&handle.worktree_path);
    let (events_path, source) = match handle.harness.as_str() {
        DEFAULT_HARNESS => (codex_events_path(worktree_path), "codex-json"),
        CLAUDE_HARNESS => (claude_events_path(worktree_path), "claude-stream-json"),
        OPENCODE_HARNESS => (opencode_events_path(worktree_path), "opencode-json"),
        _ => return None,
    };
    let raw = fs::read_to_string(events_path).ok()?;
    parse_usage_jsonl(&raw, source)
}

fn quota_window_kind_for_harness(harness: &str) -> &'static str {
    match harness {
        OPENCODE_HARNESS => "api-budget",
        CLAUDE_HARNESS => "rate-limit",
        _ => "local-messages",
    }
}

const RATE_LIMIT_PATTERNS: &[&str] = &[
    "rate limit",
    "rate_limit",
    "rate-limit",
    "ratelimit",
    "too many requests",
    "too many messages",
    "usage limit",
    "you've reached your limit",
    "you have reached your limit",
    "quota exceeded",
    "request limit reached",
    "limit reached",
];

#[cfg(test)]
fn detect_quota_exhaustion_in_log(log_path: &Path, harness: &str) -> bool {
    let Ok(raw) = fs::read_to_string(log_path) else {
        return false;
    };
    for line in raw.lines() {
        let Ok(record) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if record.get("stream").and_then(|v| v.as_str()) != Some("stderr") {
            continue;
        }
        let Some(text) = record.get("line").and_then(|v| v.as_str()) else {
            continue;
        };
        let lower = text.to_ascii_lowercase();
        if RATE_LIMIT_PATTERNS
            .iter()
            .any(|pattern| lower.contains(pattern))
        {
            info!(
                harness,
                line = text,
                "detected rate-limit signal in Picker stderr"
            );
            return true;
        }
    }
    // Also check Codex-specific: exit code alone with very short runtime can
    // indicate quota hit (Codex exits immediately when throttled).
    false
}

async fn publish_quota_exhausted(
    nats: &JamNats,
    handle: &PickerHandle,
    detected_at: DateTime<Utc>,
    picker_trace: &TraceCtx,
    parent_trace: &TraceCtx,
) -> Result<(), SessionError> {
    let payload = QuotaExhausted {
        harness: handle.harness.clone(),
        window_kind: quota_window_kind_for_harness(&handle.harness).into(),
        resets_at: None,
        detected_at,
    };
    let envelope = EventEnvelope::new(
        QuotaExhausted::EVENT_TYPE,
        QuotaExhausted::EVENT_SUBTYPE_VERSION,
        0,
        picker_trace.trace_id.to_string(),
        SERVICE_NAME,
        payload,
    )
    .with_parent_trace(parent_trace.trace_id.to_string());
    nats.publish_traced("journal.quota.exhausted", &envelope, picker_trace)
        .await
        .map_err(|err| {
            SessionError::protocol(
                "journal-publish-failed",
                err.to_string(),
                "Verify NATS is running and the journal bridge is healthy.",
                "principle-failure-surfaces-immediately",
            )
        })
}

fn parse_usage_jsonl(raw: &str, source: &'static str) -> Option<QuotaUsageObservation> {
    let mut preferred = QuotaUsageObservation {
        source,
        ..QuotaUsageObservation::default()
    };
    let mut fallback = QuotaUsageObservation {
        source,
        ..QuotaUsageObservation::default()
    };
    for line in raw.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if let Some(observation) = usage_from_json_value(&value, source) {
            fallback.merge(observation.clone());
            if usage_observation_is_preferred(source, &value) {
                preferred.merge(observation);
            }
        }
    }
    preferred
        .has_usage()
        .then_some(preferred)
        .or_else(|| fallback.has_usage().then_some(fallback))
}

fn usage_observation_is_preferred(source: &str, value: &serde_json::Value) -> bool {
    source != "claude-stream-json"
        || value.get("type").and_then(serde_json::Value::as_str) == Some("result")
}

fn usage_from_json_value(
    value: &serde_json::Value,
    source: &'static str,
) -> Option<QuotaUsageObservation> {
    let input_tokens = first_u64_path(
        value,
        &[
            &["usage", "input_tokens"],
            &["usage", "input"],
            &["usage", "prompt_tokens"],
            &["tokens", "input"],
            &["input_tokens"],
            &["prompt_tokens"],
            &["usage", "inputTokens"],
        ],
    )
    .or_else(|| first_model_usage_u64(value, "inputTokens"))
    .unwrap_or(0);
    let output_tokens = first_u64_path(
        value,
        &[
            &["usage", "output_tokens"],
            &["usage", "output"],
            &["usage", "completion_tokens"],
            &["tokens", "output"],
            &["output_tokens"],
            &["completion_tokens"],
            &["usage", "outputTokens"],
        ],
    )
    .or_else(|| first_model_usage_u64(value, "outputTokens"))
    .unwrap_or(0);
    let cost_usd = first_f64_path(
        value,
        &[
            &["usage", "cost_usd"],
            &["usage", "cost"],
            &["usage", "costUSD"],
            &["cost", "usd"],
            &["cost_usd"],
            &["total_cost_usd"],
            &["totalCostUSD"],
        ],
    )
    .or_else(|| first_model_usage_f64(value, "costUSD"))
    .filter(|value| value.is_finite() && *value >= 0.0);
    if input_tokens == 0 && output_tokens == 0 && cost_usd.is_none() {
        return None;
    }
    Some(QuotaUsageObservation {
        provider: first_string_path(value, &[&["provider"], &["usage", "provider"]]),
        model: first_string_path(
            value,
            &[&["model"], &["usage", "model"], &["message", "model"]],
        )
        .or_else(|| first_model_usage_key(value)),
        input_tokens,
        output_tokens,
        cost_usd,
        source,
    })
}

fn first_string_path(value: &serde_json::Value, paths: &[&[&str]]) -> Option<String> {
    paths.iter().find_map(|path| {
        json_path(value, path)?
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    })
}

fn first_u64_path(value: &serde_json::Value, paths: &[&[&str]]) -> Option<u64> {
    paths
        .iter()
        .find_map(|path| json_path(value, path)?.as_u64())
}

fn first_f64_path(value: &serde_json::Value, paths: &[&[&str]]) -> Option<f64> {
    paths
        .iter()
        .find_map(|path| json_path(value, path)?.as_f64())
}

fn first_model_usage_key(value: &serde_json::Value) -> Option<String> {
    value
        .get("modelUsage")?
        .as_object()?
        .keys()
        .next()
        .map(ToOwned::to_owned)
}

fn first_model_usage_u64(value: &serde_json::Value, key: &str) -> Option<u64> {
    first_model_usage_value(value, key)?.as_u64()
}

fn first_model_usage_f64(value: &serde_json::Value, key: &str) -> Option<f64> {
    first_model_usage_value(value, key)?.as_f64()
}

fn first_model_usage_value<'a>(
    value: &'a serde_json::Value,
    key: &str,
) -> Option<&'a serde_json::Value> {
    value
        .get("modelUsage")?
        .as_object()?
        .values()
        .find_map(|entry| entry.get(key))
}

fn json_path<'a>(value: &'a serde_json::Value, path: &[&str]) -> Option<&'a serde_json::Value> {
    let mut current = value;
    for segment in path {
        current = current.get(*segment)?;
    }
    Some(current)
}

fn watch_picker_output(
    nats: JamNats,
    handle: PickerHandle,
    picker_trace: TraceCtx,
    parent_trace: TraceCtx,
    stdout: Option<ChildStdout>,
    stderr: Option<ChildStderr>,
    output_log: Arc<Mutex<tokio::fs::File>>,
) -> Arc<AtomicBool> {
    let sequence = Arc::new(AtomicU64::new(0));
    let first_output = Arc::new(AtomicBool::new(false));
    let quota_exhausted_flag = Arc::new(AtomicBool::new(false));
    let usage_stdout_path =
        direct_stdout_log_path_for_harness(&handle.harness, Path::new(&handle.worktree_path))
            .filter(|_| !handle.dry_run);

    if let Some(stdout) = stdout {
        spawn_picker_output_reader(
            stdout,
            "stdout",
            usage_stdout_path,
            nats.clone(),
            handle.clone(),
            picker_trace.clone(),
            parent_trace.clone(),
            output_log.clone(),
            sequence.clone(),
            first_output.clone(),
            quota_exhausted_flag.clone(),
        );
    }
    if let Some(stderr) = stderr {
        spawn_picker_output_reader(
            stderr,
            "stderr",
            None,
            nats,
            handle,
            picker_trace,
            parent_trace,
            output_log,
            sequence,
            first_output,
            quota_exhausted_flag.clone(),
        );
    }
    quota_exhausted_flag
}

#[allow(clippy::too_many_arguments)]
fn spawn_picker_output_reader<R>(
    reader: R,
    stream: &'static str,
    usage_stdout_path: Option<PathBuf>,
    nats: JamNats,
    handle: PickerHandle,
    picker_trace: TraceCtx,
    parent_trace: TraceCtx,
    output_log: Arc<Mutex<tokio::fs::File>>,
    sequence: Arc<AtomicU64>,
    first_output: Arc<AtomicBool>,
    quota_exhausted_flag: Arc<AtomicBool>,
) where
    R: AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut usage_log = match usage_stdout_path {
            Some(path) => match tokio::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .await
            {
                Ok(file) => Some(file),
                Err(err) => {
                    warn!(
                        session_id = %handle.session_id,
                        path = %path.display(),
                        "failed to open Picker usage stdout log: {err}"
                    );
                    None
                }
            },
            None => None,
        };
        let mut lines = BufReader::new(reader).lines();
        loop {
            let line = match lines.next_line().await {
                Ok(Some(line)) => line,
                Ok(None) => break,
                Err(err) => {
                    warn!(
                        session_id = %handle.session_id,
                        stream,
                        "failed to read Picker output: {err}"
                    );
                    break;
                }
            };

            if let Some(file) = usage_log.as_mut() {
                if let Err(err) = file.write_all(line.as_bytes()).await {
                    warn!(
                        session_id = %handle.session_id,
                        stream,
                        "failed to append Picker usage stdout line: {err}"
                    );
                    usage_log = None;
                } else if let Err(err) = file.write_all(b"\n").await {
                    warn!(
                        session_id = %handle.session_id,
                        stream,
                        "failed to append Picker usage stdout newline: {err}"
                    );
                    usage_log = None;
                }
            }

            if line.trim().is_empty() {
                continue;
            }

            let ts = Utc::now();
            if !first_output.swap(true, Ordering::SeqCst) {
                if let Err(err) =
                    publish_picker_first_output(&nats, &handle, ts, &picker_trace, &parent_trace)
                        .await
                {
                    warn!(
                        session_id = %handle.session_id,
                        "failed to publish Picker first-output event: {err}"
                    );
                }
            }

            let (line, truncated) = truncate_picker_output_line(&line);
            let record = PickerOutputRecord {
                session_id: handle.session_id.clone(),
                task_id: handle.task_id.clone(),
                trace_id: picker_trace.trace_id.to_string(),
                stream,
                line,
                ts,
                sequence: sequence.fetch_add(1, Ordering::SeqCst),
                truncated,
            };
            if stream == "stderr" && !quota_exhausted_flag.load(Ordering::Relaxed) {
                let lower = record.line.to_ascii_lowercase();
                if RATE_LIMIT_PATTERNS
                    .iter()
                    .any(|pattern| lower.contains(pattern))
                {
                    info!(
                        session_id = %handle.session_id,
                        line = %record.line,
                        "detected rate-limit signal in Picker stderr"
                    );
                    quota_exhausted_flag.store(true, Ordering::SeqCst);
                }
            }
            append_picker_output_log(&output_log, &record).await;
            publish_picker_output(&nats, &record, &picker_trace).await;
        }
    });
}

fn truncate_picker_output_line(line: &str) -> (String, bool) {
    let line = line.replace('\0', "");
    let mut chars = line.chars();
    let rendered: String = chars.by_ref().take(PICKER_OUTPUT_LINE_MAX_CHARS).collect();
    (rendered, chars.next().is_some())
}

async fn append_picker_output_log(
    output_log: &Arc<Mutex<tokio::fs::File>>,
    record: &PickerOutputRecord,
) {
    let rendered = match serde_json::to_vec(record) {
        Ok(rendered) => rendered,
        Err(err) => {
            warn!(
                session_id = %record.session_id,
                "failed to serialize Picker output record: {err}"
            );
            return;
        }
    };
    let mut file = output_log.lock().await;
    if let Err(err) = file.write_all(&rendered).await {
        warn!(
            session_id = %record.session_id,
            "failed to write Picker output log line: {err}"
        );
        return;
    }
    if let Err(err) = file.write_all(b"\n").await {
        warn!(
            session_id = %record.session_id,
            "failed to write Picker output log newline: {err}"
        );
        return;
    }
    if let Err(err) = file.flush().await {
        warn!(
            session_id = %record.session_id,
            "failed to flush Picker output log: {err}"
        );
    }
}

async fn publish_picker_output(nats: &JamNats, record: &PickerOutputRecord, ctx: &TraceCtx) {
    if let Err(err) = nats
        .publish_traced(picker_output_subject(&record.session_id), record, ctx)
        .await
    {
        warn!(
            session_id = %record.session_id,
            "failed to publish Picker output: {err}"
        );
    }
}

fn picker_output_subject(session_id: &str) -> String {
    format!("picker.{session_id}.output")
}

fn watch_picker_exit(
    mut child: Child,
    active: Arc<Mutex<HashMap<String, PickerRecord>>>,
    nats: JamNats,
    config: SessionConfig,
    routing: jam_nats::RoutingResolver,
    handle: PickerHandle,
    picker_trace: TraceCtx,
    parent_trace: TraceCtx,
    quota_exhausted_flag: Arc<AtomicBool>,
) {
    let session_id = handle.session_id.clone();
    tokio::spawn(async move {
        match child.wait().await {
            Ok(status) => {
                let exited_at = Utc::now();
                let exit_code = status.code();
                let mut publish_exited = false;
                let mut active = active.lock().await;
                if let Some(record) = active.get_mut(&session_id) {
                    publish_exited = record.status == PickerStatus::Running;
                    record.status = if publish_exited {
                        PickerStatus::Exited
                    } else {
                        PickerStatus::Killed
                    };
                    record.exited_at = Some(exited_at);
                    record.exit_code = exit_code;
                }
                drop(active);

                if publish_exited {
                    if let Some(usage) = quota_usage_for_handle(&handle, exited_at) {
                        if let Err(err) = publish_quota_usage_observed(
                            &nats,
                            &handle,
                            usage,
                            exited_at,
                            &picker_trace,
                            &parent_trace,
                        )
                        .await
                        {
                            warn!(
                                session_id = %handle.session_id,
                                "failed to publish quota usage observation: {err}"
                            );
                        }
                    }
                    publish_exit_for_status(
                        &nats,
                        &handle,
                        exit_code,
                        exited_at,
                        &picker_trace,
                        &parent_trace,
                    )
                    .await;
                    if exit_code.is_some_and(|c| c != 0)
                        && quota_exhausted_flag.load(Ordering::SeqCst)
                    {
                        if let Err(err) = publish_quota_exhausted(
                            &nats,
                            &handle,
                            exited_at,
                            &picker_trace,
                            &parent_trace,
                        )
                        .await
                        {
                            warn!(
                                session_id = %handle.session_id,
                                "failed to publish quota.exhausted: {err}"
                            );
                        }
                    }
                    if exit_code == Some(0) {
                        if let Err(err) = maybe_open_pr_for_successful_picker(
                            &nats,
                            &config,
                            &routing,
                            &handle,
                            &picker_trace,
                        )
                        .await
                        {
                            warn!(
                                session_id = %handle.session_id,
                                task_id = %handle.task_id,
                                "post-Picker PR handoff failed: {err}"
                            );
                        }
                    }
                }
            }
            Err(err) => {
                warn!(session_id = %session_id, "failed waiting for Picker process: {err}");
            }
        }
    });
}

async fn maybe_open_pr_for_successful_picker(
    nats: &JamNats,
    config: &SessionConfig,
    routing: &jam_nats::RoutingResolver,
    handle: &PickerHandle,
    ctx: &TraceCtx,
) -> Result<(), SessionError> {
    if handle.dry_run || !config.open_pr_on_success {
        return Ok(());
    }
    let worktree = validate_worktree_path(&handle.worktree_path)?;
    if !ensure_picker_worktree_commit(config, &worktree, &handle.task_id).await? {
        info!(
            task_id = %handle.task_id,
            worktree = %handle.worktree_path,
            "Picker exited 0 with no code changes; skipping PR handoff"
        );
        return Ok(());
    }
    let pr_metadata = read_picker_pr_metadata(&worktree, &handle.task_id)?;

    let branch = branch_for_task(&handle.task_id);
    let request = RepoOpenPrInput {
        task_id: handle.task_id.clone(),
        branch: branch.clone(),
        title: pr_metadata.title,
        body: pr_metadata.body,
        repo: repo_for_project(config, &handle.project),
        draft: config.pr_draft,
        base: base_for_project(config, &handle.project),
        worktree_path: handle.worktree_path.clone(),
        push: true,
    };
    // Route through the manifest so patch-agent-applied versions of jam-svc-repo
    // are reached at their current subject_prefix, not the unversioned default.
    let subject = routing.subject_for("repo", "open-pr").await;
    let value: serde_json::Value = nats
        .request_traced(subject, &request, ctx, config.request_timeout)
        .await
        .map_err(|err| {
            SessionError::protocol(
                "repo-open-pr-request-failed",
                err.to_string(),
                "Verify jam-svc-repo is running and reachable at its routing-manifest subject_prefix.",
                "api-open-pr",
            )
        })?;
    if let Some(error) = value.get("error") {
        return Err(SessionError::protocol(
            "repo-open-pr-failed",
            error.to_string(),
            "Inspect jam-svc-repo logs and retry the task or open the PR manually.",
            "api-open-pr",
        ));
    }
    info!(
        task_id = %handle.task_id,
        branch = %branch,
        "opened PR for successful Picker worktree"
    );
    Ok(())
}

fn repo_for_project(config: &SessionConfig, project: &str) -> Option<String> {
    if project == JAMBOREE_PROJECT {
        Some(config.jamboree_github_repo.clone())
    } else {
        None
    }
}

fn base_for_project(config: &SessionConfig, project: &str) -> String {
    if project == JAMBOREE_PROJECT {
        config.jamboree_trunk_branch.clone()
    } else {
        config.trunk_branch.clone()
    }
}

fn read_picker_pr_metadata(
    worktree: &Path,
    task_id: &str,
) -> Result<PickerPrMetadata, SessionError> {
    let title = read_pr_title(&pr_title_path(worktree), task_id)?;
    let body = read_pr_body(&pr_body_path(worktree), task_id)?;
    Ok(PickerPrMetadata { title, body })
}

fn read_pr_title(path: &Path, task_id: &str) -> Result<String, SessionError> {
    let raw = fs::read_to_string(path).map_err(|err| {
        SessionError::protocol(
            "pr-metadata-missing",
            format!("failed to read required PR title {}: {err}", path.display()),
            "Have the Picker write .jam/pr-title.txt before it exits successfully.",
            "api-open-pr",
        )
    })?;
    let Some(title) = raw.lines().map(str::trim).find(|line| !line.is_empty()) else {
        return Err(SessionError::protocol(
            "pr-metadata-invalid",
            format!("{} did not contain a non-empty title", path.display()),
            "Have the Picker write a concise one-line title focused on the actual code change.",
            "api-open-pr",
        ));
    };
    format_jam_pr_title(title, task_id)
}

fn read_pr_body(path: &Path, task_id: &str) -> Result<String, SessionError> {
    let raw = fs::read_to_string(path).map_err(|err| {
        SessionError::protocol(
            "pr-metadata-missing",
            format!("failed to read required PR body {}: {err}", path.display()),
            "Have the Picker write .jam/pr-body.md before it exits successfully.",
            "api-open-pr",
        )
    })?;
    let body = raw.trim();
    if body.is_empty()
        || body.len() > PR_BODY_MAX_LEN
        || body.contains('\0')
        || body.contains("Automated Jamboree PR")
        || body == task_id
    {
        return Err(SessionError::protocol(
            "pr-metadata-invalid",
            format!(
                "{} must be non-empty Markdown under {PR_BODY_MAX_LEN} bytes and focused on the PR contents",
                path.display()
            ),
            "Have the Picker write a reviewer-facing Summary and Verification section instead of raw logs or IDs.",
            "api-open-pr",
        ));
    }
    Ok(body.to_owned())
}

fn format_jam_pr_title(raw: &str, task_id: &str) -> Result<String, SessionError> {
    let title = normalize_pr_title_text(raw);
    let title = title
        .strip_prefix("[jam]")
        .or_else(|| title.strip_prefix("[JAM]"))
        .map(str::trim)
        .unwrap_or(title.as_str());
    if title.is_empty()
        || title.len() + "[jam] ".len() > PR_TITLE_MAX_LEN
        || title == task_id
        || title.starts_with("task:")
        || title.contains("codex-cli:")
        || title.contains("claude-code:")
        || title.contains("opencode-deepseek:")
    {
        return Err(SessionError::protocol(
            "pr-metadata-invalid",
            "PR title must describe the actual change, not a task id, session id, or log line",
            "Have the Picker rewrite .jam/pr-title.txt as a concise reviewer-facing title.",
            "api-open-pr",
        ));
    }
    Ok(format!("[jam] {title}"))
}

fn normalize_pr_title_text(raw: &str) -> String {
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

async fn ensure_picker_worktree_commit(
    config: &SessionConfig,
    worktree: &Path,
    task_id: &str,
) -> Result<bool, SessionError> {
    if has_commits_since_trunk(config, worktree).await? {
        return Ok(true);
    }
    if !has_uncommitted_code_changes(config, worktree).await? {
        return Ok(false);
    }
    run_picker_git_checked(
        config,
        worktree,
        &["add", "-A", "--", ".", ":(exclude).jam"],
        "git-add-picker-changes",
    )
    .await?;
    let diff = run_picker_git(
        config,
        worktree,
        &["diff", "--cached", "--quiet", "--", ".", ":(exclude).jam"],
    )
    .await?;
    if diff.status.success() {
        return Ok(false);
    }
    if diff.status.code() != Some(1) {
        return Err(command_error(
            "git-diff-cached-failed",
            "git-diff-cached",
            &diff,
            "Verify the Picker worktree is a healthy git checkout.",
        ));
    }
    let message = format!("task: {task_id}");
    run_picker_git_checked(
        config,
        worktree,
        &[
            "-c",
            "user.name=Jamboree Picker",
            "-c",
            "user.email=picker@jamboree.local",
            "commit",
            "-m",
            &message,
        ],
        "git-commit-picker-changes",
    )
    .await?;
    Ok(true)
}

async fn has_uncommitted_code_changes(
    config: &SessionConfig,
    worktree: &Path,
) -> Result<bool, SessionError> {
    let output = run_picker_git(
        config,
        worktree,
        &["status", "--porcelain", "--", ".", ":(exclude).jam"],
    )
    .await?;
    if !output.status.success() {
        return Err(command_error(
            "git-status-failed",
            "git-status",
            &output,
            "Verify the Picker worktree is a healthy git checkout.",
        ));
    }
    Ok(!String::from_utf8_lossy(&output.stdout).trim().is_empty())
}

async fn has_commits_since_trunk(
    config: &SessionConfig,
    worktree: &Path,
) -> Result<bool, SessionError> {
    let range = format!("origin/{}..HEAD", config.trunk_branch);
    let output = run_picker_git(config, worktree, &["rev-list", "--count", &range]).await?;
    if !output.status.success() {
        return Err(command_error(
            "git-rev-list-failed",
            "git-rev-list",
            &output,
            "Verify the Picker worktree has origin/trunk refs.",
        ));
    }
    let count = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<u64>()
        .unwrap_or(0);
    Ok(count > 0)
}

async fn run_picker_git_checked(
    config: &SessionConfig,
    worktree: &Path,
    args: &[&str],
    kind: &'static str,
) -> Result<Output, SessionError> {
    let output = run_picker_git(config, worktree, args).await?;
    if output.status.success() {
        return Ok(output);
    }
    Err(command_error(
        kind,
        kind,
        &output,
        "Verify the Picker worktree is writable and git is configured.",
    ))
}

async fn run_picker_git(
    config: &SessionConfig,
    worktree: &Path,
    args: &[&str],
) -> Result<Output, SessionError> {
    let mut command = if config.use_sudo {
        let mut command = Command::new(&config.sudo_bin);
        command.arg("-n");
        command.arg("-u");
        command.arg("picker");
        command.arg("-H");
        command.arg("--");
        command.arg(&config.git_bin);
        command
    } else {
        Command::new(&config.git_bin)
    };
    command.arg("-C");
    command.arg(worktree);
    command.args(args);
    command.env("GIT_TERMINAL_PROMPT", "0");
    command.output().await.map_err(|err| {
        SessionError::protocol(
            "git-command-failed",
            format!("failed to run git in {}: {err}", worktree.display()),
            "Verify git is installed and the Picker worktree still exists.",
            "api-open-pr",
        )
    })
}

fn command_error(
    kind: &'static str,
    command_name: &str,
    output: &Output,
    remediation: &'static str,
) -> SessionError {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    SessionError::protocol(
        kind,
        format!(
            "{command_name} exited with status {}: {}{}",
            output.status,
            stderr.trim(),
            if stderr.trim().is_empty() {
                stdout.trim()
            } else {
                ""
            }
        ),
        remediation,
        "api-open-pr",
    )
}

fn branch_for_task(task_id: &str) -> String {
    format!("task/{task_id}")
}

async fn watch_picker_messages(
    nats: &JamNats,
    active: &Arc<Mutex<HashMap<String, PickerRecord>>>,
    session_id: &str,
    stdin: &Arc<Mutex<ChildStdin>>,
) {
    for mode in [PickerMessageMode::Queue, PickerMessageMode::Interrupt] {
        let subject = picker_message_subject(session_id, mode);
        let mut subscription = match nats.client().subscribe(subject.clone()).await {
            Ok(subscription) => subscription,
            Err(err) => {
                warn!(subject = %subject, "failed to subscribe for Picker messages: {err}");
                continue;
            }
        };
        info!(subject = %subject, "subscribed for Picker messages");

        let nats = nats.clone();
        let active = Arc::clone(active);
        let session_id = session_id.to_owned();
        let stdin = Arc::clone(stdin);
        tokio::spawn(async move {
            while let Some(message) = subscription.next().await {
                handle_picker_message_command(
                    &nats,
                    active.clone(),
                    &session_id,
                    mode,
                    message,
                    stdin.clone(),
                )
                .await;
            }
            warn!(subject = %subject, "Picker message subscription closed");
        });
    }
}

async fn handle_picker_message_command(
    nats: &JamNats,
    active: Arc<Mutex<HashMap<String, PickerRecord>>>,
    session_id: &str,
    expected_mode: PickerMessageMode,
    message: async_nats::Message,
    stdin: Arc<Mutex<ChildStdin>>,
) {
    let Some(ctx) = message
        .headers
        .as_ref()
        .and_then(jam_nats::extract_trace_from_headers)
    else {
        warn!(
            subject = %message.subject,
            "refusing untraced Picker message command"
        );
        return;
    };

    let payload = serde_json::from_slice::<PickerMessagePayload>(&message.payload);
    let payload = match payload {
        Ok(payload) => payload,
        Err(err) => {
            warn!(
                subject = %message.subject,
                "Picker message command payload is invalid JSON: {err}"
            );
            return;
        }
    };

    if let Err(detail) = validate_picker_message_payload(&payload, session_id, expected_mode) {
        publish_picker_message_delivery_failed(nats, &payload, detail, &ctx).await;
        return;
    }

    if !picker_session_running(&active, session_id).await {
        publish_picker_message_delivery_failed(
            nats,
            &payload,
            "session is not running in jam-svc-session",
            &ctx,
        )
        .await;
        return;
    }

    if expected_mode == PickerMessageMode::Interrupt {
        let detail = serde_json::json!({});
        publish_picker_message_status(
            nats,
            PickerMessageStatusUpdate {
                session_id,
                mode: expected_mode,
                message_id: &payload.message_id,
                status: "interrupt-accepted",
                from: &payload.from,
                detail: &detail,
            },
            &ctx,
        )
        .await;
    }

    match write_picker_message_to_stdin(stdin, &payload).await {
        Ok(()) => {
            let detail = serde_json::json!({});
            publish_picker_message_status(
                nats,
                PickerMessageStatusUpdate {
                    session_id,
                    mode: expected_mode,
                    message_id: &payload.message_id,
                    status: "delivered",
                    from: &payload.from,
                    detail: &detail,
                },
                &ctx,
            )
            .await;
        }
        Err(err) => {
            publish_picker_message_delivery_failed(
                nats,
                &payload,
                format!("failed to write Picker stdin: {err}"),
                &ctx,
            )
            .await;
        }
    }
}

fn validate_picker_message_payload(
    payload: &PickerMessagePayload,
    session_id: &str,
    expected_mode: PickerMessageMode,
) -> Result<(), String> {
    if payload.session_id != session_id {
        return Err(format!(
            "message was for session {}, expected {session_id}",
            payload.session_id
        ));
    }
    if payload.mode != expected_mode {
        return Err(format!(
            "message mode was {:?}, expected {:?}",
            payload.mode, expected_mode
        ));
    }
    if !safe_message_token(&payload.message_id) {
        return Err("message_id is empty, too long, or contains unsafe characters".into());
    }
    if !safe_message_token(&payload.from) {
        return Err("from is empty, too long, or contains unsafe characters".into());
    }
    let Some(text) = payload.text.as_deref() else {
        return Err("queue/interrupt message text is missing".into());
    };
    if text.trim().is_empty() || text.len() > MESSAGE_TEXT_MAX_LEN {
        return Err(format!(
            "message text must be 1-{MESSAGE_TEXT_MAX_LEN} characters"
        ));
    }
    Ok(())
}

fn safe_message_token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= TOKEN_MAX_LEN
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | ':'))
}

async fn picker_session_running(
    active: &Arc<Mutex<HashMap<String, PickerRecord>>>,
    session_id: &str,
) -> bool {
    active
        .lock()
        .await
        .get(session_id)
        .is_some_and(|record| record.status == PickerStatus::Running)
}

async fn write_picker_message_to_stdin(
    stdin: Arc<Mutex<ChildStdin>>,
    payload: &PickerMessagePayload,
) -> std::io::Result<()> {
    let frame = picker_message_stdin_frame(payload);
    let mut stdin = stdin.lock().await;
    stdin.write_all(frame.as_bytes()).await?;
    stdin.flush().await
}

fn picker_message_stdin_frame(payload: &PickerMessagePayload) -> String {
    format!(
        "\n\n<jamboree-message mode=\"{}\" from=\"{}\" id=\"{}\">\n{}\n</jamboree-message>\n",
        payload.mode.subject_token(),
        payload.from,
        payload.message_id,
        payload.text.as_deref().unwrap_or_default()
    )
}

async fn publish_picker_message_delivery_failed(
    nats: &JamNats,
    payload: &PickerMessagePayload,
    detail: impl Into<String>,
    ctx: &TraceCtx,
) {
    let detail = serde_json::json!({ "error": detail.into() });
    publish_picker_message_status(
        nats,
        PickerMessageStatusUpdate {
            session_id: &payload.session_id,
            mode: payload.mode,
            message_id: &payload.message_id,
            status: "delivery-failed",
            from: &payload.from,
            detail: &detail,
        },
        ctx,
    )
    .await;
}

async fn publish_picker_message_status(
    nats: &JamNats,
    update: PickerMessageStatusUpdate<'_>,
    ctx: &TraceCtx,
) {
    let payload = PickerMessageStatusPayload {
        message_id: update.message_id,
        session_id: update.session_id,
        mode: update.mode,
        status: update.status,
        from: update.from,
        detail: update.detail,
        updated_at: Utc::now(),
    };
    if let Err(err) = nats
        .publish_traced(picker_status_subject(update.session_id), &payload, ctx)
        .await
    {
        warn!(
            session_id = %update.session_id,
            message_id = %update.message_id,
            status = %update.status,
            "failed to publish Picker message status: {err}"
        );
    }
}

fn picker_message_subject(session_id: &str, mode: PickerMessageMode) -> String {
    format!("picker.{session_id}.msg.{}", mode.subject_token())
}

fn picker_status_subject(session_id: &str) -> String {
    format!("picker.{session_id}.msg.status")
}

async fn publish_exit_for_status(
    nats: &JamNats,
    handle: &PickerHandle,
    exit_code: Option<i32>,
    exited_at: DateTime<Utc>,
    picker_trace: &TraceCtx,
    parent_trace: &TraceCtx,
) {
    let session_id = handle.session_id.as_str();
    match exit_code.and_then(|code| u32::try_from(code).ok()) {
        Some(exit_code) => {
            if let Err(err) = publish_picker_exited(
                nats,
                handle,
                exit_code,
                exited_at,
                picker_trace,
                parent_trace,
            )
            .await
            {
                warn!(session_id, "failed publishing Picker exit event: {err}");
            }
        }
        None => {
            warn!(
                session_id,
                "Picker process ended without a normal exit code"
            );
        }
    }
}

fn picker_duration_ms(spawned_at: DateTime<Utc>, exited_at: DateTime<Utc>) -> u64 {
    let millis = exited_at
        .signed_duration_since(spawned_at)
        .num_milliseconds()
        .max(0);
    u64::try_from(millis).unwrap_or(0)
}

#[derive(Debug, Deserialize)]
struct InspectPickerInput {
    session_id: String,
}

async fn inspect_picker(
    payload: &[u8],
    state: &SessionState,
) -> Result<PickerRecord, SessionError> {
    let input: InspectPickerInput = serde_json::from_slice(payload).map_err(|err| {
        SessionError::protocol(
            "invalid-input",
            format!("tool.session.inspect-picker payload is invalid JSON: {err}"),
            "Send {\"session_id\":\"...\"}.",
            "graph/components/comp-jam-svc-session.md",
        )
    })?;
    let active = state.active.lock().await;
    active.get(&input.session_id).cloned().ok_or_else(|| {
        SessionError::protocol(
            "unknown-session",
            format!(
                "no active or recently exited Picker session {}",
                input.session_id
            ),
            "Use tool.session.list-active to discover known sessions in this service process.",
            "graph/components/comp-jam-svc-session.md",
        )
    })
}

async fn list_active(state: &SessionState) -> Vec<PickerRecord> {
    let active = state.active.lock().await;
    active
        .values()
        .filter(|record| matches!(record.status, PickerStatus::Running | PickerStatus::Killing))
        .cloned()
        .collect()
}

#[derive(Debug, Deserialize)]
struct FullStopInput {
    session_id: String,
    reason: String,
    #[serde(alias = "from")]
    requested_by: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ArchiveSessionInput {
    session_id: String,
}

#[derive(Debug, Deserialize)]
struct PurgeSessionInput {
    session_id: String,
    reason: String,
    preserve_worktree: Option<bool>,
}

#[derive(Debug, Serialize)]
struct FullStopOutcome {
    session_id: String,
    task_id: String,
    killed_at: DateTime<Utc>,
    marker_path: String,
    tempyr_finalized: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    tempyr_detail: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct ArchiveSessionOutput {
    session_id: String,
    task_id: String,
    status: &'static str,
    worktree_path: String,
    archived_at: DateTime<Utc>,
    trace_id: String,
}

#[derive(Debug, Clone, Serialize)]
struct PurgeSessionOutput {
    session_id: String,
    task_id: String,
    status: &'static str,
    worktree_path: String,
    worktree_removed: bool,
    purged_at: DateTime<Utc>,
    reason: String,
    trace_id: String,
}

async fn archive_session(
    payload: &[u8],
    state: &SessionState,
    ctx: &TraceCtx,
    nats: &JamNats,
) -> Result<ArchiveSessionOutput, SessionError> {
    let input: ArchiveSessionInput = serde_json::from_slice(payload).map_err(|err| {
        SessionError::protocol(
            "invalid-input",
            format!("tool.session.archive-session payload is invalid JSON: {err}"),
            "Send {\"session_id\":\"...\"}.",
            "api-archive-session",
        )
    })?;
    validate_session_id_for_lifecycle(&input.session_id, "api-archive-session")?;
    let record = remove_completed_session(
        state,
        &input.session_id,
        "archive-session",
        "api-archive-session",
    )
    .await?;
    let archived_at = Utc::now();
    let output = ArchiveSessionOutput {
        session_id: record.handle.session_id,
        task_id: record.handle.task_id,
        status: "archived",
        worktree_path: record.handle.worktree_path,
        archived_at,
        trace_id: ctx.trace_id.to_string(),
    };
    publish_session_archived(nats, &output, ctx).await?;
    Ok(output)
}

async fn purge_session(
    payload: &[u8],
    state: &SessionState,
    ctx: &TraceCtx,
    nats: &JamNats,
) -> Result<PurgeSessionOutput, SessionError> {
    let input: PurgeSessionInput = serde_json::from_slice(payload).map_err(|err| {
        SessionError::protocol(
            "invalid-input",
            format!("tool.session.purge-session payload is invalid JSON: {err}"),
            "Send {\"session_id\":\"...\",\"reason\":\"...\"}.",
            "api-purge-session",
        )
    })?;
    validate_session_id_for_lifecycle(&input.session_id, "api-purge-session")?;
    validate_lifecycle_reason(&input.reason, "api-purge-session")?;
    let record = remove_completed_session(
        state,
        &input.session_id,
        "purge-session",
        "api-purge-session",
    )
    .await?;
    let purged_at = Utc::now();
    let preserve_worktree = input.preserve_worktree.unwrap_or(false);
    let worktree_removed = if preserve_worktree {
        false
    } else {
        remove_worktree_dir(&record.handle.worktree_path)?
    };
    let output = PurgeSessionOutput {
        session_id: record.handle.session_id,
        task_id: record.handle.task_id,
        status: "purged",
        worktree_path: record.handle.worktree_path,
        worktree_removed,
        purged_at,
        reason: input.reason,
        trace_id: ctx.trace_id.to_string(),
    };
    publish_task_abandoned(nats, &output.task_id, &output.reason, purged_at, ctx).await?;
    publish_session_purged(nats, &output, ctx).await?;
    Ok(output)
}

async fn full_stop_picker(
    payload: &[u8],
    state: &SessionState,
    ctx: &TraceCtx,
    nats: &JamNats,
) -> Result<FullStopOutcome, SessionError> {
    let input: FullStopInput = serde_json::from_slice(payload).map_err(|err| {
        SessionError::protocol(
            "invalid-input",
            format!("tool.session.full-stop payload is invalid JSON: {err}"),
            "Send {\"session_id\":\"...\",\"reason\":\"...\"}.",
            "task-trace-replay-tool-prove",
        )
    })?;
    validate_full_stop_input(&input)?;
    let handle = mark_session_killing(state, &input.session_id).await?;
    let pid = handle.picker_pid.ok_or_else(|| {
        SessionError::protocol(
            "missing-picker-pid",
            format!("session {} has no Picker pid", input.session_id),
            "Use inspect-picker to verify the session was spawned by this service process.",
            "task-trace-replay-tool-prove",
        )
    })?;

    terminate_process_group(pid, state.config.kill_grace).await?;
    let killed_at = Utc::now();
    let marker_path = write_killed_marker(&handle, killed_at, &input.reason)?;
    let diff_snapshot = diff_snapshot(&handle.worktree_path);
    let (tempyr_finalized, tempyr_detail) =
        finalize_tempyr_journal(&handle.worktree_path, &handle.harness, &input.reason);
    mark_session_killed(state, &handle.session_id, killed_at).await;

    let from = input
        .requested_by
        .unwrap_or_else(|| format!("maestro:{}", ctx.trace_id));
    publish_picker_killed(
        nats,
        &handle,
        &input.reason,
        killed_at,
        diff_snapshot,
        &from,
        ctx,
    )
    .await?;
    publish_task_abandoned(nats, &handle.task_id, &input.reason, killed_at, ctx).await?;

    Ok(FullStopOutcome {
        session_id: handle.session_id,
        task_id: handle.task_id,
        killed_at,
        marker_path: marker_path.to_string_lossy().into_owned(),
        tempyr_finalized,
        tempyr_detail,
    })
}

fn validate_full_stop_input(input: &FullStopInput) -> Result<(), SessionError> {
    validate_session_id_for_lifecycle(&input.session_id, "task-trace-replay-tool-prove")?;
    validate_lifecycle_reason(&input.reason, "task-trace-replay-tool-prove")?;
    Ok(())
}

fn validate_session_id_for_lifecycle(
    session_id: &str,
    tracked_by: &'static str,
) -> Result<(), SessionError> {
    if session_id.is_empty()
        || session_id.len() > TOKEN_MAX_LEN
        || session_id.contains('/')
        || session_id.contains("..")
    {
        return Err(SessionError::protocol(
            "invalid-input",
            format!("session_id is not a known safe session handle: {session_id}"),
            "Use inspect-picker or list-active to copy an existing session_id.",
            tracked_by,
        ));
    }
    Ok(())
}

fn validate_lifecycle_reason(reason: &str, tracked_by: &'static str) -> Result<(), SessionError> {
    if reason.trim().is_empty() || reason.contains('\0') {
        return Err(SessionError::protocol(
            "invalid-input",
            "session lifecycle reason may not be empty or contain NUL",
            "Send a short reason for the session audit trail.",
            tracked_by,
        ));
    }
    Ok(())
}

async fn remove_completed_session(
    state: &SessionState,
    session_id: &str,
    action: &'static str,
    tracked_by: &'static str,
) -> Result<PickerRecord, SessionError> {
    let mut active = state.active.lock().await;
    let record = active.get(session_id).cloned().ok_or_else(|| {
        SessionError::protocol(
            "unknown-session",
            format!("no Picker session {session_id} in this service process"),
            "Use tool.session.list-active or inspect-picker before archiving or purging.",
            tracked_by,
        )
    })?;
    if matches!(record.status, PickerStatus::Running | PickerStatus::Killing) {
        return Err(SessionError::protocol(
            "session-still-running",
            format!("{action} requires a completed session; {session_id} is {:?}", record.status),
            "Use full-stop for running sessions, then archive or purge once the session is killed/exited.",
            tracked_by,
        ));
    }
    active.remove(session_id);
    Ok(record)
}

fn remove_worktree_dir(path: &str) -> Result<bool, SessionError> {
    let worktree = validate_worktree_path(path)?;
    if !worktree.exists() {
        return Ok(false);
    }
    fs::remove_dir_all(&worktree).map_err(|err| {
        SessionError::protocol(
            "worktree-remove-failed",
            format!("failed to remove {}: {err}", worktree.display()),
            "Inspect the worktree permissions, then retry purge-session or preserve the worktree.",
            "api-purge-session",
        )
    })?;
    Ok(true)
}

async fn mark_session_killing(
    state: &SessionState,
    session_id: &str,
) -> Result<PickerHandle, SessionError> {
    let mut active = state.active.lock().await;
    let record = active.get_mut(session_id).ok_or_else(|| {
        SessionError::protocol(
            "unknown-session",
            format!("no Picker session {session_id} in this service process"),
            "Use tool.session.list-active to discover known sessions before full-stop.",
            "task-trace-replay-tool-prove",
        )
    })?;
    if record.status != PickerStatus::Running {
        return Err(SessionError::protocol(
            "session-not-running",
            format!("session {session_id} is {:?}", record.status),
            "Use inspect-picker; only running sessions can be full-stopped.",
            "task-trace-replay-tool-prove",
        ));
    }
    record.status = PickerStatus::Killing;
    Ok(record.handle.clone())
}

async fn mark_session_killed(state: &SessionState, session_id: &str, killed_at: DateTime<Utc>) {
    let mut active = state.active.lock().await;
    if let Some(record) = active.get_mut(session_id) {
        record.status = PickerStatus::Killed;
        record.exited_at = Some(killed_at);
    }
}

async fn terminate_process_group(pid: u32, grace: Duration) -> Result<(), SessionError> {
    send_signal_to_group(pid, "TERM")?;
    let deadline = tokio::time::Instant::now() + grace;
    while tokio::time::Instant::now() < deadline {
        if !process_group_alive(pid) {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    if process_group_alive(pid) {
        send_signal_to_group(pid, "KILL")?;
    }
    Ok(())
}

fn send_signal_to_group(pid: u32, signal: &str) -> Result<(), SessionError> {
    let group = format!("-{pid}");
    let status = StdCommand::new("kill")
        .arg(format!("-{signal}"))
        .arg("--")
        .arg(&group)
        .status()
        .map_err(|err| {
            SessionError::protocol(
                "kill-command-failed",
                format!("failed to run kill -{signal} {group}: {err}"),
                "Verify /usr/bin/kill is available in the service environment.",
                "task-trace-replay-tool-prove",
            )
        })?;
    if status.success() {
        Ok(())
    } else {
        Err(SessionError::protocol(
            "kill-command-failed",
            format!("kill -{signal} {group} exited with {status}"),
            "Use inspect-picker to verify the process still exists and belongs to this service.",
            "task-trace-replay-tool-prove",
        ))
    }
}

fn process_group_alive(pid: u32) -> bool {
    StdCommand::new("kill")
        .arg("-0")
        .arg("--")
        .arg(format!("-{pid}"))
        .status()
        .is_ok_and(|status| status.success())
}

fn write_killed_marker(
    handle: &PickerHandle,
    killed_at: DateTime<Utc>,
    reason: &str,
) -> Result<PathBuf, SessionError> {
    let marker_path = killed_marker_path(&handle.worktree_path, killed_at);
    let body = format!(
        "session_id: {}\ntask_id: {}\nkilled_at: {}\nreason: {}\n",
        handle.session_id,
        handle.task_id,
        killed_at.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        reason
    );
    fs::write(&marker_path, body).map_err(|err| {
        SessionError::protocol(
            "killed-marker-write-failed",
            format!("failed to write {}: {err}", marker_path.display()),
            "Verify the Picker worktree is still writable before retrying full-stop.",
            "task-trace-replay-tool-prove",
        )
    })?;
    Ok(marker_path)
}

fn killed_marker_path(worktree_path: &str, killed_at: DateTime<Utc>) -> PathBuf {
    Path::new(worktree_path).join(format!(".killed-at-{}", killed_at.format("%Y%m%dT%H%M%SZ")))
}

fn diff_snapshot(worktree_path: &str) -> Option<String> {
    let status = git_output(worktree_path, &["status", "--short"]);
    let diff = git_output(worktree_path, &["diff", "--stat"]);
    match (status, diff) {
        (None, None) => None,
        (status, diff) => Some(format!(
            "status --short:\n{}\ndiff --stat:\n{}",
            status.unwrap_or_default(),
            diff.unwrap_or_default()
        )),
    }
}

fn git_output(worktree_path: &str, args: &[&str]) -> Option<String> {
    let output = StdCommand::new("git")
        .arg("-C")
        .arg(worktree_path)
        .args(args)
        .output()
        .ok()?;
    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    } else {
        None
    }
}

fn finalize_tempyr_journal(
    worktree_path: &str,
    harness: &str,
    reason: &str,
) -> (bool, Option<String>) {
    if harness == OPENCODE_HARNESS {
        return finalize_opencode_tempyr_journal(worktree_path, reason);
    }
    let output = StdCommand::new("tempyr")
        .arg("journal")
        .arg("log")
        .arg("--agent")
        .arg(tempyr_agent_for_harness(harness))
        .arg("outcome")
        .arg(format!("killed by full-stop: {reason}"))
        .arg("--passed")
        .arg("false")
        .arg("--final")
        .current_dir(worktree_path)
        .output();
    match output {
        Ok(output) if output.status.success() => (true, None),
        Ok(output) => (
            false,
            Some(String::from_utf8_lossy(&output.stderr).trim().to_owned()),
        ),
        Err(err) => (false, Some(err.to_string())),
    }
}

fn finalize_opencode_tempyr_journal(worktree_path: &str, _reason: &str) -> (bool, Option<String>) {
    let finalize_output = StdCommand::new("tempyr")
        .arg("journal")
        .arg("finalize")
        .arg("--agent")
        .arg("opencode")
        .arg("--quiet")
        .current_dir(worktree_path)
        .output();
    match command_failure_detail(finalize_output) {
        Some(detail) => (false, Some(detail)),
        None => (true, None),
    }
}

fn command_failure_detail(output: std::io::Result<std::process::Output>) -> Option<String> {
    match output {
        Ok(output) if output.status.success() => None,
        Ok(output) => Some(String::from_utf8_lossy(&output.stderr).trim().to_owned()),
        Err(err) => Some(err.to_string()),
    }
}

fn tempyr_agent_for_harness(harness: &str) -> &'static str {
    match harness {
        CLAUDE_HARNESS => "claude",
        OPENCODE_HARNESS => "opencode",
        _ => "codex",
    }
}

async fn publish_picker_killed(
    nats: &JamNats,
    handle: &PickerHandle,
    reason: &str,
    killed_at: DateTime<Utc>,
    diff_snapshot: Option<String>,
    from: &str,
    ctx: &TraceCtx,
) -> Result<(), SessionError> {
    let payload = PickerKilled {
        session_id: handle.session_id.clone(),
        task_id: handle.task_id.clone(),
        reason: reason.into(),
        killed_at,
        diff_snapshot,
        from: from.into(),
    };
    let envelope = EventEnvelope::new(
        PickerKilled::EVENT_TYPE,
        PickerKilled::EVENT_SUBTYPE_VERSION,
        0,
        ctx.trace_id.to_string(),
        SERVICE_NAME,
        payload,
    );
    nats.publish_traced("journal.picker.killed", &envelope, ctx)
        .await
        .map_err(|err| {
            SessionError::protocol(
                "journal-publish-failed",
                err.to_string(),
                "Verify NATS is running and the journal bridge is healthy.",
                "principle-failure-surfaces-immediately",
            )
        })
}

async fn publish_task_abandoned(
    nats: &JamNats,
    task_id: &str,
    reason: &str,
    abandoned_at: DateTime<Utc>,
    ctx: &TraceCtx,
) -> Result<(), SessionError> {
    let payload = TaskAbandoned {
        task_id: task_id.into(),
        reason: reason.into(),
        abandoned_at,
    };
    let envelope = EventEnvelope::new(
        TaskAbandoned::EVENT_TYPE,
        TaskAbandoned::EVENT_SUBTYPE_VERSION,
        0,
        ctx.trace_id.to_string(),
        SERVICE_NAME,
        payload,
    );
    nats.publish_traced("journal.task.abandoned", &envelope, ctx)
        .await
        .map_err(|err| {
            SessionError::protocol(
                "journal-publish-failed",
                err.to_string(),
                "Verify NATS is running and the journal bridge is healthy.",
                "principle-failure-surfaces-immediately",
            )
        })
}

async fn publish_session_archived(
    nats: &JamNats,
    output: &ArchiveSessionOutput,
    ctx: &TraceCtx,
) -> Result<(), SessionError> {
    nats.publish_traced("journal.session.archived", output, ctx)
        .await
        .map_err(|err| {
            SessionError::protocol(
                "journal-publish-failed",
                err.to_string(),
                "Verify NATS is running and the journal bridge is healthy.",
                "principle-failure-surfaces-immediately",
            )
        })
}

async fn publish_session_purged(
    nats: &JamNats,
    output: &PurgeSessionOutput,
    ctx: &TraceCtx,
) -> Result<(), SessionError> {
    nats.publish_traced("journal.session.purged", output, ctx)
        .await
        .map_err(|err| {
            SessionError::protocol(
                "journal-publish-failed",
                err.to_string(),
                "Verify NATS is running and the journal bridge is healthy.",
                "principle-failure-surfaces-immediately",
            )
        })
}

fn parse_spawn_input(payload: &[u8]) -> Result<SpawnPickerInput, SessionError> {
    serde_json::from_slice(payload).map_err(|err| {
        SessionError::protocol(
            "invalid-input",
            format!("tool.session.spawn-picker payload is invalid JSON: {err}"),
            "Send a JSON object with task_id and optional Codex/local/default fields.",
            "task-jam-svc-session-codex-cli-only",
        )
    })
}

#[derive(Debug, Deserialize)]
struct HarnessLockfile {
    harnesses: HashMap<String, HarnessPin>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct HarnessPin {
    version: String,
    checksum_sha256: String,
}

fn verify_harness_lockfile(
    harness: &str,
    harness_bin: &Path,
    lockfile_path: &Path,
) -> Result<(), SessionError> {
    let raw = fs::read_to_string(lockfile_path).map_err(|err| {
        SessionError::protocol(
            "harness-lockfile-missing",
            format!("failed to read {}: {err}", lockfile_path.display()),
            "Create ~/.jam/config/projects/blueberry-harnesses.lock with pinned harness entries before spawning.",
            "comp-harness-version-lockfile",
        )
    })?;
    let lockfile: HarnessLockfile = toml::from_str(&raw).map_err(|err| {
        SessionError::protocol(
            "harness-lockfile-invalid",
            format!("failed to parse {}: {err}", lockfile_path.display()),
            "Fix the harness lockfile TOML before spawning.",
            "comp-harness-version-lockfile",
        )
    })?;
    let pin = lockfile.harnesses.get(harness).ok_or_else(|| {
        SessionError::protocol(
            "harness-not-pinned",
            format!("{harness} is not pinned in the Blueberry harness lockfile"),
            "Add [harnesses.<id>] with version and checksum-sha256.",
            "comp-harness-version-lockfile",
        )
    })?;

    let version = harness_version(harness, harness_bin)?;
    if version != pin.version {
        return Err(SessionError::protocol(
            "harness-version-drift",
            format!("{harness} version is {version}, lockfile expects {}", pin.version),
            "Run the harness validation workflow, then update the lockfile if this version is approved.",
            "comp-harness-version-lockfile",
        ));
    }

    let checksum = sha256_file(harness, harness_bin)?;
    if checksum != pin.checksum_sha256 {
        return Err(SessionError::protocol(
            "harness-checksum-drift",
            format!(
                "{harness} checksum is {checksum}, lockfile expects {}",
                pin.checksum_sha256
            ),
            "Reinstall the pinned harness or validate and update the lockfile checksum.",
            "comp-harness-version-lockfile",
        ));
    }
    Ok(())
}

fn harness_lockfile_error_blocks(policy: HarnessLockfilePolicy, err: &SessionError) -> bool {
    match policy {
        HarnessLockfilePolicy::Strict => true,
        HarnessLockfilePolicy::Off => false,
        HarnessLockfilePolicy::Warn => !matches!(
            err,
            SessionError::Protocol {
                kind: "harness-version-drift" | "harness-checksum-drift",
                ..
            }
        ),
    }
}

fn harness_version(harness: &str, harness_bin: &Path) -> Result<String, SessionError> {
    let output = std::process::Command::new(harness_bin)
        .arg("--version")
        .output()
        .map_err(|err| {
            SessionError::protocol(
                "harness-version-check-failed",
                format!("failed to run {} --version: {err}", harness_bin.display()),
                "Install the harness CLI for the service user or set the matching JAM_*_BIN variable.",
                "comp-harness-version-lockfile",
            )
        })?;
    if !output.status.success() {
        return Err(SessionError::protocol(
            "harness-version-check-failed",
            format!(
                "{} --version failed: {}",
                harness_bin.display(),
                String::from_utf8_lossy(&output.stderr).trim()
            ),
            "Install the harness CLI for the service user or set the matching JAM_*_BIN variable.",
            "comp-harness-version-lockfile",
        ));
    }
    parse_harness_version(&String::from_utf8_lossy(&output.stdout)).ok_or_else(|| {
        SessionError::protocol(
            "harness-version-check-failed",
            format!(
                "{} --version output for {harness} was not understood: {}",
                harness_bin.display(),
                String::from_utf8_lossy(&output.stdout).trim()
            ),
            "Update jam-svc-session's harness version parser for the installed CLI.",
            "comp-harness-version-lockfile",
        )
    })
}

fn parse_harness_version(output: &str) -> Option<String> {
    let trimmed = output.trim();
    if trimmed.is_empty() {
        return None;
    }
    trimmed
        .split_whitespace()
        .find(|part| part.chars().next().is_some_and(|ch| ch.is_ascii_digit()))
        .map(ToOwned::to_owned)
}

fn sha256_file(harness: &str, path: &Path) -> Result<String, SessionError> {
    let canonical = resolve_binary_path(harness, path)?;
    let bytes = fs::read(&canonical).map_err(|err| {
        SessionError::protocol(
            "harness-checksum-failed",
            format!("failed to read {}: {err}", canonical.display()),
            "Verify the configured harness binary path is readable by jam-svc-session.",
            "comp-harness-version-lockfile",
        )
    })?;
    let digest = Sha256::digest(bytes);
    Ok(hex::encode(digest))
}

fn resolve_binary_path(_harness: &str, path: &Path) -> Result<PathBuf, SessionError> {
    if path.components().count() > 1 || path.is_absolute() {
        return path.canonicalize().map_err(|err| {
            SessionError::protocol(
                "harness-binary-not-found",
                format!("failed to canonicalize {}: {err}", path.display()),
                "Set the matching JAM_*_BIN path to an installed harness binary.",
                "comp-harness-version-lockfile",
            )
        });
    }
    let path_var = std::env::var_os("PATH").unwrap_or_default();
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(path);
        if candidate.is_file() {
            return candidate.canonicalize().map_err(|err| {
                SessionError::protocol(
                    "harness-binary-not-found",
                    format!("failed to canonicalize {}: {err}", candidate.display()),
                    "Set the matching JAM_*_BIN path to an installed harness binary.",
                    "comp-harness-version-lockfile",
                )
            });
        }
    }
    Err(SessionError::protocol(
        "harness-binary-not-found",
        format!("could not find {} on PATH", path.display()),
        "Set the matching JAM_*_BIN path to an installed harness binary.",
        "comp-harness-version-lockfile",
    ))
}

fn write_picker_metadata(
    worktree_path: &Path,
    spec: &SpawnSpec,
    picker_trace: &TraceCtx,
    parent_trace: &TraceCtx,
) -> Result<(), SessionError> {
    let git_dir = git_dir_for_worktree(worktree_path, &spec.task_id)?;
    fs::create_dir_all(&git_dir).map_err(|err| {
        SessionError::protocol(
            "picker-metadata-write-failed",
            format!("failed to create {}: {err}", git_dir.display()),
            "Verify the worktree is writable by jam-svc-session before launch.",
            "task-tempyr-journal-integration-maestro",
        )
    })?;
    let metadata_path = git_dir.join("jam-picker-env.toml");
    let agent = format!(
        "picker:{}:{}",
        tempyr_agent_for_harness(&spec.harness),
        spec.task_id
    );
    let contents = format!(
        "JAM_TRACE_ID = \"{}\"\nJAM_PARENT_TRACE_ID = \"{}\"\nJAM_TASK_ID = \"{}\"\nTEMPYR_AGENT = \"{}\"\n",
        picker_trace.trace_id, parent_trace.trace_id, spec.task_id, agent
    );
    fs::write(&metadata_path, contents).map_err(|err| {
        SessionError::protocol(
            "picker-metadata-write-failed",
            format!("failed to write {}: {err}", metadata_path.display()),
            "Verify the worktree is writable by jam-svc-session before launch.",
            "task-tempyr-journal-integration-maestro",
        )
    })
}

fn git_dir_for_worktree(worktree_path: &Path, task_id: &str) -> Result<PathBuf, SessionError> {
    let output = run_git_dir(worktree_path)?;
    if !output.status.success() {
        repair_broken_linked_worktree(worktree_path, task_id)?;
        let retry = run_git_dir(worktree_path)?;
        if retry.status.success() {
            let raw = String::from_utf8_lossy(&retry.stdout);
            return resolve_git_metadata_path(worktree_path, raw.trim());
        }
        return Err(worktree_gitdir_error(format!(
            "git -C {} rev-parse --git-dir failed after repair: {}",
            worktree_path.display(),
            String::from_utf8_lossy(&retry.stderr).trim()
        )));
    }
    let raw = String::from_utf8_lossy(&output.stdout);
    resolve_git_metadata_path(worktree_path, raw.trim())
}

fn run_git_dir(worktree_path: &Path) -> Result<Output, SessionError> {
    std::process::Command::new("git")
        .arg("-C")
        .arg(worktree_path)
        .args(["rev-parse", "--git-dir"])
        .output()
        .map_err(|err| {
            worktree_gitdir_error(format!(
                "failed to inspect git metadata for {}: {err}",
                worktree_path.display(),
            ))
        })
}

fn repair_broken_linked_worktree(worktree_path: &Path, task_id: &str) -> Result<(), SessionError> {
    let git_file = worktree_path.join(".git");
    let old_admin_dir = read_gitdir_pointer(&git_file, worktree_path)?;
    let repo_path = repo_path_from_worktree_admin(&old_admin_dir)?;
    let desired_admin_dir = desired_repaired_admin_dir(worktree_path, &old_admin_dir)?;
    let branch = branch_for_task(task_id);
    let repair_path = worktree_path
        .parent()
        .ok_or_else(|| {
            worktree_gitdir_error(format!(
                "cannot repair {} because it has no parent directory",
                worktree_path.display()
            ))
        })?
        .join(format!(
            ".jam-repair-{}-{}",
            task_id.replace('/', "-"),
            std::process::id()
        ));
    if repair_path.exists() {
        fs::remove_dir_all(&repair_path).map_err(|err| {
            worktree_gitdir_error(format!(
                "failed to remove stale repair path {}: {err}",
                repair_path.display()
            ))
        })?;
    }

    let output = StdCommand::new("git")
        .arg("-C")
        .arg(&repo_path)
        .args(["worktree", "add", "--force"])
        .arg(&repair_path)
        .arg(&branch)
        .output()
        .map_err(|err| {
            worktree_gitdir_error(format!(
                "failed to run git worktree repair fallback for {}: {err}",
                worktree_path.display()
            ))
        })?;
    if !output.status.success() {
        return Err(worktree_gitdir_error(format!(
            "git -C {} worktree add --force {} {} failed while repairing {}: {}",
            repo_path.display(),
            repair_path.display(),
            branch,
            worktree_path.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }

    let new_admin_dir = read_gitdir_pointer(&repair_path.join(".git"), &repair_path)?
        .canonicalize()
        .map_err(|err| {
            worktree_gitdir_error(format!(
                "failed to canonicalize repaired git dir for {}: {err}",
                repair_path.display()
            ))
        })?;
    if desired_admin_dir.exists() && desired_admin_dir != new_admin_dir {
        fs::remove_dir_all(&desired_admin_dir).map_err(|err| {
            worktree_gitdir_error(format!(
                "failed to replace stale git admin dir {}: {err}",
                desired_admin_dir.display()
            ))
        })?;
    }
    if let Some(parent) = desired_admin_dir.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            worktree_gitdir_error(format!(
                "failed to create git worktrees dir {}: {err}",
                parent.display()
            ))
        })?;
    }
    if new_admin_dir != desired_admin_dir {
        fs::rename(&new_admin_dir, &desired_admin_dir).map_err(|err| {
            worktree_gitdir_error(format!(
                "failed to move repaired git admin dir {} to {}: {err}",
                new_admin_dir.display(),
                desired_admin_dir.display()
            ))
        })?;
    }

    fs::write(
        &git_file,
        format!("gitdir: {}\n", desired_admin_dir.display()),
    )
    .map_err(|err| {
        worktree_gitdir_error(format!(
            "failed to repoint {} to repaired git dir: {err}",
            git_file.display()
        ))
    })?;
    fs::write(
        desired_admin_dir.join("gitdir"),
        format!("{}/.git\n", worktree_path.display()),
    )
    .map_err(|err| {
        worktree_gitdir_error(format!(
            "failed to repoint repaired git admin dir {}: {err}",
            desired_admin_dir.display()
        ))
    })?;
    if let Err(err) = fs::remove_dir_all(&repair_path) {
        warn!(
            path = %repair_path.display(),
            "failed to remove temporary worktree repair directory: {err}"
        );
    }
    Ok(())
}

fn desired_repaired_admin_dir(
    worktree_path: &Path,
    current_admin_dir: &Path,
) -> Result<PathBuf, SessionError> {
    let worktrees_dir = current_admin_dir.parent().ok_or_else(|| {
        worktree_gitdir_error(format!(
            "cannot infer stable git admin dir from {}",
            current_admin_dir.display()
        ))
    })?;
    let current_name = current_admin_dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    if current_name.starts_with("-jam-repair-") || current_name.starts_with(".jam-repair-") {
        let worktree_name = worktree_path.file_name().ok_or_else(|| {
            worktree_gitdir_error(format!(
                "cannot infer stable git admin dir for {}",
                worktree_path.display()
            ))
        })?;
        Ok(worktrees_dir.join(worktree_name))
    } else {
        Ok(current_admin_dir.to_path_buf())
    }
}

fn read_gitdir_pointer(git_file: &Path, worktree_path: &Path) -> Result<PathBuf, SessionError> {
    let raw = fs::read_to_string(git_file).map_err(|err| {
        worktree_gitdir_error(format!(
            "cannot repair {} because {} is unreadable: {err}",
            worktree_path.display(),
            git_file.display()
        ))
    })?;
    let pointer = raw
        .trim()
        .strip_prefix("gitdir:")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            worktree_gitdir_error(format!(
                "cannot repair {} because {} is not a gitdir pointer",
                worktree_path.display(),
                git_file.display()
            ))
        })?;
    let path = PathBuf::from(pointer);
    Ok(if path.is_absolute() {
        path
    } else {
        worktree_path.join(path)
    })
}

fn repo_path_from_worktree_admin(admin_dir: &Path) -> Result<PathBuf, SessionError> {
    let worktrees_dir = admin_dir.parent().ok_or_else(|| {
        worktree_gitdir_error(format!(
            "cannot infer repository from git admin dir {}",
            admin_dir.display()
        ))
    })?;
    if worktrees_dir.file_name().and_then(|name| name.to_str()) != Some("worktrees") {
        return Err(worktree_gitdir_error(format!(
            "cannot infer repository because {} is not under a worktrees directory",
            admin_dir.display()
        )));
    }
    let git_dir = worktrees_dir.parent().ok_or_else(|| {
        worktree_gitdir_error(format!(
            "cannot infer repository from git admin dir {}",
            admin_dir.display()
        ))
    })?;
    if git_dir.file_name().and_then(|name| name.to_str()) != Some(".git") {
        return Err(worktree_gitdir_error(format!(
            "cannot infer non-bare repository from git admin dir {}",
            admin_dir.display()
        )));
    }
    git_dir
        .parent()
        .ok_or_else(|| {
            worktree_gitdir_error(format!(
                "cannot infer repository from git dir {}",
                git_dir.display()
            ))
        })?
        .canonicalize()
        .map_err(|err| {
            worktree_gitdir_error(format!(
                "failed to canonicalize repository for git admin dir {}: {err}",
                admin_dir.display()
            ))
        })
}

fn worktree_gitdir_error(detail: impl Into<String>) -> SessionError {
    SessionError::protocol(
        "worktree-gitdir-failed",
        detail,
        "Verify jam-svc-worktree returned a valid git worktree.",
        "task-jam-svc-worktree-creation-protocol",
    )
}

fn resolve_git_metadata_path(
    worktree_path: &Path,
    raw_git_path: &str,
) -> Result<PathBuf, SessionError> {
    let raw = PathBuf::from(raw_git_path);
    let candidate = if raw.is_absolute() {
        raw
    } else {
        worktree_path.join(raw)
    };
    candidate.canonicalize().map_err(|err| {
        SessionError::protocol(
            "worktree-gitdir-failed",
            format!(
                "failed to canonicalize git dir {}: {err}",
                candidate.display()
            ),
            "Verify jam-svc-worktree returned a valid git worktree.",
            "task-jam-svc-worktree-creation-protocol",
        )
    })
}

fn validate_worktree_path(path: &str) -> Result<PathBuf, SessionError> {
    let raw = PathBuf::from(path);
    if !raw.is_absolute() || is_windows_mount(&raw) {
        return Err(SessionError::protocol(
            "invalid-worktree-path",
            format!(
                "worktree path must be native Linux absolute path: {}",
                raw.display()
            ),
            "Fix jam-svc-worktree so it returns a Linux-native /home/<user>/... worktree.",
            "principle-native-fs-only",
        ));
    }
    if raw.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::CurDir | Component::Prefix(_)
        )
    }) {
        return Err(SessionError::protocol(
            "invalid-worktree-path",
            format!(
                "worktree path contains unsafe components: {}",
                raw.display()
            ),
            "Fix jam-svc-worktree path construction and retry.",
            "principle-native-fs-only",
        ));
    }
    raw.canonicalize().map_err(|err| {
        SessionError::protocol(
            "worktree-not-found",
            format!("failed to canonicalize {}: {err}", raw.display()),
            "Verify jam-svc-worktree created the worktree before returning success.",
            "task-jam-svc-worktree-creation-protocol",
        )
    })
}

fn is_windows_mount(path: &Path) -> bool {
    let mut components = path.components();
    if !matches!(components.next(), Some(Component::RootDir)) {
        return false;
    }
    let Some(Component::Normal(first)) = components.next() else {
        return false;
    };
    if first == "cygdrive" {
        return true;
    }
    if first != "mnt" {
        return false;
    }
    matches!(components.next(), Some(Component::Normal(drive)) if drive.to_string_lossy().len() == 1)
}

fn validate_token(name: &'static str, value: &str, max_len: usize) -> Result<(), SessionError> {
    if value.is_empty() || value.len() > max_len {
        return Err(SessionError::protocol(
            "invalid-token",
            format!("{name} must be 1-{max_len} characters"),
            "Use slug-like values with letters, numbers, dots, underscores, and dashes.",
            "task-jam-svc-session-codex-cli-only",
        ));
    }
    if value == "." || value == ".." || value.contains("..") {
        return Err(SessionError::protocol(
            "invalid-token",
            format!("{name} may not contain parent-directory segments: {value}"),
            "Use slug-like values with letters, numbers, dots, underscores, and dashes.",
            "task-jam-svc-session-codex-cli-only",
        ));
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(SessionError::protocol(
            "invalid-token",
            format!("{name} contains unsafe characters: {value}"),
            "Use slug-like values with letters, numbers, dots, underscores, and dashes.",
            "task-jam-svc-session-codex-cli-only",
        ));
    }
    Ok(())
}

fn validate_model_id(name: &'static str, value: &str, max_len: usize) -> Result<(), SessionError> {
    if value.is_empty() || value.len() > max_len {
        return Err(SessionError::protocol(
            "invalid-model-id",
            format!("{name} must be 1-{max_len} characters"),
            "Use a model id such as gpt-5.2, sonnet, or deepseek/deepseek-v4-pro.",
            "task-opencode-deepseek-adapter-impl",
        ));
    }
    if value == "."
        || value == ".."
        || value.contains("..")
        || value.starts_with('/')
        || value.ends_with('/')
        || value.contains("//")
    {
        return Err(SessionError::protocol(
            "invalid-model-id",
            format!("{name} contains unsafe path-like segments: {value}"),
            "Use a provider/model id without empty or parent-directory segments.",
            "task-opencode-deepseek-adapter-impl",
        ));
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b'/'))
    {
        return Err(SessionError::protocol(
            "invalid-model-id",
            format!("{name} contains unsafe characters: {value}"),
            "Use letters, numbers, dots, underscores, dashes, and provider/model slashes.",
            "task-opencode-deepseek-adapter-impl",
        ));
    }
    Ok(())
}

fn error_response(err: SessionError) -> Response {
    match err {
        SessionError::Protocol {
            kind,
            detail,
            remediation,
            tracked_by,
        } => Response::Error {
            error: ResponseError {
                kind: kind.into(),
                detail,
                remediation: remediation.into(),
                tracked_by,
            },
        },
    }
}

fn method_from_subject(subject: &str) -> Option<&str> {
    let (before_last, last) = subject.rsplit_once('.')?;
    let previous = before_last
        .rsplit_once('.')
        .map_or(before_last, |(_, previous)| previous);
    if matches!(previous, "ping" | "drain") {
        Some(previous)
    } else {
        Some(last)
    }
}

fn configured_subject_prefix(service_env: &str, default_prefix: &str) -> String {
    std::env::var(service_env)
        .or_else(|_| std::env::var("JAM_TOOL_SUBJECT_PREFIX"))
        .ok()
        .filter(|prefix| !prefix.trim().is_empty())
        .unwrap_or_else(|| default_prefix.into())
}

fn parse_bool_env(name: &str) -> Option<bool> {
    let raw = std::env::var(name).ok()?;
    match raw.as_str() {
        "1" | "true" | "TRUE" | "yes" | "YES" => Some(true),
        "0" | "false" | "FALSE" | "no" | "NO" => Some(false),
        _ => None,
    }
}

fn shell_words(raw: &str) -> Vec<String> {
    raw.split_whitespace().map(ToOwned::to_owned).collect()
}

fn default_dry_run_command() -> Vec<String> {
    vec!["sleep".into(), "300".into()]
}

fn default_picker_path() -> OsString {
    "/home/caleb/.local/share/tempyr/bin:/home/picker/.local/share/tempyr/bin:/home/picker/.cargo/bin:/home/picker/.npm-global/bin:/home/maestro/.npm-global/bin:/usr/local/bin:/usr/bin:/bin".into()
}

fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("jam_svc_session=info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;
    use tempfile::TempDir;

    #[test]
    fn versioned_subject_prefix_subscription_keeps_methods_parseable() {
        let prefix = "tool.session.v047";

        assert_eq!(format!("{prefix}.>"), "tool.session.v047.>");
        assert_eq!(
            method_from_subject("tool.session.v047.spawn-picker"),
            Some("spawn-picker")
        );
        assert_eq!(method_from_subject("tool.session.v047.ping"), Some("ping"));
    }

    #[test]
    fn validates_live_spawn_supported_harnesses_and_sandbox_combinations() {
        let input = SpawnPickerInput {
            task_id: "task-1".into(),
            project: None,
            harness: Some("claude-code".into()),
            sandbox_backend: None,
            sandbox_profile: None,
            task_class: None,
            initial_prompt: Some("do work".into()),
            model_override: None,
            reasoning_effort: None,
            budget_usd: None,
            dry_run: None,
        };
        let spec = SpawnSpec::from_input(input).unwrap();
        assert_eq!(spec.harness, CLAUDE_HARNESS);

        let input = SpawnPickerInput {
            task_id: "task-1".into(),
            project: None,
            harness: Some(OPENCODE_HARNESS.into()),
            sandbox_backend: None,
            sandbox_profile: None,
            task_class: None,
            initial_prompt: Some("do work".into()),
            model_override: None,
            reasoning_effort: None,
            budget_usd: None,
            dry_run: None,
        };
        let spec = SpawnSpec::from_input(input).unwrap();
        assert_eq!(spec.harness, OPENCODE_HARNESS);

        let input = SpawnPickerInput {
            task_id: "task-1".into(),
            project: None,
            harness: None,
            sandbox_backend: Some("docker".into()),
            sandbox_profile: None,
            task_class: None,
            initial_prompt: Some("do work".into()),
            model_override: None,
            reasoning_effort: None,
            budget_usd: None,
            dry_run: None,
        };
        let spec = SpawnSpec::from_input(input).unwrap();
        assert_eq!(spec.sandbox_backend, DOCKER_SANDBOX_BACKEND);

        let input = SpawnPickerInput {
            task_id: "task-1".into(),
            project: None,
            harness: None,
            sandbox_backend: Some(DOCKER_SANDBOX_BACKEND.into()),
            sandbox_profile: Some(HARDENED_SANDBOX_PROFILE.into()),
            task_class: None,
            initial_prompt: Some("do work".into()),
            model_override: None,
            reasoning_effort: None,
            budget_usd: None,
            dry_run: None,
        };
        let spec = SpawnSpec::from_input(input).unwrap();
        assert_eq!(spec.sandbox_profile, HARDENED_SANDBOX_PROFILE);

        let input = SpawnPickerInput {
            task_id: "task-1".into(),
            project: None,
            harness: None,
            sandbox_backend: None,
            sandbox_profile: Some(HARDENED_SANDBOX_PROFILE.into()),
            task_class: None,
            initial_prompt: Some("do work".into()),
            model_override: None,
            reasoning_effort: None,
            budget_usd: None,
            dry_run: None,
        };
        assert!(SpawnSpec::from_input(input)
            .unwrap_err()
            .to_string()
            .contains("unsupported-sandbox-combination"));
    }

    #[test]
    fn accepts_known_non_codex_harnesses_for_dry_run_only() {
        for harness in ["claude-code", "opencode-deepseek"] {
            let input = SpawnPickerInput {
                task_id: "task-1".into(),
                project: None,
                harness: Some(harness.into()),
                sandbox_backend: None,
                sandbox_profile: None,
                task_class: None,
                initial_prompt: Some("do work".into()),
                model_override: None,
                reasoning_effort: None,
                budget_usd: None,
                dry_run: Some(true),
            };
            let spec = SpawnSpec::from_input(input).unwrap();
            assert_eq!(spec.harness, harness);
            assert!(spec.dry_run);
        }
    }

    #[test]
    fn rejects_unknown_harness_even_for_dry_run() {
        let input = SpawnPickerInput {
            task_id: "task-1".into(),
            project: None,
            harness: Some("unknown-harness".into()),
            sandbox_backend: None,
            sandbox_profile: None,
            task_class: None,
            initial_prompt: Some("do work".into()),
            model_override: None,
            reasoning_effort: None,
            budget_usd: None,
            dry_run: Some(true),
        };

        assert!(SpawnSpec::from_input(input).is_err());
    }

    #[test]
    fn rejects_unsafe_task_ids() {
        for task_id in ["", "../x", "x/y", "x y", "..", "a..b"] {
            let input = SpawnPickerInput {
                task_id: task_id.into(),
                project: None,
                harness: None,
                sandbox_backend: None,
                sandbox_profile: None,
                task_class: None,
                initial_prompt: Some("do work".into()),
                model_override: None,
                reasoning_effort: None,
                budget_usd: None,
                dry_run: None,
            };
            assert!(SpawnSpec::from_input(input).is_err(), "{task_id}");
        }
    }

    #[test]
    fn accepts_provider_model_overrides() {
        let input = SpawnPickerInput {
            task_id: "task-1".into(),
            project: None,
            harness: Some(OPENCODE_HARNESS.into()),
            sandbox_backend: None,
            sandbox_profile: None,
            task_class: None,
            initial_prompt: Some("do work".into()),
            model_override: Some("deepseek/deepseek-v4-pro".into()),
            reasoning_effort: None,
            budget_usd: None,
            dry_run: None,
        };

        let spec = SpawnSpec::from_input(input).unwrap();

        assert_eq!(
            spec.model_override.as_deref(),
            Some("deepseek/deepseek-v4-pro")
        );
    }

    #[test]
    fn spawn_prompt_instructs_picker_to_write_pr_metadata() {
        let input = SpawnPickerInput {
            task_id: "task-1".into(),
            project: None,
            harness: None,
            sandbox_backend: None,
            sandbox_profile: None,
            task_class: None,
            initial_prompt: Some("Implement the requested cleanup.".into()),
            model_override: None,
            reasoning_effort: None,
            budget_usd: None,
            dry_run: None,
        };

        let spec = SpawnSpec::from_input(input).unwrap();

        assert!(spec
            .initial_prompt
            .contains("Implement the requested cleanup."));
        assert!(spec.initial_prompt.contains(".jam/pr-title.txt"));
        assert!(spec.initial_prompt.contains(".jam/pr-body.md"));
        assert!(spec
            .initial_prompt
            .contains("Jamboree adds [jam] deterministically"));
    }

    #[test]
    fn jamboree_pr_handoff_targets_jamboree_repo_and_base() {
        let config = test_config(false);

        assert_eq!(
            repo_for_project(&config, JAMBOREE_PROJECT),
            Some(DEFAULT_JAMBOREE_GITHUB_REPO.into())
        );
        assert_eq!(
            base_for_project(&config, JAMBOREE_PROJECT),
            DEFAULT_JAMBOREE_TRUNK_BRANCH
        );
        assert_eq!(repo_for_project(&config, DEFAULT_PROJECT), None);
        assert_eq!(
            base_for_project(&config, DEFAULT_PROJECT),
            DEFAULT_TRUNK_BRANCH
        );
    }

    #[test]
    fn reads_picker_pr_metadata_and_prefixes_title() {
        let worktree = TempDir::new().unwrap();
        fs::create_dir_all(worktree.path().join(".jam")).unwrap();
        fs::write(
            pr_title_path(worktree.path()),
            "Improve terrain manifest loading\n",
        )
        .unwrap();
        fs::write(
            pr_body_path(worktree.path()),
            "## Summary\n- Tightens terrain manifest loading.\n\n## Verification\n- cargo test -p blueberry\n",
        )
        .unwrap();

        let metadata = read_picker_pr_metadata(worktree.path(), "task-1").unwrap();

        assert_eq!(metadata.title, "[jam] Improve terrain manifest loading");
        assert!(metadata.body.contains("## Summary"));
        assert!(format_jam_pr_title("[jam] Already prefixed", "task-1").is_ok());
        assert!(format_jam_pr_title("task: task-1", "task-1").is_err());
    }

    #[test]
    fn builds_direct_codex_exec_command() {
        let config = test_config(false);
        let spec = test_spec(false);
        let worktree = TempDir::new().unwrap();
        let parent = TraceCtx::new_root("test", "parent");
        let picker = TraceCtx::child(&parent, "test.child", "picker");

        let command = build_launch_command(
            &config,
            &spec,
            worktree.path(),
            "codex-cli:test",
            &picker,
            &parent,
        )
        .unwrap();
        let args: Vec<_> = command.as_std().get_args().collect();

        assert_eq!(command.as_std().get_program(), Path::new("/bin/codex"));
        assert!(args.contains(&OsString::from("exec").as_os_str()));
        assert!(args.contains(&OsString::from("--cd").as_os_str()));
        assert!(args.contains(&worktree.path().as_os_str()));
        assert!(args
            .contains(&OsString::from("--dangerously-bypass-approvals-and-sandbox").as_os_str()));
        assert!(codex_events_path(worktree.path()).exists());
        assert_eq!(
            command
                .as_std()
                .get_envs()
                .find(|(key, _)| *key == OsStr::new("JAM_TRACE_ID"))
                .unwrap()
                .1,
            Some(OsStr::new(&picker.trace_id.to_string()))
        );
    }

    #[test]
    fn builds_direct_claude_print_command_with_project_mcp_config() {
        let config = test_config(false);
        let mut spec = test_spec(false);
        spec.harness = CLAUDE_HARNESS.into();
        spec.model_override = Some("sonnet".into());
        spec.reasoning_effort = Some("high".into());
        let worktree = TempDir::new().unwrap();
        let parent = TraceCtx::new_root("test", "parent");
        let picker = TraceCtx::child(&parent, "test.child", "picker");

        let command = build_launch_command(
            &config,
            &spec,
            worktree.path(),
            "claude-code:test",
            &picker,
            &parent,
        )
        .unwrap();
        let args: Vec<_> = command.as_std().get_args().map(OsString::from).collect();

        assert_eq!(command.as_std().get_program(), Path::new("/bin/claude"));
        assert!(args.contains(&OsString::from("--print")));
        assert!(args.contains(&OsString::from("--strict-mcp-config")));
        assert!(args.contains(&OsString::from("--dangerously-skip-permissions")));
        assert!(args.contains(&OsString::from("--mcp-config")));
        assert!(args.contains(&claude_mcp_config_path(worktree.path()).into_os_string()));
        assert!(args.contains(&OsString::from("--settings")));
        assert!(args.contains(&claude_settings_path(worktree.path()).into_os_string()));
        assert!(args.contains(&OsString::from("--model")));
        assert!(args.contains(&OsString::from("sonnet")));
        assert!(args.contains(&OsString::from("--effort")));
        assert!(args.contains(&OsString::from("high")));
        assert!(claude_events_path(worktree.path()).exists());
        assert_eq!(
            command_env(&command, "JAM_SESSION_ID"),
            Some(OsStr::new("claude-code:test"))
        );
    }

    #[test]
    fn resume_spec_preserves_codex_harness_from_parent_session() {
        let input = ResumePickerInput {
            task_id: "task-1".into(),
            prompt: "continue and create the PR".into(),
            project: Some(JAMBOREE_PROJECT.into()),
            harness: None,
            parent_session_id: Some("codex-cli:old-session".into()),
            task_class: Some("jamboree-self-modification".into()),
        };

        let spec = SpawnSpec::for_resume(input).unwrap();

        assert_eq!(spec.harness, DEFAULT_HARNESS);
        assert_eq!(spec.project, JAMBOREE_PROJECT);
        assert_eq!(
            spec.parent_session_id.as_deref(),
            Some("codex-cli:old-session")
        );
        assert!(spec.resume_from_last);
    }

    #[test]
    fn resume_spec_preserves_claude_harness_from_parent_session() {
        let input = ResumePickerInput {
            task_id: "task-1".into(),
            prompt: "continue and create the PR".into(),
            project: Some(JAMBOREE_PROJECT.into()),
            harness: None,
            parent_session_id: Some("claude-code:old-session".into()),
            task_class: Some("jamboree-self-modification".into()),
        };

        let spec = SpawnSpec::for_resume(input).unwrap();

        assert_eq!(spec.harness, CLAUDE_HARNESS);
        assert_eq!(spec.project, JAMBOREE_PROJECT);
        assert_eq!(
            spec.parent_session_id.as_deref(),
            Some("claude-code:old-session")
        );
        assert!(spec.resume_from_last);
    }

    #[test]
    fn resume_spec_rejects_harness_without_resume_support() {
        let err = SpawnSpec::for_resume(ResumePickerInput {
            task_id: "task-1".into(),
            prompt: "continue and create the PR".into(),
            project: Some(DEFAULT_PROJECT.into()),
            harness: Some(OPENCODE_HARNESS.into()),
            parent_session_id: Some("opencode-deepseek:old-session".into()),
            task_class: Some(DEFAULT_TASK_CLASS.into()),
        })
        .unwrap_err();

        assert!(err.to_string().contains("unsupported-resume-harness"));
    }

    #[test]
    fn builds_codex_resume_command() {
        let config = test_config(false);
        let spec = SpawnSpec::for_resume(ResumePickerInput {
            task_id: "task-1".into(),
            prompt: "continue and create the PR".into(),
            project: Some(DEFAULT_PROJECT.into()),
            harness: Some(DEFAULT_HARNESS.into()),
            parent_session_id: Some("codex-cli:old-session".into()),
            task_class: Some(DEFAULT_TASK_CLASS.into()),
        })
        .unwrap();
        let worktree = TempDir::new().unwrap();
        let parent = TraceCtx::new_root("test", "parent");
        let picker = TraceCtx::child(&parent, "test.child", "picker");

        let command = build_launch_command(
            &config,
            &spec,
            worktree.path(),
            "codex-cli:new-session",
            &picker,
            &parent,
        )
        .unwrap();
        let args: Vec<_> = command.as_std().get_args().map(OsString::from).collect();

        assert_eq!(command.as_std().get_program(), Path::new("/bin/codex"));
        let exec_pos = args.iter().position(|arg| arg == "exec").unwrap();
        let cd_pos = args.iter().position(|arg| arg == "--cd").unwrap();
        let resume_pos = args.iter().position(|arg| arg == "resume").unwrap();
        assert!(exec_pos < cd_pos);
        assert!(cd_pos < resume_pos);
        assert!(args
            .windows(2)
            .any(|window| window == [OsString::from("resume"), OsString::from("--last")]));
        assert!(args.contains(&OsString::from("--cd")));
        assert!(args.contains(&worktree.path().as_os_str().to_os_string()));
    }

    #[test]
    fn builds_claude_continue_command_for_resume() {
        let config = test_config(false);
        let spec = SpawnSpec::for_resume(ResumePickerInput {
            task_id: "task-1".into(),
            prompt: "PR exists, find it and continue working".into(),
            project: Some(DEFAULT_PROJECT.into()),
            harness: Some(CLAUDE_HARNESS.into()),
            parent_session_id: Some("claude-code:old-session".into()),
            task_class: Some(DEFAULT_TASK_CLASS.into()),
        })
        .unwrap();
        let worktree = TempDir::new().unwrap();
        let parent = TraceCtx::new_root("test", "parent");
        let picker = TraceCtx::child(&parent, "test.child", "picker");

        let command = build_launch_command(
            &config,
            &spec,
            worktree.path(),
            "claude-code:new-session",
            &picker,
            &parent,
        )
        .unwrap();
        let args: Vec<_> = command.as_std().get_args().map(OsString::from).collect();

        assert_eq!(command.as_std().get_program(), Path::new("/bin/claude"));
        assert!(args.contains(&OsString::from("--print")));
        assert!(args.contains(&OsString::from("--continue")));
        assert!(args.contains(&OsString::from("--add-dir")));
        assert!(args.contains(&worktree.path().as_os_str().to_os_string()));
        assert!(claude_events_path(worktree.path()).exists());
        assert_eq!(
            command_env(&command, "JAM_SESSION_ID"),
            Some(OsStr::new("claude-code:new-session"))
        );
    }

    #[test]
    fn builds_sudo_claude_continue_command_from_worktree() {
        let config = test_config(true);
        let spec = SpawnSpec::for_resume(ResumePickerInput {
            task_id: "task-1".into(),
            prompt: "PR exists, find it and continue working".into(),
            project: Some(DEFAULT_PROJECT.into()),
            harness: Some(CLAUDE_HARNESS.into()),
            parent_session_id: Some("claude-code:old-session".into()),
            task_class: Some(DEFAULT_TASK_CLASS.into()),
        })
        .unwrap();
        let worktree = TempDir::new().unwrap();
        let parent = TraceCtx::new_root("test", "parent");
        let picker = TraceCtx::child(&parent, "test.child", "picker");

        let command = build_launch_command(
            &config,
            &spec,
            worktree.path(),
            "claude-code:new-session",
            &picker,
            &parent,
        )
        .unwrap();
        let std_command = command.as_std();
        let args: Vec<_> = std_command.get_args().map(OsString::from).collect();

        assert_eq!(std_command.get_program(), Path::new("/usr/bin/sudo"));
        assert_eq!(std_command.get_current_dir(), Some(Path::new("/")));
        assert_eq!(args[5], OsString::from(DEFAULT_SHELL_BIN));
        assert_eq!(args[6], OsString::from("-lc"));
        assert_eq!(args[7], OsString::from("cd \"$1\" && shift && exec \"$@\""));
        assert_eq!(args[8], OsString::from("sh"));
        assert_eq!(args[9], worktree.path().as_os_str().to_os_string());
        assert_eq!(args[10], OsString::from("/bin/claude"));
        assert!(args.contains(&OsString::from("--continue")));
        assert!(args.contains(&OsString::from("--add-dir")));
        assert!(args.contains(&worktree.path().as_os_str().to_os_string()));
    }

    #[test]
    fn builds_direct_opencode_runner_command_with_deepseek_env() {
        let worktree = TempDir::new().unwrap();
        let secrets = worktree.path().join("secrets.toml");
        fs::write(
            &secrets,
            r#"[secrets]
"jam/pickers/deepseek-api-key" = "deepseek-test-key"
"#,
        )
        .unwrap();
        let mut config = test_config(false);
        config.secrets_file = Some(secrets);
        let mut spec = test_spec(false);
        spec.harness = OPENCODE_HARNESS.into();
        spec.model_override = Some("deepseek-v4-flash".into());
        spec.reasoning_effort = Some("high".into());
        let parent = TraceCtx::new_root("test", "parent");
        let picker = TraceCtx::child(&parent, "test.child", "picker");

        let command = build_launch_command(
            &config,
            &spec,
            worktree.path(),
            "opencode-deepseek:test",
            &picker,
            &parent,
        )
        .unwrap();
        let args: Vec<_> = command.as_std().get_args().map(OsString::from).collect();

        assert_eq!(
            command.as_std().get_program(),
            opencode_runner_path(worktree.path()).as_os_str()
        );
        assert_eq!(args[0], OsString::from("/bin/opencode"));
        assert_eq!(
            args[1],
            opencode_prompt_path(worktree.path()).into_os_string()
        );
        assert_eq!(args[2], worktree.path().as_os_str());
        assert_eq!(args[3], OsString::from("deepseek/deepseek-v4-flash"));
        assert_eq!(args[4], OsString::from("high"));
        assert_eq!(
            command_env(&command, DEEPSEEK_SECRET_ENV),
            Some(OsStr::new("deepseek-test-key"))
        );
        assert_eq!(
            command_env(&command, "OPENCODE_CONFIG"),
            Some(opencode_config_path(worktree.path()).as_os_str())
        );
    }

    #[test]
    fn prepares_claude_hooks_and_mcp_config_without_clobbering_existing_settings() {
        let worktree = TempDir::new().unwrap();
        let settings_path = claude_settings_path(worktree.path());
        fs::create_dir_all(settings_path.parent().unwrap()).unwrap();
        fs::write(
            &settings_path,
            r#"{
  "hooks": {
    "SessionStart": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "python scripts/dev/bootstrap_build_cache.py"
          }
        ]
      }
    ],
    "PostToolUse": [
      {
        "matcher": "Edit|Write",
        "hooks": [
          {
            "type": "command",
            "command": "tempyr validate --json"
          }
        ]
      }
    ]
  }
}"#,
        )
        .unwrap();

        let project_config = worktree.path().join("blueberry.toml");
        fs::write(
            &project_config,
            r#"[mcp-servers]
tempyr = { url = "stdio:tempyr --mcp", enabled = true }
context7 = { url = "https://mcp.context7.com/mcp/v1", enabled = true }
disabled = { url = "stdio:warpgrep", enabled = false }
"#,
        )
        .unwrap();
        let mut config = test_config(false);
        config.project_config_path = project_config;
        let mut spec = test_spec(false);
        spec.harness = CLAUDE_HARNESS.into();

        prepare_harness_worktree(&config, &spec, worktree.path(), "claude-code:test").unwrap();

        let settings: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&settings_path).unwrap()).unwrap();
        let session_start = settings["hooks"]["SessionStart"].as_array().unwrap();
        assert!(hook_entries_contain_command(
            session_start,
            "python scripts/dev/bootstrap_build_cache.py"
        ));
        assert!(hook_entries_contain_command(
            session_start,
            TEMPYR_BOOTSTRAP_COMMAND
        ));
        let session_end = settings["hooks"]["SessionEnd"].as_array().unwrap();
        assert!(hook_entries_contain_command(
            session_end,
            TEMPYR_CLAUDE_FINALIZE_COMMAND
        ));
        assert!(settings["hooks"]["PostToolUse"].is_array());

        let mcp: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(claude_mcp_config_path(worktree.path())).unwrap(),
        )
        .unwrap();
        assert_eq!(
            mcp["mcpServers"]["tempyr"],
            serde_json::json!({"command": "tempyr", "args": ["--mcp"]})
        );
        assert_eq!(
            mcp["mcpServers"]["context7"],
            serde_json::json!({"type": "http", "url": "https://mcp.context7.com/mcp/v1"})
        );
        assert!(mcp["mcpServers"].get("disabled").is_none());
    }

    #[test]
    fn prepares_opencode_runner_prompt_and_config() {
        let worktree = TempDir::new().unwrap();
        let project_config = worktree.path().join("blueberry.toml");
        fs::write(
            &project_config,
            r#"[mcp-servers]
tempyr = { url = "stdio:tempyr --mcp", enabled = true }
context7 = { url = "https://mcp.context7.com/mcp/v1", enabled = true }
disabled = { url = "stdio:warpgrep", enabled = false }
"#,
        )
        .unwrap();
        let mut config = test_config(false);
        config.project_config_path = project_config;
        let mut spec = test_spec(false);
        spec.harness = OPENCODE_HARNESS.into();
        spec.initial_prompt = "do opencode work\nwith context".into();

        prepare_harness_worktree(&config, &spec, worktree.path(), "opencode-deepseek:test")
            .unwrap();

        assert_eq!(
            fs::read_to_string(opencode_prompt_path(worktree.path())).unwrap(),
            "do opencode work\nwith context"
        );
        let runner = fs::read_to_string(opencode_runner_path(worktree.path())).unwrap();
        assert!(runner.contains("tempyr journal bootstrap --quiet"));
        assert!(runner.contains("tempyr journal log --agent opencode plan"));
        assert!(runner.contains("tempyr journal finalize --agent opencode --quiet"));
        assert!(runner.contains("opencode-events.jsonl"));
        assert!(runner.contains("| tee \"$events_path\""));

        let config_json: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(opencode_config_path(worktree.path())).unwrap(),
        )
        .unwrap();
        assert_eq!(config_json["model"], DEFAULT_OPENCODE_MODEL);
        assert_eq!(
            config_json["enabled_providers"],
            serde_json::json!(["deepseek"])
        );
        assert_eq!(
            config_json["provider"]["deepseek"]["options"]["apiKey"],
            "{env:DEEPSEEK_API_KEY}"
        );
        assert_eq!(
            config_json["mcp"]["tempyr"],
            serde_json::json!({"type": "local", "command": ["tempyr", "--mcp"], "enabled": true})
        );
        assert_eq!(
            config_json["mcp"]["context7"],
            serde_json::json!({"type": "remote", "url": "https://mcp.context7.com/mcp/v1", "enabled": true})
        );
        assert!(config_json["mcp"].get("disabled").is_none());
    }

    #[test]
    fn parses_opencode_usage_jsonl_for_quota_events() {
        let raw = r#"
{"type":"message","provider":"deepseek","model":"deepseek-v4-pro","usage":{"input_tokens":1000,"output_tokens":250,"cost_usd":0.5}}
{"type":"message","usage":{"prompt_tokens":10,"completion_tokens":5,"cost":0.01}}
{"type":"log","message":"ignored"}
"#;

        let usage = parse_usage_jsonl(raw, "opencode-json").unwrap();

        assert_eq!(usage.provider.as_deref(), Some("deepseek"));
        assert_eq!(usage.model.as_deref(), Some("deepseek-v4-pro"));
        assert_eq!(usage.input_tokens, 1010);
        assert_eq!(usage.output_tokens, 255);
        assert!((usage.cost_usd.unwrap() - 0.51).abs() < f64::EPSILON);
        assert_eq!(usage.source, "opencode-json");
    }

    #[test]
    fn reads_codex_and_claude_usage_logs_for_quota_events() {
        let codex_worktree = TempDir::new().unwrap();
        fs::create_dir_all(codex_events_path(codex_worktree.path()).parent().unwrap()).unwrap();
        fs::write(
            codex_events_path(codex_worktree.path()),
            r#"{"provider":"openai","model":"gpt-5.4","usage":{"input_tokens":20,"output_tokens":5,"cost_usd":0.02}}"#,
        )
        .unwrap();
        let codex = test_handle(DEFAULT_HARNESS, codex_worktree.path(), false);

        let codex_usage = quota_usage_for_handle(&codex, Utc::now()).unwrap();

        assert_eq!(codex_usage.provider.as_deref(), Some("openai"));
        assert_eq!(codex_usage.model.as_deref(), Some("gpt-5.4"));
        assert_eq!(codex_usage.input_tokens, 20);
        assert_eq!(codex_usage.output_tokens, 5);
        assert!((codex_usage.cost_usd.unwrap() - 0.02).abs() < f64::EPSILON);
        assert_eq!(codex_usage.source, "codex-json");

        let claude_worktree = TempDir::new().unwrap();
        fs::create_dir_all(claude_events_path(claude_worktree.path()).parent().unwrap()).unwrap();
        fs::write(
            claude_events_path(claude_worktree.path()),
            r#"
{"type":"assistant","message":{"model":"claude-opus-4-7","usage":{"input_tokens":6,"output_tokens":1}}}
{"type":"result","total_cost_usd":0.18687375,"usage":{"input_tokens":6,"cache_creation_input_tokens":29871,"output_tokens":6},"modelUsage":{"claude-opus-4-7[1m]":{"inputTokens":6,"outputTokens":6,"costUSD":0.18687375}}}
"#,
        )
        .unwrap();
        let claude = test_handle(CLAUDE_HARNESS, claude_worktree.path(), false);

        let claude_usage = quota_usage_for_handle(&claude, Utc::now()).unwrap();

        assert_eq!(claude_usage.model.as_deref(), Some("claude-opus-4-7[1m]"));
        assert_eq!(claude_usage.input_tokens, 6);
        assert_eq!(claude_usage.output_tokens, 6);
        assert!((claude_usage.cost_usd.unwrap() - 0.186_873_75).abs() < f64::EPSILON);
        assert_eq!(claude_usage.source, "claude-stream-json");
    }

    #[test]
    fn claude_mcp_config_rejects_enabled_auth_servers_until_secret_injection_lands() {
        let worktree = TempDir::new().unwrap();
        let project_config = worktree.path().join("blueberry.toml");
        fs::write(
            &project_config,
            r#"[mcp-servers]
github-mcp = { url = "https://api.githubcopilot.com/mcp/", enabled = true, auth = "github-pat" }
"#,
        )
        .unwrap();
        let mut config = test_config(false);
        config.project_config_path = project_config;

        let err = write_claude_mcp_config(&config, worktree.path()).unwrap_err();

        assert!(err.to_string().contains("mcp-auth-not-implemented"));
    }

    #[test]
    fn dry_run_non_codex_uses_probe_command_not_harness_binary() {
        let config = test_config(false);
        let mut spec = test_spec(true);
        spec.harness = "opencode-deepseek".into();
        let worktree = PathBuf::from("/tmp/task-1");
        let parent = TraceCtx::new_root("test", "parent");
        let picker = TraceCtx::child(&parent, "test.child", "picker");

        let command = build_launch_command(
            &config,
            &spec,
            &worktree,
            "opencode-deepseek:test",
            &picker,
            &parent,
        )
        .unwrap();
        let args: Vec<_> = command.as_std().get_args().collect();

        assert_eq!(command.as_std().get_program(), Path::new("sleep"));
        assert_eq!(args, vec![OsStr::new("1")]);
        assert_eq!(
            command_env(&command, "JAM_SESSION_ID"),
            Some(OsStr::new("opencode-deepseek:test"))
        );
    }

    #[test]
    fn builds_docker_dry_run_command_with_worktree_and_repo_mounts() {
        let worktree = TempDir::new().unwrap();
        git_init(worktree.path());
        let mut config = test_config(false);
        config.docker_bin = PathBuf::from("/usr/bin/docker");
        config.docker_image = "alpine:3.20".into();
        config.dry_run_command = vec!["/work/.jam/docker-picker.sh".into()];
        let mut spec = test_spec(true);
        spec.sandbox_backend = DOCKER_SANDBOX_BACKEND.into();
        spec.sandbox_profile = HARDENED_SANDBOX_PROFILE.into();
        let parent = TraceCtx::new_root("test", "parent");
        let picker = TraceCtx::child(&parent, "test.child", "picker");

        let command = build_launch_command(
            &config,
            &spec,
            worktree.path(),
            "codex-cli:test",
            &picker,
            &parent,
        )
        .unwrap();
        let std_command = command.as_std();
        let args: Vec<_> = std_command.get_args().map(OsString::from).collect();
        let worktree_mount = format!("{}:/work:rw", worktree.path().to_string_lossy());
        let repo_mount = format!(
            "{}:/repo.git:ro",
            worktree
                .path()
                .join(".git")
                .canonicalize()
                .unwrap()
                .display()
        );

        assert_eq!(std_command.get_program(), Path::new("/usr/bin/docker"));
        assert!(args.contains(&OsString::from("run")));
        assert!(args.contains(&OsString::from("--read-only")));
        assert!(arg_pair_exists(&args, "--network", "none"));
        assert!(arg_pair_exists(
            &args,
            "--label",
            "org.jamboree.session=codex-cli:test"
        ));
        assert!(arg_pair_exists(
            &args,
            "--label",
            "org.jamboree.task=task-1"
        ));
        assert!(arg_pair_exists(&args, "--workdir", DOCKER_WORKTREE_PATH));
        assert!(arg_pair_exists(&args, "--volume", &worktree_mount));
        assert!(arg_pair_exists(&args, "--volume", &repo_mount));
        assert!(arg_pair_exists(&args, "--env", "JAM_TRACE_ID"));
        assert!(args.contains(&OsString::from("alpine:3.20")));
        assert!(args.contains(&OsString::from("/work/.jam/docker-picker.sh")));
        assert_eq!(
            command_env(&command, "JAM_SESSION_ID"),
            Some(OsStr::new("codex-cli:test"))
        );
    }

    #[test]
    fn wraps_local_compile_heavy_spawn_in_systemd_scope() {
        let mut config = test_config(false);
        config.use_systemd_scope = true;
        let mut spec = test_spec(true);
        spec.task_class = "compile-heavy-rust".into();
        let worktree = TempDir::new().unwrap();
        let parent = TraceCtx::new_root("test", "parent");
        let picker = TraceCtx::child(&parent, "test.child", "picker");

        let command = build_launch_command(
            &config,
            &spec,
            worktree.path(),
            "codex-cli:test",
            &picker,
            &parent,
        )
        .unwrap();
        let args: Vec<_> = command.as_std().get_args().map(OsString::from).collect();

        assert_eq!(
            command.as_std().get_program(),
            Path::new("/usr/bin/systemd-run")
        );
        assert!(args.contains(&OsString::from("--scope")));
        assert!(args.contains(&OsString::from("--property=CPUQuota=800%")));
        assert!(args.contains(&OsString::from("--property=MemoryMax=8G")));
        assert!(args.contains(&OsString::from("sleep")));
        assert_eq!(
            resource_scope_for_spec(&config, &spec, "codex-cli:test").as_deref(),
            Some("jam-picker-codex-cli-test.scope")
        );
    }

    #[test]
    fn risky_architecture_scope_uses_idle_ionice() {
        let mut config = test_config(false);
        config.use_systemd_scope = true;
        let mut spec = test_spec(true);
        spec.task_class = "risky-architecture".into();
        let worktree = TempDir::new().unwrap();
        let parent = TraceCtx::new_root("test", "parent");
        let picker = TraceCtx::child(&parent, "test.child", "picker");

        let command = build_launch_command(
            &config,
            &spec,
            worktree.path(),
            "codex-cli:test",
            &picker,
            &parent,
        )
        .unwrap();
        let args: Vec<_> = command.as_std().get_args().map(OsString::from).collect();

        assert!(args.contains(&OsString::from("--property=CPUQuota=100%")));
        assert!(args.contains(&OsString::from("--property=IOWeight=10")));
        assert!(args.contains(&OsString::from("/usr/bin/ionice")));
        assert!(arg_pair_exists(&args, "-c", "3"));
    }

    #[test]
    fn builds_sudo_wrapped_command() {
        let config = test_config(true);
        let spec = test_spec(false);
        let worktree = TempDir::new().unwrap();
        let parent = TraceCtx::new_root("test", "parent");
        let picker = TraceCtx::child(&parent, "test.child", "picker");

        let command = build_launch_command(
            &config,
            &spec,
            worktree.path(),
            "codex-cli:test",
            &picker,
            &parent,
        )
        .unwrap();
        let std_command = command.as_std();
        let args: Vec<_> = std_command.get_args().map(OsString::from).collect();

        assert_eq!(std_command.get_program(), Path::new("/usr/bin/sudo"));
        assert_eq!(std_command.get_current_dir(), Some(Path::new("/")));
        assert_eq!(args[0], OsString::from("-n"));
        assert_eq!(args[1], OsString::from("-u"));
        assert_eq!(args[2], OsString::from("picker"));
        assert_eq!(args[4], OsString::from("--"));
        assert_eq!(args[5], OsString::from("/bin/codex"));
        assert!(args.contains(&OsString::from("--ask-for-approval")));

        let preserve = args[3].to_string_lossy();
        let mut preserve_keys: Vec<_> = preserve
            .strip_prefix("--preserve-env=")
            .unwrap()
            .split(',')
            .map(ToOwned::to_owned)
            .collect();
        let mut env_keys: Vec<_> = std_command
            .get_envs()
            .filter_map(|(key, value)| value.map(|_| key.to_string_lossy().into_owned()))
            .collect();
        preserve_keys.sort_unstable();
        env_keys.sort_unstable();
        assert_eq!(preserve_keys, env_keys);

        assert_eq!(
            command_env(&command, "HOME"),
            Some(OsStr::new("/home/picker"))
        );
        assert_eq!(
            command_env(&command, "CODEX_HOME"),
            Some(OsStr::new("/home/maestro/.codex"))
        );
        assert_eq!(
            command_env(&command, "JAM_TRACE_ID"),
            Some(OsStr::new(&picker.trace_id.to_string()))
        );
    }

    #[test]
    fn parses_harness_version_output() {
        assert_eq!(
            parse_harness_version("codex-cli 0.128.0\n"),
            Some("0.128.0".into())
        );
        assert_eq!(
            parse_harness_version("2.1.128 (Claude Code)\n"),
            Some("2.1.128".into())
        );
        assert_eq!(parse_harness_version("0.128.0\n"), Some("0.128.0".into()));
        assert_eq!(parse_harness_version(""), None);
    }

    #[cfg(unix)]
    #[test]
    fn harness_lockfile_accepts_matching_pin() {
        let fixture = HarnessLockfileFixture::new(CLAUDE_HARNESS, "2.1.128 (Claude Code)");
        fixture.write_lockfile("2.1.128", &fixture.checksum);

        verify_harness_lockfile(&fixture.harness, &fixture.harness_bin, &fixture.lockfile).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn harness_lockfile_rejects_version_drift() {
        let fixture = HarnessLockfileFixture::new(DEFAULT_HARNESS, "codex-cli 0.128.0");
        fixture.write_lockfile("0.127.0", &fixture.checksum);

        let err =
            verify_harness_lockfile(&fixture.harness, &fixture.harness_bin, &fixture.lockfile)
                .unwrap_err();

        assert!(err.to_string().contains("harness-version-drift"));
    }

    #[cfg(unix)]
    #[test]
    fn warn_policy_allows_harness_drift() {
        let fixture = HarnessLockfileFixture::new(DEFAULT_HARNESS, "codex-cli 0.128.0");
        fixture.write_lockfile("0.127.0", &fixture.checksum);

        let err =
            verify_harness_lockfile(&fixture.harness, &fixture.harness_bin, &fixture.lockfile)
                .unwrap_err();

        assert!(!harness_lockfile_error_blocks(
            HarnessLockfilePolicy::Warn,
            &err,
        ));
        assert!(harness_lockfile_error_blocks(
            HarnessLockfilePolicy::Strict,
            &err,
        ));
    }

    #[test]
    fn validates_native_worktree_paths() {
        let tmp = TempDir::new().unwrap();
        let path = validate_worktree_path(tmp.path().to_str().unwrap()).unwrap();
        assert!(path.is_absolute());
        assert!(validate_worktree_path("/mnt/c/project").is_err());
        assert!(validate_worktree_path("/cygdrive/c/project").is_err());
    }

    #[test]
    fn repairs_missing_linked_worktree_admin_dir_before_resume() {
        let tmp = TempDir::new().unwrap();
        let repo = tmp.path().join("repo");
        let worktree = tmp.path().join("task-1");
        fs::create_dir(&repo).unwrap();
        git_ok(&repo, &["init", "-q"]);
        git_ok(&repo, &["config", "user.email", "test@example.com"]);
        git_ok(&repo, &["config", "user.name", "Test User"]);
        fs::write(repo.join("a.txt"), "a\n").unwrap();
        git_ok(&repo, &["add", "a.txt"]);
        git_ok(&repo, &["commit", "-q", "-m", "init"]);
        git_ok(
            &repo,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "task/task-1",
                worktree.to_str().unwrap(),
                "HEAD",
            ],
        );
        fs::write(worktree.join("a.txt"), "a\nchanged\n").unwrap();

        let admin_dir = read_gitdir_pointer(&worktree.join(".git"), &worktree).unwrap();
        fs::remove_dir_all(&admin_dir).unwrap();

        let repaired = git_dir_for_worktree(&worktree, "task-1").unwrap();
        let repaired_pointer = read_gitdir_pointer(&worktree.join(".git"), &worktree).unwrap();
        let status = git_stdout(&worktree, &["status", "--short"]);

        assert!(repaired.exists());
        assert_eq!(repaired_pointer, admin_dir);
        assert!(status.contains(" M a.txt"), "{status}");
    }

    #[test]
    fn default_picker_path_includes_tempyr_install_dir() {
        let path = default_picker_path();
        let path = path.to_string_lossy();

        assert!(path.contains("/home/caleb/.local/share/tempyr/bin"));
        assert!(path.contains("/home/picker/.local/share/tempyr/bin"));
        assert!(path.contains("/home/picker/.cargo/bin"));
    }

    #[test]
    fn picker_duration_ms_never_goes_negative() {
        let spawned = DateTime::parse_from_rfc3339("2026-05-06T04:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let exited = DateTime::parse_from_rfc3339("2026-05-06T04:00:01.250Z")
            .unwrap()
            .with_timezone(&Utc);

        assert_eq!(picker_duration_ms(spawned, exited), 1_250);
        assert_eq!(picker_duration_ms(exited, spawned), 0);
    }

    #[test]
    fn full_stop_accepts_codex_session_handles() {
        let input = FullStopInput {
            session_id: "codex-cli:01KQXZ66Q1EDRDG1RM8VY7XKWG".into(),
            reason: "stalled".into(),
            requested_by: None,
        };

        validate_full_stop_input(&input).unwrap();
    }

    #[test]
    fn picker_message_subjects_match_session_scoped_bus_contract() {
        assert_eq!(
            picker_message_subject(
                "codex-cli:01KQXZ66Q1EDRDG1RM8VY7XKWG",
                PickerMessageMode::Queue
            ),
            "picker.codex-cli:01KQXZ66Q1EDRDG1RM8VY7XKWG.msg.queue"
        );
        assert_eq!(
            picker_message_subject(
                "codex-cli:01KQXZ66Q1EDRDG1RM8VY7XKWG",
                PickerMessageMode::Interrupt
            ),
            "picker.codex-cli:01KQXZ66Q1EDRDG1RM8VY7XKWG.msg.interrupt"
        );
        assert_eq!(
            picker_status_subject("codex-cli:01KQXZ66Q1EDRDG1RM8VY7XKWG"),
            "picker.codex-cli:01KQXZ66Q1EDRDG1RM8VY7XKWG.msg.status"
        );
    }

    #[test]
    fn picker_message_validation_requires_matching_session_mode_and_text() {
        let payload = PickerMessagePayload {
            message_id: "msg:01KQXZ66Q1EDRDG1RM8VY7XKWG".into(),
            session_id: "codex-cli:session".into(),
            mode: PickerMessageMode::Queue,
            text: Some("check docs/proposal-v5.md".into()),
            from: "human:caleb".into(),
        };

        validate_picker_message_payload(&payload, "codex-cli:session", PickerMessageMode::Queue)
            .unwrap();
        assert!(validate_picker_message_payload(
            &payload,
            "codex-cli:other",
            PickerMessageMode::Queue,
        )
        .is_err());
        assert!(validate_picker_message_payload(
            &payload,
            "codex-cli:session",
            PickerMessageMode::Interrupt,
        )
        .is_err());
    }

    #[test]
    fn picker_message_frame_preserves_text_with_metadata() {
        let payload = PickerMessagePayload {
            message_id: "msg:01KQXZ66Q1EDRDG1RM8VY7XKWG".into(),
            session_id: "codex-cli:session".into(),
            mode: PickerMessageMode::Interrupt,
            text: Some("please switch to the smaller test first".into()),
            from: "human:caleb".into(),
        };

        let frame = picker_message_stdin_frame(&payload);

        assert!(frame.contains(r#"<jamboree-message mode="interrupt""#));
        assert!(frame.contains(r#"from="human:caleb""#));
        assert!(frame.contains("please switch to the smaller test first"));
        assert!(frame.ends_with("</jamboree-message>\n"));
    }

    #[test]
    fn killed_marker_uses_stable_utc_filename() {
        let killed_at = DateTime::parse_from_rfc3339("2026-05-06T04:00:01Z")
            .unwrap()
            .with_timezone(&Utc);

        assert_eq!(
            killed_marker_path("/tmp/task-1", killed_at),
            PathBuf::from("/tmp/task-1/.killed-at-20260506T040001Z")
        );
    }

    #[tokio::test]
    async fn archive_and_purge_require_completed_sessions() {
        let tmp = TempDir::new().unwrap();
        let worktree = tmp.path().join("worktree");
        std::fs::create_dir(&worktree).unwrap();
        let state = SessionState {
            config: test_config(false),
            active: Arc::new(Mutex::new(HashMap::new())),
            routing: jam_nats::RoutingResolver::disconnected(),
        };
        state.active.lock().await.insert(
            "codex-cli:running".into(),
            test_picker_record("codex-cli:running", &worktree, PickerStatus::Running),
        );

        let err = remove_completed_session(&state, "codex-cli:running", "archive-session", "test")
            .await
            .unwrap_err();

        assert!(err.to_string().contains("session-still-running"));

        state.active.lock().await.insert(
            "codex-cli:exited".into(),
            test_picker_record("codex-cli:exited", &worktree, PickerStatus::Exited),
        );

        let record =
            remove_completed_session(&state, "codex-cli:exited", "archive-session", "test")
                .await
                .unwrap();

        assert_eq!(record.status, PickerStatus::Exited);
        assert!(!state.active.lock().await.contains_key("codex-cli:exited"));
    }

    #[test]
    fn purge_removes_worktree_directory() {
        let tmp = TempDir::new().unwrap();
        let worktree = tmp.path().join("worktree");
        std::fs::create_dir(&worktree).unwrap();

        let removed = remove_worktree_dir(worktree.to_str().unwrap()).unwrap();

        assert!(removed);
        assert!(!worktree.exists());
    }

    #[test]
    fn resolves_relative_git_dir_under_worktree() {
        let tmp = TempDir::new().unwrap();
        let git_dir = tmp.path().join(".git");
        std::fs::create_dir(&git_dir).unwrap();

        let resolved = resolve_git_metadata_path(tmp.path(), ".git").unwrap();

        assert_eq!(resolved, git_dir.canonicalize().unwrap());
    }

    fn test_picker_record(session_id: &str, worktree: &Path, status: PickerStatus) -> PickerRecord {
        let spawned_at = DateTime::parse_from_rfc3339("2026-05-06T04:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        PickerRecord {
            handle: PickerHandle {
                session_id: session_id.into(),
                task_id: "task-1".into(),
                project: DEFAULT_PROJECT.into(),
                harness: DEFAULT_HARNESS.into(),
                worktree_path: worktree.to_string_lossy().into_owned(),
                picker_trace_id: TraceCtx::new_root("test", "picker").trace_id.to_string(),
                maestro_trace_id: TraceCtx::new_root("test", "maestro").trace_id.to_string(),
                sandbox_backend: DEFAULT_SANDBOX_BACKEND.into(),
                sandbox_profile: DEFAULT_SANDBOX_PROFILE.into(),
                task_class: DEFAULT_TASK_CLASS.into(),
                picker_pid: None,
                resource_scope: None,
                spawned_at,
                dry_run: true,
                parent_session_id: None,
            },
            status,
            exited_at: matches!(status, PickerStatus::Exited | PickerStatus::Killed)
                .then_some(spawned_at),
            exit_code: (status == PickerStatus::Exited).then_some(0),
        }
    }

    fn test_config(use_sudo: bool) -> SessionConfig {
        SessionConfig {
            worktree_subject: DEFAULT_WORKTREE_SUBJECT.into(),
            repo_open_pr_subject: DEFAULT_REPO_OPEN_PR_SUBJECT.into(),
            lockfile_path: PathBuf::from("/tmp/lock"),
            harness_lockfile_policy: HarnessLockfilePolicy::Warn,
            git_bin: PathBuf::from("/usr/bin/git"),
            codex_bin: PathBuf::from("/bin/codex"),
            claude_bin: PathBuf::from("/bin/claude"),
            opencode_bin: PathBuf::from("/bin/opencode"),
            docker_bin: PathBuf::from("/usr/bin/docker"),
            docker_image: DEFAULT_DOCKER_IMAGE.into(),
            systemd_run_bin: PathBuf::from("/usr/bin/systemd-run"),
            ionice_bin: PathBuf::from("/usr/bin/ionice"),
            opencode_model: DEFAULT_OPENCODE_MODEL.into(),
            opencode_small_model: DEFAULT_OPENCODE_SMALL_MODEL.into(),
            project_config_path: PathBuf::from("/tmp/blueberry.toml"),
            session_log_root: PathBuf::from("/tmp/session-logs"),
            secrets_file: None,
            picker_home: PathBuf::from("/home/picker"),
            codex_home: PathBuf::from("/home/maestro/.codex"),
            picker_path: default_picker_path(),
            sudo_bin: PathBuf::from("/usr/bin/sudo"),
            use_sudo,
            use_systemd_scope: false,
            request_timeout: Duration::from_secs(1),
            kill_grace: Duration::from_millis(10),
            dry_run_command: vec!["sleep".into(), "1".into()],
            open_pr_on_success: true,
            pr_draft: true,
            trunk_branch: DEFAULT_TRUNK_BRANCH.into(),
            jamboree_github_repo: DEFAULT_JAMBOREE_GITHUB_REPO.into(),
            jamboree_trunk_branch: DEFAULT_JAMBOREE_TRUNK_BRANCH.into(),
        }
    }

    fn test_spec(dry_run: bool) -> SpawnSpec {
        SpawnSpec {
            task_id: "task-1".into(),
            project: DEFAULT_PROJECT.into(),
            harness: DEFAULT_HARNESS.into(),
            sandbox_backend: DEFAULT_SANDBOX_BACKEND.into(),
            sandbox_profile: DEFAULT_SANDBOX_PROFILE.into(),
            task_class: DEFAULT_TASK_CLASS.into(),
            initial_prompt: "do work".into(),
            model_override: None,
            reasoning_effort: None,
            budget_usd: Some(1.0),
            dry_run,
            resume_from_last: false,
            parent_session_id: None,
        }
    }

    fn command_env<'a>(command: &'a Command, key: &str) -> Option<&'a OsStr> {
        command
            .as_std()
            .get_envs()
            .find(|(env_key, _)| *env_key == OsStr::new(key))
            .and_then(|(_, value)| value)
    }

    fn arg_pair_exists(args: &[OsString], left: &str, right: &str) -> bool {
        args.windows(2)
            .any(|pair| pair[0] == OsStr::new(left) && pair[1] == OsStr::new(right))
    }

    fn git_init(path: &Path) {
        let output = StdCommand::new("git")
            .arg("init")
            .arg(path)
            .output()
            .unwrap();
        assert!(output.status.success());
    }

    fn git_ok(path: &Path, args: &[&str]) {
        let output = StdCommand::new("git")
            .arg("-C")
            .arg(path)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git -C {} {} failed: {}",
            path.display(),
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn git_stdout(path: &Path, args: &[&str]) -> String {
        let output = StdCommand::new("git")
            .arg("-C")
            .arg(path)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git -C {} {} failed: {}",
            path.display(),
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    fn test_handle(harness: &str, worktree_path: &Path, dry_run: bool) -> PickerHandle {
        PickerHandle {
            session_id: format!("{harness}:test"),
            task_id: "task-1".into(),
            project: DEFAULT_PROJECT.into(),
            harness: harness.into(),
            worktree_path: worktree_path.to_string_lossy().into_owned(),
            picker_trace_id: "picker-trace".into(),
            maestro_trace_id: "maestro-trace".into(),
            sandbox_backend: DEFAULT_SANDBOX_BACKEND.into(),
            sandbox_profile: DEFAULT_SANDBOX_PROFILE.into(),
            task_class: DEFAULT_TASK_CLASS.into(),
            picker_pid: None,
            resource_scope: None,
            spawned_at: Utc::now(),
            dry_run,
            parent_session_id: None,
        }
    }

    #[cfg(unix)]
    struct HarnessLockfileFixture {
        _tmp: TempDir,
        harness: String,
        harness_bin: PathBuf,
        lockfile: PathBuf,
        checksum: String,
    }

    #[cfg(unix)]
    impl HarnessLockfileFixture {
        fn new(harness: &str, version_output: &str) -> Self {
            use std::os::unix::fs::PermissionsExt;

            let tmp = TempDir::new().unwrap();
            let harness_bin = tmp.path().join("harness");
            fs::write(
                &harness_bin,
                format!("#!/bin/sh\nprintf '{version_output}\\n'\n"),
            )
            .unwrap();
            let mut permissions = fs::metadata(&harness_bin).unwrap().permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&harness_bin, permissions).unwrap();
            let checksum = sha256_file(harness, &harness_bin).unwrap();
            let lockfile = tmp.path().join("blueberry-harnesses.lock");
            Self {
                _tmp: tmp,
                harness: harness.into(),
                harness_bin,
                lockfile,
                checksum,
            }
        }

        fn write_lockfile(&self, version: &str, checksum: &str) {
            fs::write(
                &self.lockfile,
                format!(
                    r#"[harnesses.{}]
version = "{version}"
checksum-sha256 = "{checksum}"
"#,
                    self.harness
                ),
            )
            .unwrap();
        }
    }

    #[test]
    fn detects_rate_limit_in_stderr_log() {
        let tmp = TempDir::new().unwrap();
        let log_path = tmp.path().join("test-session.jsonl");
        let mut file = std::fs::File::create(&log_path).unwrap();
        use std::io::Write;
        writeln!(
            file,
            r#"{{"stream":"stdout","line":"Starting task...","session_id":"s1","task_id":"t1","trace_id":"tr1","ts":"2026-05-06T10:00:00Z","sequence":0,"truncated":false}}"#
        )
        .unwrap();
        writeln!(
            file,
            r#"{{"stream":"stderr","line":"Error: rate limit exceeded, try again later","session_id":"s1","task_id":"t1","trace_id":"tr1","ts":"2026-05-06T10:00:01Z","sequence":1,"truncated":false}}"#
        )
        .unwrap();

        assert!(detect_quota_exhaustion_in_log(&log_path, "codex-cli"));
    }

    #[test]
    fn no_false_positive_on_normal_stderr() {
        let tmp = TempDir::new().unwrap();
        let log_path = tmp.path().join("test-session.jsonl");
        let mut file = std::fs::File::create(&log_path).unwrap();
        use std::io::Write;
        writeln!(
            file,
            r#"{{"stream":"stderr","line":"warning: unused variable `x`","session_id":"s1","task_id":"t1","trace_id":"tr1","ts":"2026-05-06T10:00:00Z","sequence":0,"truncated":false}}"#
        )
        .unwrap();
        writeln!(
            file,
            r#"{{"stream":"stderr","line":"Error: compilation failed","session_id":"s1","task_id":"t1","trace_id":"tr1","ts":"2026-05-06T10:00:01Z","sequence":1,"truncated":false}}"#
        )
        .unwrap();

        assert!(!detect_quota_exhaustion_in_log(&log_path, "codex-cli"));
    }

    #[test]
    fn detects_429_in_stderr() {
        let tmp = TempDir::new().unwrap();
        let log_path = tmp.path().join("test-session.jsonl");
        let mut file = std::fs::File::create(&log_path).unwrap();
        use std::io::Write;
        writeln!(
            file,
            r#"{{"stream":"stderr","line":"HTTP 429 Too Many Requests","session_id":"s1","task_id":"t1","trace_id":"tr1","ts":"2026-05-06T10:00:00Z","sequence":0,"truncated":false}}"#
        )
        .unwrap();

        assert!(detect_quota_exhaustion_in_log(&log_path, "claude-code"));
    }

    #[test]
    fn ignores_stdout_rate_limit_messages() {
        let tmp = TempDir::new().unwrap();
        let log_path = tmp.path().join("test-session.jsonl");
        let mut file = std::fs::File::create(&log_path).unwrap();
        use std::io::Write;
        writeln!(
            file,
            r#"{{"stream":"stdout","line":"I'm getting rate limited by the API","session_id":"s1","task_id":"t1","trace_id":"tr1","ts":"2026-05-06T10:00:00Z","sequence":0,"truncated":false}}"#
        )
        .unwrap();

        assert!(!detect_quota_exhaustion_in_log(&log_path, "codex-cli"));
    }
}
