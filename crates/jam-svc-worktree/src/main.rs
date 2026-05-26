//! `jam-svc-worktree` - Picker worktree creation service (§6.9).
//!
//! Phase 1 MVP for `task-jam-svc-worktree-creation-protocol`: traced NATS
//! request-reply on `tool.worktree.create`, a two-mutex worktree-create
//! protocol, a 60s fetch cursor, and strict path checks. The lease is
//! process-local for this first single-node service instance; the public
//! protocol shape leaves room for the later NATS-backed lease without changing
//! callers.

#![deny(missing_docs)]

use std::path::{Component, Path, PathBuf};
use std::process::ExitStatus;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::Bytes;
use chrono::{DateTime, Utc};
use futures::StreamExt;
use jam_events::generated::{Event, WorktreeCreated};
use jam_events::EventEnvelope;
use jam_nats::async_nats;
use jam_nats::JamNats;
use jam_tools_core::workspace::WorkspaceKey;
use jam_trace::TraceCtx;
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

const SERVICE_NAME: &str = "jam-svc-worktree";
const SERVICE_VERSION: &str = env!("CARGO_PKG_VERSION");
const SUBJECT_PREFIX: &str = "tool.worktree";
const SUBJECT_PREFIX_ENV: &str = "JAM_WORKTREE_SUBJECT_PREFIX";
const DEFAULT_BLUEBERRY_REPO_PATH: &str = "/home/caleb/blueberry";
const DEFAULT_JAMBOREE_REPO_PATH: &str = "/home/caleb/jamboree";
const DEFAULT_WORKTREE_ROOT: &str = "/home/picker/workers";
const DEFAULT_BLUEBERRY_TRUNK_BRANCH: &str = "main";
const DEFAULT_JAMBOREE_TRUNK_BRANCH: &str = "main";
const DEFAULT_FETCH_STALENESS_SECONDS: u64 = 60;
const TASK_ID_MAX_LEN: usize = 128;
const DEFAULT_PICKER_USER: &str = "picker";
const DEFAULT_SUDO_BIN: &str = "sudo";

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
enum WorktreeError {
    #[error("{kind}: {detail}")]
    Protocol {
        kind: &'static str,
        detail: String,
        remediation: &'static str,
    },
}

impl WorktreeError {
    fn protocol(kind: &'static str, detail: impl Into<String>, remediation: &'static str) -> Self {
        Self::Protocol {
            kind,
            detail: detail.into(),
            remediation,
        }
    }
}

#[derive(Clone)]
struct WorktreeState {
    fetch_mutex: Arc<Mutex<()>>,
    create_mutex: Arc<Mutex<()>>,
    fetch_cursor: Arc<Mutex<Option<FetchCursor>>>,
    config: WorktreeConfig,
}

#[derive(Debug, Clone)]
struct FetchCursor {
    fetched_at: DateTime<Utc>,
    observed_at: Instant,
}

#[derive(Debug, Clone)]
struct WorktreeConfig {
    blueberry_repo_path: PathBuf,
    jamboree_repo_path: PathBuf,
    worktree_root: PathBuf,
    blueberry_trunk_branch: String,
    jamboree_trunk_branch: String,
    fetch_staleness: Duration,
    configure_picker_safe_directory: bool,
    picker_user: String,
    sudo_bin: PathBuf,
}

impl WorktreeConfig {
    fn from_env() -> Self {
        let blueberry_repo_path = std::env::var_os("JAM_BLUEBERRY_REPO")
            .map_or_else(|| PathBuf::from(DEFAULT_BLUEBERRY_REPO_PATH), PathBuf::from);
        let jamboree_repo_path = std::env::var_os("JAM_JAMBOREE_REPO")
            .map_or_else(|| PathBuf::from(DEFAULT_JAMBOREE_REPO_PATH), PathBuf::from);
        let worktree_root = std::env::var_os("JAM_WORKTREE_ROOT")
            .map_or_else(|| PathBuf::from(DEFAULT_WORKTREE_ROOT), PathBuf::from);
        let blueberry_trunk_branch = std::env::var("JAM_BLUEBERRY_TRUNK_BRANCH")
            .or_else(|_| std::env::var("JAM_TRUNK_BRANCH"))
            .unwrap_or_else(|_| DEFAULT_BLUEBERRY_TRUNK_BRANCH.into());
        let jamboree_trunk_branch = std::env::var("JAM_JAMBOREE_TRUNK_BRANCH")
            .unwrap_or_else(|_| DEFAULT_JAMBOREE_TRUNK_BRANCH.into());
        let fetch_staleness = std::env::var("JAM_FETCH_STALENESS_SECS")
            .ok()
            .and_then(|raw| raw.parse().ok())
            .map_or(
                Duration::from_secs(DEFAULT_FETCH_STALENESS_SECONDS),
                Duration::from_secs,
            );
        let configure_picker_safe_directory = parse_bool_env("JAM_CONFIGURE_PICKER_SAFE_DIRECTORY")
            .unwrap_or_else(|| std::env::var("USER").is_ok_and(|user| user == "maestro"));
        let picker_user =
            std::env::var("JAM_PICKER_USER").unwrap_or_else(|_| DEFAULT_PICKER_USER.into());
        let sudo_bin = std::env::var_os("JAM_SUDO_BIN")
            .map_or_else(|| PathBuf::from(DEFAULT_SUDO_BIN), PathBuf::from);
        Self {
            blueberry_repo_path,
            jamboree_repo_path,
            worktree_root,
            blueberry_trunk_branch,
            jamboree_trunk_branch,
            fetch_staleness,
            configure_picker_safe_directory,
            picker_user,
            sudo_bin,
        }
    }
}

#[derive(Debug, Deserialize)]
struct WorktreeCreateInput {
    task_id: String,
    project: Option<String>,
    repo_path: Option<String>,
    worktree_root: Option<String>,
    trunk_branch: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WorktreeDiffInput {
    worktree_path: String,
    base_ref: Option<String>,
}

#[derive(Debug, Serialize)]
struct WorktreeDiffOutput {
    worktree_path: String,
    base_ref: String,
    changed_files: Vec<String>,
    diff: String,
}

#[derive(Debug, Deserialize)]
struct FindConflictsInput {
    worktree_path: String,
    target_ref: String,
}

#[derive(Debug, Serialize)]
struct FindConflictsOutput {
    worktree_path: String,
    target_ref: String,
    conflicting_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorktreeCreateOutput {
    task_id: String,
    project: String,
    repo_path: String,
    worktree_path: String,
    branch: String,
    trunk_ref: String,
    trunk_sha: String,
    fetched: bool,
    branched_at: DateTime<Utc>,
    fetch_cursor_at_create: DateTime<Utc>,
    trace_id: String,
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
        error!("jam-svc-worktree fatal: {err}");
        return std::process::ExitCode::FAILURE;
    }
    std::process::ExitCode::SUCCESS
}

async fn run() -> Result<(), ServiceError> {
    init_tracing();

    let nats_url = std::env::var("NATS_URL").unwrap_or_else(|_| "nats://127.0.0.1:4222".into());
    let nats_token = std::env::var("NATS_TOKEN").ok();
    let subject_prefix = configured_subject_prefix(SUBJECT_PREFIX_ENV, SUBJECT_PREFIX);
    let config = WorktreeConfig::from_env();

    info!(
        service = %SERVICE_NAME,
        version = %SERVICE_VERSION,
        nats = %nats_url,
        subject_prefix = %subject_prefix,
        blueberry_repo = %config.blueberry_repo_path.display(),
        jamboree_repo = %config.jamboree_repo_path.display(),
        worktree_root = %config.worktree_root.display(),
        "starting",
    );

    let nats = JamNats::connect(&nats_url, nats_token).await?;
    info!("connected to NATS");

    let state = WorktreeState {
        fetch_mutex: Arc::new(Mutex::new(())),
        create_mutex: Arc::new(Mutex::new(())),
        fetch_cursor: Arc::new(Mutex::new(None)),
        config,
    };

    let mut sub = nats
        .client()
        .subscribe(format!("{subject_prefix}.>"))
        .await
        .map_err(|e| ServiceError::Subscribe(e.to_string()))?;
    info!(subject = %format!("{subject_prefix}.>"), "subscribed");

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
    state: &WorktreeState,
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
                detail: "tool.worktree requests must include Trace-Id headers".into(),
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
    state: &WorktreeState,
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
        "create" | "worktree-create-protocol" => match create_worktree(payload, state, ctx).await {
            Ok(output) => {
                if let Err(err) = publish_created(nats, &output, ctx).await {
                    return Response::Error {
                        error: ResponseError {
                            kind: "journal-publish-failed".into(),
                            detail: err.to_string(),
                            remediation: "Verify NATS is running and the journal stream exists."
                                .into(),
                            tracked_by: "principle-failure-surfaces-immediately",
                        },
                    };
                }
                Response::Ok(serde_json::to_value(output).expect("output serializes"))
            }
            Err(err) => error_response(err),
        },
        "worktree-diff" => match worktree_diff(payload, state).await {
            Ok(output) => Response::Ok(serde_json::to_value(output).expect("output serializes")),
            Err(err) => error_response_with_tracker(err, "api-worktree-diff"),
        },
        "find-conflicts" => match find_conflicts(payload, state).await {
            Ok(output) => Response::Ok(serde_json::to_value(output).expect("output serializes")),
            Err(err) => error_response_with_tracker(err, "api-find-conflicts"),
        },
        unknown => Response::Error {
            error: ResponseError {
                kind: "unknown-method".into(),
                detail: format!("{SUBJECT_PREFIX}.{unknown} is not a recognized worktree method"),
                remediation: "Use tool.worktree.create.".into(),
                tracked_by: "graph/components/comp-jam-svc-worktree.md",
            },
        },
    }
}

async fn create_worktree(
    payload: &[u8],
    state: &WorktreeState,
    ctx: &TraceCtx,
) -> Result<WorktreeCreateOutput, WorktreeError> {
    let input = parse_input(payload)?;
    validate_task_id(&input.task_id)?;

    let project = input.project.unwrap_or_else(|| "blueberry".into());
    let project_defaults = project_defaults(&project, &state.config)?;
    let repo_path = input
        .repo_path
        .as_deref()
        .map_or_else(|| project_defaults.repo_path.clone(), PathBuf::from);
    let worktree_root = input
        .worktree_root
        .as_deref()
        .map_or_else(|| state.config.worktree_root.clone(), PathBuf::from);
    let trunk_branch = input
        .trunk_branch
        .unwrap_or_else(|| project_defaults.trunk_branch.clone());

    let repo_path =
        canonical_existing_dir(&repo_path, "repo-not-found", repo_remediation(&project))?;
    let workspace_key = WorkspaceKey::new(&input.task_id);
    let worktree_path = safe_worktree_path(&worktree_root, &workspace_key)?;
    let branch = format!("task/{}", input.task_id);

    let fetch = maybe_fetch(&repo_path, state).await?;
    // Prefer `origin/<trunk>` (latest fetched remote tip) whenever it
    // resolves — even when the most recent fetch was skipped via the
    // staleness cache, an earlier fetch could have populated the ref. Only
    // fall back to the local branch ref when origin/<trunk> doesn't exist
    // (private repo with no working remote, network outage, 404'd origin).
    let remote_ref = format!("origin/{trunk_branch}");
    let trunk_ref = if rev_parse(&repo_path, &remote_ref).await.is_ok() {
        remote_ref
    } else {
        trunk_branch.clone()
    };
    let trunk_sha = rev_parse(&repo_path, &trunk_ref).await?;

    {
        let _create_guard = state.create_mutex.lock().await;
        if worktree_path.exists() {
            if !worktree_path.join(".git").exists() {
                return Err(WorktreeError::protocol(
                    "worktree-corrupt",
                    format!(
                        "directory exists but is not a git worktree: {}",
                        worktree_path.display()
                    ),
                    "Remove the directory and retry, or use a different task_id.",
                ));
            }
            info!(
                task = %input.task_id,
                path = %worktree_path.display(),
                "worktree already exists; returning existing worktree (idempotent retry)",
            );
            return Ok(WorktreeCreateOutput {
                task_id: input.task_id,
                project,
                repo_path: repo_path.to_string_lossy().into_owned(),
                worktree_path: worktree_path.to_string_lossy().into_owned(),
                branch,
                trunk_ref,
                trunk_sha,
                fetched: fetch.fetched,
                branched_at: Utc::now(),
                fetch_cursor_at_create: fetch.cursor_at,
                trace_id: ctx.trace_id.to_string(),
            });
        }
        let worktree_path_string = worktree_path.to_string_lossy().into_owned();
        git(
            &repo_path,
            [
                "worktree",
                "add",
                &worktree_path_string,
                "-b",
                &branch,
                &trunk_sha,
            ],
        )
        .await?;
        make_group_writable_tree(&worktree_path)?;
        seed_jam_metadata_dir(&worktree_path)?;
        if state.config.configure_picker_safe_directory {
            mark_picker_safe_directory(&worktree_path, &state.config).await?;
        }
    }

    let branched_at = Utc::now();
    Ok(WorktreeCreateOutput {
        task_id: input.task_id,
        project,
        repo_path: repo_path.to_string_lossy().into_owned(),
        worktree_path: worktree_path.to_string_lossy().into_owned(),
        branch,
        trunk_ref,
        trunk_sha,
        fetched: fetch.fetched,
        branched_at,
        fetch_cursor_at_create: fetch.cursor_at,
        trace_id: ctx.trace_id.to_string(),
    })
}

#[derive(Debug, Clone, Copy)]
struct FetchOutcome {
    fetched: bool,
    cursor_at: DateTime<Utc>,
}

async fn maybe_fetch(
    repo_path: &Path,
    state: &WorktreeState,
) -> Result<FetchOutcome, WorktreeError> {
    let _fetch_guard = state.fetch_mutex.lock().await;
    let now = Utc::now();

    {
        let cursor = state.fetch_cursor.lock().await;
        if let Some(cursor) = cursor.as_ref() {
            if cursor.observed_at.elapsed() < state.config.fetch_staleness {
                return Ok(FetchOutcome {
                    fetched: false,
                    cursor_at: cursor.fetched_at,
                });
            }
        }
    }

    // Fetch is best-effort. Failure modes that should NOT block worktree
    // creation:
    //   - origin has no URL configured (private monorepo, never pushed)
    //   - origin URL resolves but the remote repo doesn't exist (404)
    //   - network is down
    // We still try the fetch so day-to-day Blueberry/Jamboree work picks up
    // upstream refs; we just log + continue when it fails. The rest of the
    // flow uses whichever ref `trunk_ref` resolves to — local-only refs work
    // fine when origin isn't fetchable.
    let fetched = match git(repo_path, ["fetch", "origin", "--prune", "--tags"]).await {
        Ok(_) => true,
        Err(err) => {
            tracing::warn!(
                repo = %repo_path.display(),
                error = ?err,
                "git fetch origin failed; continuing with local refs",
            );
            false
        }
    };
    let mut cursor = state.fetch_cursor.lock().await;
    *cursor = Some(FetchCursor {
        fetched_at: now,
        observed_at: Instant::now(),
    });
    Ok(FetchOutcome {
        fetched,
        cursor_at: now,
    })
}

async fn rev_parse(repo_path: &Path, trunk_ref: &str) -> Result<String, WorktreeError> {
    let commit_ref = format!("{trunk_ref}^{{commit}}");
    let output = git(repo_path, ["rev-parse", "--verify", &commit_ref]).await?;
    Ok(output.trim().to_owned())
}

async fn publish_created(
    nats: &JamNats,
    output: &WorktreeCreateOutput,
    ctx: &TraceCtx,
) -> Result<(), jam_nats::NatsError> {
    let payload = WorktreeCreated {
        task_id: output.task_id.clone(),
        project: output.project.clone(),
        repo_path: output.repo_path.clone(),
        worktree_path: output.worktree_path.clone(),
        branch: output.branch.clone(),
        trunk_ref: output.trunk_ref.clone(),
        trunk_sha: output.trunk_sha.clone(),
        fetched: output.fetched,
        branched_at: output.branched_at,
        fetch_cursor_at_create: output.fetch_cursor_at_create,
    };
    let envelope = EventEnvelope::new(
        WorktreeCreated::EVENT_TYPE,
        WorktreeCreated::EVENT_SUBTYPE_VERSION,
        0,
        ctx.trace_id.to_string(),
        SERVICE_NAME,
        payload,
    );
    nats.publish_traced("journal.worktree.created", &envelope, ctx)
        .await
}

async fn worktree_diff(
    payload: &[u8],
    state: &WorktreeState,
) -> Result<WorktreeDiffOutput, WorktreeError> {
    let input = parse_diff_input(payload)?;
    let worktree_path =
        canonical_worktree_under_root(&input.worktree_path, &state.config.worktree_root)?;
    ensure_git_worktree_root(&worktree_path).await?;
    let base_ref = input
        .base_ref
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("HEAD")
        .to_owned();
    validate_git_ref("base_ref", &base_ref)?;
    let changed_files =
        parse_git_paths(&git(&worktree_path, ["diff", "--name-only", &base_ref, "--"]).await?);
    let diff = git(
        &worktree_path,
        ["diff", "--no-ext-diff", "--no-color", &base_ref, "--"],
    )
    .await?;

    Ok(WorktreeDiffOutput {
        worktree_path: worktree_path.to_string_lossy().into_owned(),
        base_ref,
        changed_files,
        diff,
    })
}

async fn find_conflicts(
    payload: &[u8],
    state: &WorktreeState,
) -> Result<FindConflictsOutput, WorktreeError> {
    let input = parse_conflicts_input(payload)?;
    let worktree_path =
        canonical_worktree_under_root(&input.worktree_path, &state.config.worktree_root)?;
    ensure_git_worktree_root(&worktree_path).await?;
    let target_ref = input.target_ref.trim().to_owned();
    validate_git_ref("target_ref", &target_ref)?;
    let output = git_output(
        &worktree_path,
        [
            "merge-tree",
            "--write-tree",
            "--name-only",
            "--no-messages",
            "HEAD",
            &target_ref,
        ],
    )
    .await?;
    let conflicting_paths = match output.status.code() {
        Some(0) => Vec::new(),
        Some(1) => parse_merge_tree_conflicts(&output.stdout),
        _ => {
            let detail = if output.stderr.trim().is_empty() {
                output.stdout.trim().to_owned()
            } else {
                output.stderr.trim().to_owned()
            };
            return Err(WorktreeError::protocol(
                "git-command-failed",
                format!(
                    "git -C {} merge-tree --write-tree --name-only --no-messages HEAD {}: {detail}",
                    worktree_path.display(),
                    target_ref
                ),
                "Verify target_ref exists and the worktree is a valid git worktree.",
            ));
        }
    };

    Ok(FindConflictsOutput {
        worktree_path: worktree_path.to_string_lossy().into_owned(),
        target_ref,
        conflicting_paths,
    })
}

async fn git<const N: usize>(repo_path: &Path, args: [&str; N]) -> Result<String, WorktreeError> {
    let output = git_output(repo_path, args).await?;
    if !output.status.success() {
        let detail = if output.stderr.trim().is_empty() {
            output.stdout.trim().to_owned()
        } else {
            output.stderr.trim().to_owned()
        };
        return Err(WorktreeError::protocol(
            "git-command-failed",
            format!(
                "git -C {} {}: {detail}",
                repo_path.display(),
                args.join(" ")
            ),
            "Do not fall back to local trunk; fix the repository/remote state and retry.",
        ));
    }
    Ok(output.stdout)
}

#[derive(Debug)]
struct GitCommandOutput {
    status: ExitStatus,
    stdout: String,
    stderr: String,
}

async fn git_output<const N: usize>(
    repo_path: &Path,
    args: [&str; N],
) -> Result<GitCommandOutput, WorktreeError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(args)
        .output()
        .await
        .map_err(|err| {
            WorktreeError::protocol(
                "git-exec-failed",
                format!("failed to run git in {}: {err}", repo_path.display()),
                "Verify git is installed and the configured repository path is valid.",
            )
        })?;
    Ok(GitCommandOutput {
        status: output.status,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

async fn mark_picker_safe_directory(
    worktree_path: &Path,
    config: &WorktreeConfig,
) -> Result<(), WorktreeError> {
    let worktree_path = worktree_path.to_string_lossy().into_owned();
    let output = Command::new(&config.sudo_bin)
        .args(["-n", "-u", &config.picker_user, "-H", "git"])
        .args([
            "config",
            "--global",
            "--add",
            "safe.directory",
            &worktree_path,
        ])
        .output()
        .await
        .map_err(|err| {
            WorktreeError::protocol(
                "picker-safe-directory-failed",
                format!(
                    "failed to run {} for picker safe.directory: {err}",
                    config.sudo_bin.display()
                ),
                "Verify maestro can run sudo -n -u picker git config.",
            )
        })?;
    if output.status.success() {
        return Ok(());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let detail = if stderr.trim().is_empty() {
        stdout.trim().to_owned()
    } else {
        stderr.trim().to_owned()
    };
    Err(WorktreeError::protocol(
        "picker-safe-directory-failed",
        format!("failed to mark Picker worktree safe for git: {detail}"),
        "Verify maestro can run sudo -n -u picker git config.",
    ))
}

fn parse_bool_env(key: &str) -> Option<bool> {
    std::env::var(key).ok().and_then(|raw| {
        let normalized = raw.trim().to_ascii_lowercase();
        match normalized.as_str() {
            "1" | "true" | "yes" | "on" => Some(true),
            "0" | "false" | "no" | "off" => Some(false),
            _ => None,
        }
    })
}

#[cfg(unix)]
fn make_group_writable_tree(path: &Path) -> Result<(), WorktreeError> {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    fn visit(path: &Path) -> std::io::Result<()> {
        let metadata = fs::symlink_metadata(path)?;
        if metadata.file_type().is_symlink() {
            return Ok(());
        }

        let mut mode = metadata.permissions().mode();
        if metadata.is_dir() {
            mode |= 0o2770;
        } else {
            mode |= 0o660;
            if mode & 0o100 != 0 {
                mode |= 0o010;
            }
        }
        fs::set_permissions(path, fs::Permissions::from_mode(mode))?;

        if metadata.is_dir() {
            for entry in fs::read_dir(path)? {
                visit(&entry?.path())?;
            }
        }
        Ok(())
    }

    visit(path).map_err(|err| {
        WorktreeError::protocol(
            "worktree-permission-fix-failed",
            format!(
                "failed to make {} group-writable for Picker handoff: {err}",
                path.display()
            ),
            "Ensure jam-svc-worktree owns the new worktree and picker is in the shared runtime group.",
        )
    })
}

#[cfg(not(unix))]
fn make_group_writable_tree(_path: &Path) -> Result<(), WorktreeError> {
    Ok(())
}

/// Pre-create the `.jam/` metadata dir inside the worktree with mode 2770
/// (group write + setgid) so files written by the picker (which runs as a
/// different uid than the maestro-running services that read/write the
/// codex-events log, PR title/body, and review-artifact state) all share
/// the maestro group and remain mutually writable. Without this, the
/// picker's default umask drops group write and resume-picker bricks on
/// `Permission denied` when trying to append to `.jam/codex-events.jsonl`.
///
/// Idempotent: a pre-existing dir is reset to the right mode without
/// touching its contents (the picker may have already created files).
#[cfg(unix)]
fn seed_jam_metadata_dir(worktree: &Path) -> Result<(), WorktreeError> {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    let path = worktree.join(".jam");
    if !path.exists() {
        fs::create_dir(&path).map_err(|err| {
            WorktreeError::protocol(
                "jam-metadata-dir-create-failed",
                format!("failed to mkdir {}: {err}", path.display()),
                "Ensure the worktree path is writable by jam-svc-worktree.",
            )
        })?;
    }
    fs::set_permissions(&path, fs::Permissions::from_mode(0o2770)).map_err(|err| {
        WorktreeError::protocol(
            "jam-metadata-dir-chmod-failed",
            format!("failed to chmod {} 2770: {err}", path.display()),
            "Ensure the worktree path is writable by jam-svc-worktree.",
        )
    })?;
    Ok(())
}

#[cfg(not(unix))]
fn seed_jam_metadata_dir(_worktree: &Path) -> Result<(), WorktreeError> {
    Ok(())
}

fn parse_input(payload: &[u8]) -> Result<WorktreeCreateInput, WorktreeError> {
    serde_json::from_slice(payload).map_err(|err| {
        WorktreeError::protocol(
            "invalid-input",
            format!("tool.worktree.create payload is invalid JSON: {err}"),
            "Send a JSON object with task_id and optional project/repo_path/worktree_root.",
        )
    })
}

fn parse_diff_input(payload: &[u8]) -> Result<WorktreeDiffInput, WorktreeError> {
    serde_json::from_slice(payload).map_err(|err| {
        WorktreeError::protocol(
            "invalid-input",
            format!("tool.worktree.worktree-diff payload is invalid JSON: {err}"),
            "Send a JSON object with worktree_path and optional base_ref.",
        )
    })
}

fn parse_conflicts_input(payload: &[u8]) -> Result<FindConflictsInput, WorktreeError> {
    serde_json::from_slice(payload).map_err(|err| {
        WorktreeError::protocol(
            "invalid-input",
            format!("tool.worktree.find-conflicts payload is invalid JSON: {err}"),
            "Send a JSON object with worktree_path and target_ref.",
        )
    })
}

fn validate_task_id(task_id: &str) -> Result<(), WorktreeError> {
    if task_id.is_empty() || task_id.len() > TASK_ID_MAX_LEN {
        return Err(WorktreeError::protocol(
            "invalid-task-id",
            "task_id must be 1-128 characters",
            "Use the task_id emitted by jam task spawn.",
        ));
    }
    if task_id == "." || task_id == ".." || task_id.contains("..") {
        return Err(WorktreeError::protocol(
            "invalid-task-id",
            format!("task_id may not contain parent-directory segments: {task_id}"),
            "Use a slug-like task_id with letters, numbers, dots, underscores, and dashes.",
        ));
    }
    if !task_id
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(WorktreeError::protocol(
            "invalid-task-id",
            format!("task_id contains unsafe characters: {task_id}"),
            "Use a slug-like task_id with letters, numbers, dots, underscores, and dashes.",
        ));
    }
    Ok(())
}

fn validate_git_ref(label: &str, value: &str) -> Result<(), WorktreeError> {
    if value.is_empty() || value.starts_with('-') {
        return Err(WorktreeError::protocol(
            "invalid-git-ref",
            format!("{label} must be a non-option git ref"),
            "Use a safe branch, tag, or commit-ish ref.",
        ));
    }
    if value.contains("..")
        || value.contains("//")
        || value.ends_with('/')
        || value.split('/').any(|part| {
            Path::new(part)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("lock"))
        })
    {
        return Err(WorktreeError::protocol(
            "invalid-git-ref",
            format!("{label} is not a safe git ref: {value}"),
            "Use a safe branch, tag, or commit-ish ref.",
        ));
    }
    if value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'/' | b'_' | b'-'))
    {
        Ok(())
    } else {
        Err(WorktreeError::protocol(
            "invalid-git-ref",
            format!("{label} may only contain ASCII letters, numbers, `.`, `/`, `_`, and `-`"),
            "Use a safe branch, tag, or commit-ish ref.",
        ))
    }
}

fn safe_worktree_path(root: &Path, workspace_key: &WorkspaceKey) -> Result<PathBuf, WorktreeError> {
    if !root.is_absolute() {
        return Err(WorktreeError::protocol(
            "invalid-worktree-root",
            format!("worktree root must be absolute: {}", root.display()),
            "Set JAM_WORKTREE_ROOT to a Linux-native absolute directory.",
        ));
    }
    if root.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::CurDir | Component::Prefix(_)
        )
    }) {
        return Err(WorktreeError::protocol(
            "invalid-worktree-root",
            format!(
                "worktree root must be an absolute native path: {}",
                root.display()
            ),
            "Set JAM_WORKTREE_ROOT to a Linux-native absolute directory.",
        ));
    }
    std::fs::create_dir_all(root).map_err(|err| {
        WorktreeError::protocol(
            "worktree-root-create-failed",
            format!("failed to create {}: {err}", root.display()),
            "Create the Picker worktree root with writable permissions for the service user.",
        )
    })?;
    let canonical_root = root.canonicalize().map_err(|err| {
        WorktreeError::protocol(
            "worktree-root-not-found",
            format!("failed to canonicalize {}: {err}", root.display()),
            "Create the Picker worktree root with writable permissions for the service user.",
        )
    })?;
    let path = canonical_root.join(workspace_key.as_str());
    if !path.starts_with(&canonical_root) {
        return Err(WorktreeError::protocol(
            "worktree-path-escape",
            format!("computed path escapes root: {}", path.display()),
            "Use a safe task_id and JAM_WORKTREE_ROOT.",
        ));
    }
    Ok(path)
}

fn canonical_worktree_under_root(
    worktree_path: &str,
    worktree_root: &Path,
) -> Result<PathBuf, WorktreeError> {
    let requested = PathBuf::from(worktree_path);
    if !requested.is_absolute()
        || requested.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::CurDir | Component::Prefix(_)
            )
        })
    {
        return Err(WorktreeError::protocol(
            "invalid-worktree-path",
            format!("worktree_path must be an absolute native path: {worktree_path}"),
            "Pass the worktree_path emitted by tool.worktree.create.",
        ));
    }
    let canonical_root = canonical_existing_dir(
        worktree_root,
        "worktree-root-not-found",
        "Verify JAM_WORKTREE_ROOT points at the Picker worktree root.",
    )?;
    let canonical = canonical_existing_dir(
        &requested,
        "worktree-not-found",
        "Pass the worktree_path emitted by tool.worktree.create.",
    )?;
    if !canonical.starts_with(&canonical_root) {
        return Err(WorktreeError::protocol(
            "worktree-path-escape",
            format!(
                "worktree path {} is outside configured root {}",
                canonical.display(),
                canonical_root.display()
            ),
            "Pass the worktree_path emitted by tool.worktree.create.",
        ));
    }
    Ok(canonical)
}

async fn ensure_git_worktree_root(path: &Path) -> Result<(), WorktreeError> {
    let top_level = git(path, ["rev-parse", "--show-toplevel"]).await?;
    let top_level = PathBuf::from(top_level.trim());
    let top_level = top_level.canonicalize().map_err(|err| {
        WorktreeError::protocol(
            "worktree-not-found",
            format!(
                "failed to canonicalize git top-level {}: {err}",
                top_level.display()
            ),
            "Pass a valid git worktree root.",
        )
    })?;
    if top_level != path {
        return Err(WorktreeError::protocol(
            "invalid-worktree-path",
            format!(
                "worktree_path must be the git top-level; got {}, top-level is {}",
                path.display(),
                top_level.display()
            ),
            "Pass the worktree_path emitted by tool.worktree.create.",
        ));
    }
    Ok(())
}

fn canonical_existing_dir(
    path: &Path,
    kind: &'static str,
    remediation: &'static str,
) -> Result<PathBuf, WorktreeError> {
    let canonical = path.canonicalize().map_err(|err| {
        WorktreeError::protocol(
            kind,
            format!("failed to canonicalize {}: {err}", path.display()),
            remediation,
        )
    })?;
    if !canonical.is_dir() {
        return Err(WorktreeError::protocol(
            kind,
            format!("not a directory: {}", canonical.display()),
            remediation,
        ));
    }
    Ok(canonical)
}

#[derive(Debug, Clone)]
struct ProjectDefaults {
    repo_path: PathBuf,
    trunk_branch: String,
}

fn project_defaults(
    project: &str,
    config: &WorktreeConfig,
) -> Result<ProjectDefaults, WorktreeError> {
    match project {
        "blueberry" => Ok(ProjectDefaults {
            repo_path: config.blueberry_repo_path.clone(),
            trunk_branch: config.blueberry_trunk_branch.clone(),
        }),
        "jamboree" => Ok(ProjectDefaults {
            repo_path: config.jamboree_repo_path.clone(),
            trunk_branch: config.jamboree_trunk_branch.clone(),
        }),
        _ => Err(WorktreeError::protocol(
            "unsupported-project",
            format!("supported projects are blueberry and jamboree, got {project}"),
            "Choose the explicit Blueberry or Jamboree target when creating the task.",
        )),
    }
}

fn repo_remediation(project: &str) -> &'static str {
    match project {
        "jamboree" => "Set JAM_JAMBOREE_REPO to the native Jamboree git checkout.",
        _ => "Set JAM_BLUEBERRY_REPO to the native Blueberry git checkout.",
    }
}

fn error_response(err: WorktreeError) -> Response {
    error_response_with_tracker(err, "task-jam-svc-worktree-creation-protocol")
}

fn error_response_with_tracker(err: WorktreeError, tracked_by: &'static str) -> Response {
    match err {
        WorktreeError::Protocol {
            kind,
            detail,
            remediation,
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

fn parse_git_paths(raw: &str) -> Vec<String> {
    raw.lines()
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .filter(|path| safe_relative_git_path(path))
        .map(str::to_owned)
        .collect()
}

fn parse_merge_tree_conflicts(raw: &str) -> Vec<String> {
    raw.lines()
        .skip(1)
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .filter(|path| safe_relative_git_path(path))
        .map(str::to_owned)
        .collect()
}

fn safe_relative_git_path(path: &str) -> bool {
    let path = Path::new(path);
    !path.is_absolute()
        && !path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::CurDir | Component::Prefix(_)
            )
        })
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

fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("jam_svc_worktree=info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn versioned_subject_prefix_subscription_keeps_methods_parseable() {
        let prefix = "tool.worktree.v047";

        assert_eq!(format!("{prefix}.>"), "tool.worktree.v047.>");
        assert_eq!(
            method_from_subject("tool.worktree.v047.create"),
            Some("create")
        );
        assert_eq!(method_from_subject("tool.worktree.v047.ping"), Some("ping"));
    }

    #[test]
    fn rejects_unsafe_task_ids() {
        for task_id in ["", "../x", "x/y", "x y", "..", "a..b"] {
            assert!(validate_task_id(task_id).is_err(), "{task_id}");
        }
        assert!(validate_task_id("2026-05-06-safe_task.1").is_ok());
    }

    #[test]
    fn safe_worktree_path_stays_under_root() {
        let tmp = TempDir::new().unwrap();
        let path = safe_worktree_path(tmp.path(), &WorkspaceKey::new("task-1")).unwrap();
        assert!(path.starts_with(tmp.path().canonicalize().unwrap()));
        assert!(path.ends_with("task-1"));
    }

    #[test]
    fn default_worktree_root_is_picker_workers() {
        assert_eq!(DEFAULT_WORKTREE_ROOT, "/home/picker/workers");
    }

    #[tokio::test]
    async fn second_create_skips_fetch_inside_staleness_window() {
        let fixture = GitFixture::new().await;
        let state = WorktreeState {
            fetch_mutex: Arc::new(Mutex::new(())),
            create_mutex: Arc::new(Mutex::new(())),
            fetch_cursor: Arc::new(Mutex::new(None)),
            config: WorktreeConfig {
                blueberry_repo_path: fixture.clone.path().to_path_buf(),
                jamboree_repo_path: fixture.clone.path().to_path_buf(),
                worktree_root: fixture.worktrees.path().to_path_buf(),
                blueberry_trunk_branch: "main".into(),
                jamboree_trunk_branch: "main".into(),
                fetch_staleness: Duration::from_secs(60),
                configure_picker_safe_directory: false,
                picker_user: DEFAULT_PICKER_USER.into(),
                sudo_bin: PathBuf::from(DEFAULT_SUDO_BIN),
            },
        };
        let ctx = TraceCtx::new_root("test", "worktree unit");

        let first = create_worktree(
            br#"{"task_id":"task-1","project":"blueberry"}"#,
            &state,
            &ctx,
        )
        .await
        .unwrap();
        let second = create_worktree(
            br#"{"task_id":"task-2","project":"blueberry"}"#,
            &state,
            &ctx,
        )
        .await
        .unwrap();

        assert!(first.fetched);
        assert!(!second.fetched);
        assert_eq!(first.trunk_sha, fixture.initial_sha);
        assert_eq!(second.trunk_sha, fixture.initial_sha);
        assert!(Path::new(&first.worktree_path).join("README.md").is_file());
        assert!(Path::new(&second.worktree_path).join("README.md").is_file());
    }

    #[tokio::test]
    async fn creates_jamboree_worktree_from_explicit_project() {
        let fixture = GitFixture::new().await;
        let state = WorktreeState {
            fetch_mutex: Arc::new(Mutex::new(())),
            create_mutex: Arc::new(Mutex::new(())),
            fetch_cursor: Arc::new(Mutex::new(None)),
            config: WorktreeConfig {
                blueberry_repo_path: PathBuf::from("/does/not/matter"),
                jamboree_repo_path: fixture.clone.path().to_path_buf(),
                worktree_root: fixture.worktrees.path().to_path_buf(),
                blueberry_trunk_branch: "main".into(),
                jamboree_trunk_branch: "main".into(),
                fetch_staleness: Duration::from_secs(60),
                configure_picker_safe_directory: false,
                picker_user: DEFAULT_PICKER_USER.into(),
                sudo_bin: PathBuf::from(DEFAULT_SUDO_BIN),
            },
        };
        let ctx = TraceCtx::new_root("test", "jamboree worktree unit");

        let created = create_worktree(
            br#"{"task_id":"task-jamboree","project":"jamboree"}"#,
            &state,
            &ctx,
        )
        .await
        .unwrap();

        assert_eq!(created.project, "jamboree");
        assert_eq!(created.trunk_ref, "origin/main");
        assert!(Path::new(&created.worktree_path)
            .join("README.md")
            .is_file());
    }

    #[tokio::test]
    async fn stale_cursor_fetches_new_origin_trunk() {
        let fixture = GitFixture::new().await;
        let state = WorktreeState {
            fetch_mutex: Arc::new(Mutex::new(())),
            create_mutex: Arc::new(Mutex::new(())),
            fetch_cursor: Arc::new(Mutex::new(Some(FetchCursor {
                fetched_at: Utc::now() - chrono::Duration::seconds(120),
                observed_at: Instant::now()
                    .checked_sub(Duration::from_secs(120))
                    .unwrap(),
            }))),
            config: WorktreeConfig {
                blueberry_repo_path: fixture.clone.path().to_path_buf(),
                jamboree_repo_path: fixture.clone.path().to_path_buf(),
                worktree_root: fixture.worktrees.path().to_path_buf(),
                blueberry_trunk_branch: "main".into(),
                jamboree_trunk_branch: "main".into(),
                fetch_staleness: Duration::from_secs(60),
                configure_picker_safe_directory: false,
                picker_user: DEFAULT_PICKER_USER.into(),
                sudo_bin: PathBuf::from(DEFAULT_SUDO_BIN),
            },
        };
        fixture.push_second_commit().await;
        let ctx = TraceCtx::new_root("test", "worktree unit");

        let created = create_worktree(
            br#"{"task_id":"task-3","project":"blueberry"}"#,
            &state,
            &ctx,
        )
        .await
        .unwrap();

        assert!(created.fetched);
        assert_ne!(created.trunk_sha, fixture.initial_sha);
        assert!(Path::new(&created.worktree_path)
            .join("SECOND.md")
            .is_file());
    }

    #[tokio::test]
    async fn worktree_diff_returns_changed_files_and_unified_diff() {
        let fixture = GitFixture::new().await;
        let state = WorktreeState {
            fetch_mutex: Arc::new(Mutex::new(())),
            create_mutex: Arc::new(Mutex::new(())),
            fetch_cursor: Arc::new(Mutex::new(None)),
            config: WorktreeConfig {
                blueberry_repo_path: fixture.clone.path().to_path_buf(),
                jamboree_repo_path: fixture.clone.path().to_path_buf(),
                worktree_root: fixture.worktrees.path().to_path_buf(),
                blueberry_trunk_branch: "main".into(),
                jamboree_trunk_branch: "main".into(),
                fetch_staleness: Duration::from_secs(60),
                configure_picker_safe_directory: false,
                picker_user: DEFAULT_PICKER_USER.into(),
                sudo_bin: PathBuf::from(DEFAULT_SUDO_BIN),
            },
        };
        let ctx = TraceCtx::new_root("test", "worktree unit");
        let created = create_worktree(
            br#"{"task_id":"task-diff","project":"blueberry"}"#,
            &state,
            &ctx,
        )
        .await
        .unwrap();
        let worktree = PathBuf::from(&created.worktree_path);
        std::fs::write(worktree.join("README.md"), "changed\n").unwrap();

        let payload = serde_json::json!({
            "worktree_path": created.worktree_path,
            "base_ref": "HEAD"
        })
        .to_string();
        let output = worktree_diff(payload.as_bytes(), &state).await.unwrap();

        assert_eq!(output.changed_files, vec!["README.md"]);
        assert!(output.diff.contains("-hello"));
        assert!(output.diff.contains("+changed"));
    }

    #[tokio::test]
    async fn find_conflicts_uses_merge_tree_paths() {
        let fixture = GitFixture::new().await;
        let state = WorktreeState {
            fetch_mutex: Arc::new(Mutex::new(())),
            create_mutex: Arc::new(Mutex::new(())),
            fetch_cursor: Arc::new(Mutex::new(None)),
            config: WorktreeConfig {
                blueberry_repo_path: fixture.clone.path().to_path_buf(),
                jamboree_repo_path: fixture.clone.path().to_path_buf(),
                worktree_root: fixture.worktrees.path().to_path_buf(),
                blueberry_trunk_branch: "main".into(),
                jamboree_trunk_branch: "main".into(),
                fetch_staleness: Duration::from_secs(60),
                configure_picker_safe_directory: false,
                picker_user: DEFAULT_PICKER_USER.into(),
                sudo_bin: PathBuf::from(DEFAULT_SUDO_BIN),
            },
        };
        let ctx = TraceCtx::new_root("test", "worktree unit");
        let created = create_worktree(
            br#"{"task_id":"task-conflict","project":"blueberry"}"#,
            &state,
            &ctx,
        )
        .await
        .unwrap();
        let worktree = PathBuf::from(&created.worktree_path);
        std::fs::write(worktree.join("README.md"), "left\n").unwrap();
        run_git(
            &worktree,
            [
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.invalid",
                "commit",
                "-am",
                "left",
            ],
        )
        .await;
        run_git(
            fixture.clone.path(),
            ["checkout", "-q", "-b", "target-conflict", "origin/main"],
        )
        .await;
        std::fs::write(fixture.clone.path().join("README.md"), "right\n").unwrap();
        run_git(
            fixture.clone.path(),
            [
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.invalid",
                "commit",
                "-am",
                "right",
            ],
        )
        .await;

        let payload = serde_json::json!({
            "worktree_path": created.worktree_path,
            "target_ref": "target-conflict"
        })
        .to_string();
        let output = find_conflicts(payload.as_bytes(), &state).await.unwrap();

        assert_eq!(output.conflicting_paths, vec!["README.md"]);
    }

    struct GitFixture {
        origin: TempDir,
        clone: TempDir,
        author: TempDir,
        worktrees: TempDir,
        initial_sha: String,
    }

    impl GitFixture {
        async fn new() -> Self {
            let origin = TempDir::new().unwrap();
            let clone = TempDir::new().unwrap();
            let author = TempDir::new().unwrap();
            let worktrees = TempDir::new().unwrap();

            run("git", ["init", "--bare", origin.path().to_str().unwrap()]).await;
            run(
                "git",
                ["init", "-b", "main", author.path().to_str().unwrap()],
            )
            .await;
            std::fs::write(author.path().join("README.md"), "hello\n").unwrap();
            run_git(
                author.path(),
                [
                    "-c",
                    "user.name=Test",
                    "-c",
                    "user.email=test@example.invalid",
                    "add",
                    ".",
                ],
            )
            .await;
            run_git(
                author.path(),
                [
                    "-c",
                    "user.name=Test",
                    "-c",
                    "user.email=test@example.invalid",
                    "commit",
                    "-m",
                    "initial",
                ],
            )
            .await;
            run_git(
                author.path(),
                ["remote", "add", "origin", origin.path().to_str().unwrap()],
            )
            .await;
            run_git(author.path(), ["push", "-u", "origin", "main"]).await;
            run(
                "git",
                [
                    "clone",
                    origin.path().to_str().unwrap(),
                    clone.path().to_str().unwrap(),
                ],
            )
            .await;
            run_git(clone.path(), ["fetch", "origin", "--prune", "--tags"]).await;
            let initial_sha = run_git_stdout(author.path(), ["rev-parse", "HEAD"]).await;

            Self {
                origin,
                clone,
                author,
                worktrees,
                initial_sha: initial_sha.trim().into(),
            }
        }

        async fn push_second_commit(&self) {
            assert!(self.origin.path().is_dir());
            std::fs::write(self.author.path().join("SECOND.md"), "second\n").unwrap();
            run_git(
                self.author.path(),
                [
                    "-c",
                    "user.name=Test",
                    "-c",
                    "user.email=test@example.invalid",
                    "add",
                    ".",
                ],
            )
            .await;
            run_git(
                self.author.path(),
                [
                    "-c",
                    "user.name=Test",
                    "-c",
                    "user.email=test@example.invalid",
                    "commit",
                    "-m",
                    "second",
                ],
            )
            .await;
            run_git(self.author.path(), ["push", "origin", "main"]).await;
        }
    }

    async fn run_git<const N: usize>(repo: &Path, args: [&str; N]) {
        let output = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .output()
            .await
            .unwrap();
        assert!(
            output.status.success(),
            "git -C {} {} failed\nstdout:\n{}\nstderr:\n{}",
            repo.display(),
            args.join(" "),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    async fn run_git_stdout<const N: usize>(repo: &Path, args: [&str; N]) -> String {
        let output = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .output()
            .await
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    async fn run<const N: usize>(bin: &str, args: [&str; N]) {
        let output = Command::new(bin).args(args).output().await.unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
