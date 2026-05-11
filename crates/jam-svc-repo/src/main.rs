//! `jam-svc-repo` - Repo / PR operations service (§5.4).
//!
//! This first slice exposes traced NATS request-reply for `open-pr` and
//! `pr-status` using the GitHub CLI as the narrow backend. The service keeps
//! the public tool shape stable for the later GitHub App implementation and
//! deliberately does not expose any merge operation (`principle-no-auto-merge`).

#![deny(missing_docs)]

use std::fs::OpenOptions;
use std::io::Write as _;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use chrono::{DateTime, Utc};
use futures::StreamExt;
use jam_events::generated::{Event, PrOpened, PrReviewReceived};
use jam_events::EventEnvelope;
use jam_nats::async_nats;
use jam_nats::JamNats;
use jam_secrets::{FileBackend, PassBackend, SecretBackend, SecretError, SecretKey};
use jam_trace::TraceCtx;
use jam_untrusted::Untrusted;
use octocrab::models::{AppId, InstallationId};
use octocrab::Octocrab;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use tracing::{debug, error, info, warn};

const SERVICE_NAME: &str = "jam-svc-repo";
const SERVICE_VERSION: &str = env!("CARGO_PKG_VERSION");
const SUBJECT_PREFIX: &str = "tool.repo";
const SUBJECT_PREFIX_ENV: &str = "JAM_REPO_SUBJECT_PREFIX";
const DEFAULT_GIT_BIN: &str = "git";
const DEFAULT_GH_BIN: &str = "gh";
const DEFAULT_CODEX_BIN: &str = "codex";
const DEFAULT_GITHUB_REPO: &str = "cleak/blueberry";
const DEFAULT_BASE_BRANCH: &str = "main";
const DEFAULT_NOTIFY_HUMAN_SUBJECT: &str = "tool.supervise.notify-human";
// CodeRabbit auto-skips PRs whose author GitHub reports as type=Bot (GitHub App
// installations always produce [bot] authors). Posting this trigger comment
// bypasses the skip; see docs/proposal-v5.md and the CodeRabbit FAQ.
// `@coderabbitai review` is incremental and treats the bot-author skip as
// already-reviewed commits, so it no-ops. `full review` forces a from-scratch
// pass over every file.
const CODERABBIT_TRIGGER_COMMENT: &str = "@coderabbitai full review";
const DEFAULT_TIMEOUT_SECS: u64 = 60;
const TOKEN_MAX_LEN: usize = 128;
const TITLE_MAX_LEN: usize = 240;
const COMMENT_MAX_LEN: usize = 65_536;
const GIT_APP_TOKEN_ENV: &str = "JAM_GITHUB_APP_INSTALLATION_TOKEN";
const GIT_APP_CREDENTIAL_HELPER: &str =
    "!f() { test \"$1\" = get || exit 0; printf 'username=x-access-token\\npassword=%s\\n' \"$JAM_GITHUB_APP_INSTALLATION_TOKEN\"; }; f";

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
enum RepoError {
    #[error("{kind}: {detail}")]
    Protocol {
        kind: &'static str,
        detail: String,
        remediation: &'static str,
        tracked_by: &'static str,
    },
}

impl RepoError {
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
struct RepoState {
    config: RepoConfig,
}

#[derive(Debug, Clone)]
struct RepoConfig {
    git_bin: PathBuf,
    gh_bin: PathBuf,
    codex_bin: PathBuf,
    github_repo: String,
    default_base: String,
    notify_human_subject: String,
    timeout: Duration,
    handled_artifacts_path: PathBuf,
    github_app: Option<GitHubAppConfig>,
    // User-to-server token (ghu_*) authorizing the App on behalf of a
    // human user. Preferred over installation tokens for write paths
    // (PR creation, push, comments) so PRs are attributed to that user
    // rather than `app/<name>[bot]`. CodeRabbit and similar reviewers
    // hard-skip bot-authored PRs; user-attribution avoids that.
    github_user_token: Option<SecretString>,
}

impl RepoConfig {
    fn from_env() -> Result<Self, RepoError> {
        let git_bin =
            std::env::var_os("JAM_GIT_BIN").map_or_else(|| DEFAULT_GIT_BIN.into(), PathBuf::from);
        let gh_bin =
            std::env::var_os("JAM_GH_BIN").map_or_else(|| DEFAULT_GH_BIN.into(), PathBuf::from);
        let codex_bin = std::env::var_os("JAM_CODEX_BIN")
            .map_or_else(|| DEFAULT_CODEX_BIN.into(), PathBuf::from);
        let github_repo = std::env::var("JAM_GITHUB_REPO")
            .or_else(|_| std::env::var("JAM_OBSERVE_GITHUB_REPO"))
            .unwrap_or_else(|_| DEFAULT_GITHUB_REPO.into());
        let default_base =
            std::env::var("JAM_TRUNK_BRANCH").unwrap_or_else(|_| DEFAULT_BASE_BRANCH.into());
        let notify_human_subject = std::env::var("JAM_NOTIFY_HUMAN_SUBJECT")
            .unwrap_or_else(|_| DEFAULT_NOTIFY_HUMAN_SUBJECT.into());
        let timeout = std::env::var("JAM_REPO_REQUEST_TIMEOUT_SECS")
            .ok()
            .and_then(|raw| raw.parse().ok())
            .map_or(
                Duration::from_secs(DEFAULT_TIMEOUT_SECS),
                Duration::from_secs,
            );
        let handled_artifacts_path = std::env::var_os("JAM_REVIEW_ARTIFACT_STATE_PATH")
            .map_or_else(
                || default_jam_home().join("review-artifacts-handled.jsonl"),
                PathBuf::from,
            );
        let github_app = GitHubAppConfig::from_env()?;
        let github_user_token = first_secret(
            &["JAM_GITHUB_USER_TOKEN", "GITHUB_USER_TOKEN"],
            "jam/pickers/github-user-token",
            "pickers/github-user-token",
        )?
        .map(SecretString::from);
        Ok(Self {
            git_bin,
            gh_bin,
            codex_bin,
            github_repo,
            default_base,
            notify_human_subject,
            timeout,
            handled_artifacts_path,
            github_app,
            github_user_token,
        })
    }
}

#[derive(Debug, Clone)]
struct GitHubAppConfig {
    app_id: u64,
    installation_id: u64,
    private_key_pem: SecretString,
    api_base_uri: Option<String>,
}

impl GitHubAppConfig {
    fn from_env() -> Result<Option<Self>, RepoError> {
        let app_id = first_secret(
            &["JAM_GITHUB_APP_ID", "GITHUB_APP_ID"],
            "jam/pickers/github-app-id",
            "pickers/github-app-id",
        )?;
        let installation_id = first_secret(
            &[
                "JAM_GITHUB_APP_INSTALLATION_ID",
                "GITHUB_APP_INSTALLATION_ID",
            ],
            "jam/pickers/github-app-installation-id",
            "pickers/github-app-installation-id",
        )?;
        let private_key = first_secret(
            &["JAM_GITHUB_APP_PRIVATE_KEY", "GITHUB_APP_PRIVATE_KEY"],
            "jam/pickers/github-app-key",
            "pickers/github-app-key",
        )?
        .or_else(|| read_optional_file_env("JAM_GITHUB_APP_PRIVATE_KEY_FILE"))
        .or_else(|| read_optional_file_env("GITHUB_APP_PRIVATE_KEY_FILE"));
        if app_id.is_none() && installation_id.is_none() && private_key.is_none() {
            return Ok(None);
        }
        let app_id = parse_required_u64("GITHUB_APP_ID", app_id)?;
        let installation_id = parse_required_u64("GITHUB_APP_INSTALLATION_ID", installation_id)?;
        let private_key_pem = private_key.ok_or_else(|| {
            RepoError::protocol(
                "invalid-github-app-config",
                "GitHub App private key is missing while other App config is set",
                "Set JAM_GITHUB_APP_PRIVATE_KEY or JAM_GITHUB_APP_PRIVATE_KEY_FILE.",
                "task-github-app-registration",
            )
        })?;
        Ok(Some(Self {
            app_id,
            installation_id,
            private_key_pem: SecretString::from(private_key_pem.replace("\\n", "\n")),
            api_base_uri: first_env(&["JAM_GITHUB_API_BASE_URI", "GITHUB_API_BASE_URI"]),
        }))
    }
}

#[derive(Debug, Deserialize)]
struct OpenPrInput {
    task_id: String,
    branch: String,
    title: String,
    body: Option<String>,
    draft: Option<bool>,
    base: Option<String>,
    repo: Option<String>,
    worktree_path: Option<String>,
    push: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenPrOutput {
    task_id: String,
    pr_ref: String,
    url: String,
    branch: String,
    title: String,
    draft: bool,
    state: String,
    opened_at: DateTime<Utc>,
    trace_id: String,
}

#[derive(Debug, Deserialize)]
struct PrStatusInput {
    pr_ref: String,
    repo: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ReadPrCommentsInput {
    pr_ref: String,
    repo: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ReplyToCommentInput {
    artifact_id: String,
    text: String,
}

#[derive(Debug, Deserialize)]
struct MarkReviewArtifactHandledInput {
    artifact_id: String,
    status: String,
    reasoning: String,
}

#[derive(Debug, Deserialize)]
struct RequestReviewInput {
    pr_ref: String,
    reviewer_id: String,
    task_id: Option<String>,
    repo: Option<String>,
    worktree_path: Option<String>,
    base: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PrepareMergeInput {
    pr_ref: String,
    repo: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RequestHumanMergeInput {
    pr_ref: String,
    summary: String,
    repo: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PrStatusOutput {
    pr_ref: String,
    url: String,
    number: u64,
    state: String,
    title: String,
    branch: String,
    draft: bool,
    trace_id: String,
}

#[derive(Debug, Serialize)]
struct ReadPrCommentsOutput {
    pr_ref: String,
    artifacts: Vec<ReviewArtifactOutput>,
    trace_id: String,
}

#[derive(Debug, Serialize)]
struct ReplyToCommentOutput {
    artifact_id: String,
    status: String,
    url: Option<String>,
    trace_id: String,
}

#[derive(Debug, Serialize)]
struct MarkReviewArtifactHandledOutput {
    artifact_id: String,
    status: String,
    handled_at: DateTime<Utc>,
    trace_id: String,
}

#[derive(Debug, Serialize)]
struct RequestReviewOutput {
    pr_ref: String,
    reviewer_id: String,
    status: String,
    artifacts: Vec<ReviewArtifactOutput>,
    trace_id: String,
}

#[derive(Debug, Clone, Serialize)]
struct PrepareMergeOutput {
    pr_ref: String,
    url: String,
    state: String,
    title: String,
    branch: String,
    draft: bool,
    merge_state_status: Option<String>,
    review_decision: Option<String>,
    checks_passed: bool,
    ready: bool,
    checks: Vec<MergeCheckOutput>,
    trace_id: String,
}

#[derive(Debug, Serialize)]
struct RequestHumanMergeOutput {
    pr_ref: String,
    status: String,
    notification_subject: String,
    notification_response: serde_json::Value,
    prepare: PrepareMergeOutput,
    trace_id: String,
}

#[derive(Debug, Serialize)]
struct ReviewArtifactOutput {
    id: String,
    reviewer: String,
    kind: &'static str,
    status: &'static str,
    body: String,
    body_trust: &'static str,
    url: Option<String>,
    path: Option<String>,
    line: Option<u64>,
    created_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize)]
struct MergeCheckOutput {
    name: String,
    state: Option<String>,
    conclusion: Option<String>,
    bucket: Option<String>,
    passed: bool,
    link: Option<String>,
    description: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GhPrView {
    number: u64,
    url: String,
    state: String,
    title: String,
    head_ref_name: String,
    is_draft: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GhPrepareMergeView {
    number: u64,
    url: String,
    state: String,
    title: String,
    head_ref_name: String,
    is_draft: bool,
    merge_state_status: Option<String>,
    review_decision: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GhPrCheck {
    name: String,
    state: Option<String>,
    conclusion: Option<String>,
    bucket: Option<String>,
    link: Option<String>,
    description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GhUser {
    login: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GhIssueComment {
    id: u64,
    body: Option<String>,
    html_url: Option<String>,
    user: Option<GhUser>,
    created_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
struct GhReviewComment {
    id: u64,
    body: Option<String>,
    html_url: Option<String>,
    user: Option<GhUser>,
    path: Option<String>,
    line: Option<u64>,
    original_line: Option<u64>,
    created_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
struct GhReview {
    id: u64,
    body: Option<String>,
    html_url: Option<String>,
    user: Option<GhUser>,
    #[serde(rename = "state")]
    _state: Option<String>,
    submitted_at: Option<DateTime<Utc>>,
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
        error!("jam-svc-repo fatal: {err}");
        return std::process::ExitCode::FAILURE;
    }
    std::process::ExitCode::SUCCESS
}

async fn run() -> Result<(), ServiceError> {
    init_tracing();

    let nats_url = std::env::var("NATS_URL").unwrap_or_else(|_| "nats://127.0.0.1:4222".into());
    let nats_token = std::env::var("NATS_TOKEN").ok();
    let subject_prefix = configured_subject_prefix(SUBJECT_PREFIX_ENV, SUBJECT_PREFIX);
    let config = RepoConfig::from_env().map_err(|err| ServiceError::Reply(err.to_string()))?;

    info!(
        service = %SERVICE_NAME,
        version = %SERVICE_VERSION,
        nats = %nats_url,
        subject_prefix = %subject_prefix,
        gh_bin = %config.gh_bin.display(),
        repo = %config.github_repo,
        "starting",
    );

    let nats = JamNats::connect(&nats_url, nats_token).await?;
    info!("connected to NATS");

    let state = RepoState { config };
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
    state: &RepoState,
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
                detail: "tool.repo requests must include Trace-Id headers".into(),
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
    state: &RepoState,
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
        "open-pr" => match open_pr(payload, state, ctx, nats).await {
            Ok(output) => Response::Ok(serde_json::to_value(output).expect("output serializes")),
            Err(err) => error_response(err),
        },
        "pr-status" => match pr_status(payload, state, ctx).await {
            Ok(output) => Response::Ok(serde_json::to_value(output).expect("output serializes")),
            Err(err) => error_response(err),
        },
        "read-pr-comments" => match read_pr_comments(payload, state, ctx).await {
            Ok(output) => Response::Ok(serde_json::to_value(output).expect("output serializes")),
            Err(err) => error_response(err),
        },
        "reply-to-comment" => match reply_to_comment(payload, state, ctx).await {
            Ok(output) => Response::Ok(serde_json::to_value(output).expect("output serializes")),
            Err(err) => error_response(err),
        },
        "mark-review-artifact-handled" => {
            match mark_review_artifact_handled(payload, state, ctx, nats).await {
                Ok(output) => {
                    Response::Ok(serde_json::to_value(output).expect("output serializes"))
                }
                Err(err) => error_response(err),
            }
        }
        "request-review" => match request_review(payload, state, ctx, nats).await {
            Ok(output) => Response::Ok(serde_json::to_value(output).expect("output serializes")),
            Err(err) => error_response(err),
        },
        "prepare-merge" => match prepare_merge(payload, state, ctx).await {
            Ok(output) => Response::Ok(serde_json::to_value(output).expect("output serializes")),
            Err(err) => error_response(err),
        },
        "request-human-merge" => match request_human_merge(payload, state, ctx, nats).await {
            Ok(output) => Response::Ok(serde_json::to_value(output).expect("output serializes")),
            Err(err) => error_response(err),
        },
        unknown => Response::Error {
            error: ResponseError {
                kind: "unknown-method".into(),
                detail: format!("{SUBJECT_PREFIX}.{unknown} is not a recognized repo method"),
                remediation: "Use tool.repo.open-pr or tool.repo.pr-status.".into(),
                tracked_by: "graph/components/comp-jam-svc-repo.md",
            },
        },
    }
}

async fn open_pr(
    payload: &[u8],
    state: &RepoState,
    ctx: &TraceCtx,
    nats: &JamNats,
) -> Result<OpenPrOutput, RepoError> {
    let mut input = parse_open_pr_input(payload)?;
    let repo = input
        .repo
        .clone()
        .unwrap_or_else(|| state.config.github_repo.clone());
    validate_repo(&repo)?;
    validate_token("task_id", &input.task_id, TOKEN_MAX_LEN)?;
    validate_branch("branch", &input.branch)?;
    let base = input
        .base
        .clone()
        .unwrap_or_else(|| state.config.default_base.clone());
    validate_branch("base", &base)?;
    input.title = format_jam_pr_title(&input.title)?;
    validate_title(&input.title)?;
    let draft = input.draft.unwrap_or(false);
    let worktree_path = input
        .worktree_path
        .as_deref()
        .map(validate_worktree_path)
        .transpose()?;

    if input.push.unwrap_or(true) {
        let worktree = worktree_path.as_deref().ok_or_else(|| {
            RepoError::protocol(
                "missing-worktree-path",
                "open-pr with push=true requires worktree_path",
                "Pass the Picker worktree path or set push=false after pushing the branch.",
                "api-open-pr",
            )
        })?;
        git_push(&state.config, worktree, &input.branch).await?;
    }

    let (url, token_kind) = gh_pr_create(
        &state.config,
        &repo,
        &base,
        &input,
        draft,
        worktree_path.as_deref(),
    )
    .await?;
    let pr_ref = pr_ref_from_url(&url).ok_or_else(|| {
        RepoError::protocol(
            "pr-url-unparseable",
            format!(
                "gh pr create returned URL that cannot be converted to owner/repo#number: {url}"
            ),
            "Verify gh returned a GitHub pull request URL.",
            "api-open-pr",
        )
    })?;
    let opened_at = Utc::now();
    // With a user-to-server token the PR is attributed to a real user
    // (is_bot:false) and CodeRabbit auto-reviews through the normal path.
    // The comment-trigger workaround is only needed when we fall back to
    // installation-token auth that flags PRs as bot-authored.
    if token_kind == TokenKind::Installation {
        if let Err(err) = post_coderabbit_trigger(&state.config, &pr_ref).await {
            warn!(
                pr_ref = %pr_ref,
                error = %err,
                "failed to post @coderabbitai full review trigger; PR is open but CodeRabbit may have skipped it"
            );
        }
    }
    let output = OpenPrOutput {
        task_id: input.task_id,
        pr_ref,
        url,
        branch: input.branch,
        title: input.title,
        draft,
        state: "open".into(),
        opened_at,
        trace_id: ctx.trace_id.to_string(),
    };
    publish_pr_opened(nats, &output, ctx).await?;
    Ok(output)
}

async fn post_coderabbit_trigger(config: &RepoConfig, pr_ref: &str) -> Result<(), RepoError> {
    let selector = PrApiSelector::from_pr_ref(pr_ref)?;
    let endpoint = format!(
        "repos/{}/issues/{}/comments",
        selector.repo, selector.number
    );
    gh_api_post_body(config, &endpoint, CODERABBIT_TRIGGER_COMMENT).await?;
    Ok(())
}

async fn github_app_installation_token(
    config: &RepoConfig,
) -> Result<Option<SecretString>, RepoError> {
    let Some(app) = &config.github_app else {
        return Ok(None);
    };
    app_installation_token(app).await.map(Some)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TokenKind {
    /// `ghu_*` user-to-server token. PRs/pushes attributed to the
    /// authorizing user with `is_bot:false`. Required to bypass
    /// CodeRabbit's bot-author skip.
    User,
    /// GitHub App installation token. PRs/pushes attributed to
    /// `app/<name>[bot]` with `is_bot:true`. Fine for read-heavy
    /// paths (higher rate limit); triggers reviewer bot-skips on
    /// writes.
    Installation,
}

/// Resolve the GitHub token for a *write* path (PR creation, push,
/// follow-up comments). Prefers the user-to-server token so reviewer
/// bots (CodeRabbit) auto-review through the normal path; falls back
/// to the installation token. Returns `(token, kind)` or `None` if
/// neither is configured.
async fn resolve_write_token(
    config: &RepoConfig,
) -> Result<Option<(SecretString, TokenKind)>, RepoError> {
    if let Some(token) = &config.github_user_token {
        return Ok(Some((token.clone(), TokenKind::User)));
    }
    Ok(github_app_installation_token(config)
        .await?
        .map(|token| (token, TokenKind::Installation)))
}

async fn app_installation_token(app: &GitHubAppConfig) -> Result<SecretString, RepoError> {
    let key =
        jsonwebtoken::EncodingKey::from_rsa_pem(app.private_key_pem.expose_secret().as_bytes())
            .map_err(|err| {
                RepoError::protocol(
                    "github-app-key-invalid",
                    format!("failed to parse GitHub App private key: {err}"),
                    "Verify the GitHub App private key PEM is seeded exactly as downloaded.",
                    "task-github-app-registration",
                )
            })?;
    let crab = {
        let mut builder = Octocrab::builder().app(AppId(app.app_id), key);
        if let Some(base_uri) = &app.api_base_uri {
            builder = builder.base_uri(base_uri).map_err(|err| {
                RepoError::protocol(
                    "github-app-config-invalid",
                    format!("invalid GitHub API base URI {base_uri:?}: {err}"),
                    "Unset JAM_GITHUB_API_BASE_URI or set it to a GitHub API-compatible base URL.",
                    "task-github-app-registration",
                )
            })?;
        }
        builder.build().map_err(|err| {
            RepoError::protocol(
                "github-app-client-build-failed",
                err.to_string(),
                "Verify GitHub App configuration and retry.",
                "task-github-app-registration",
            )
        })?
    };
    let (_installation, token) = crab
        .installation_and_token(InstallationId(app.installation_id))
        .await
        .map_err(|err| {
            RepoError::protocol(
                "github-app-token-exchange-failed",
                err.to_string(),
                "Verify the App is installed on the Blueberry repo and the private key is current.",
                "task-github-app-registration",
            )
        })?;
    Ok(token)
}

async fn pr_status(
    payload: &[u8],
    state: &RepoState,
    ctx: &TraceCtx,
) -> Result<PrStatusOutput, RepoError> {
    let input: PrStatusInput = serde_json::from_slice(payload).map_err(|err| {
        RepoError::protocol(
            "invalid-input",
            format!("tool.repo.pr-status payload is invalid JSON: {err}"),
            "Send {\"pr_ref\":\"owner/repo#123\"} or a GitHub PR URL.",
            "api-pr-status",
        )
    })?;
    let selector = PrSelector::parse(&input.pr_ref, input.repo.as_deref(), &state.config)?;
    pr_status_selector(&state.config, &selector, ctx).await
}

async fn read_pr_comments(
    payload: &[u8],
    state: &RepoState,
    ctx: &TraceCtx,
) -> Result<ReadPrCommentsOutput, RepoError> {
    let input: ReadPrCommentsInput = serde_json::from_slice(payload).map_err(|err| {
        RepoError::protocol(
            "invalid-input",
            format!("tool.repo.read-pr-comments payload is invalid JSON: {err}"),
            "Send {\"pr_ref\":\"owner/repo#123\"} or a GitHub PR URL.",
            "api-read-pr-comments",
        )
    })?;
    let selector = PrApiSelector::parse(&input.pr_ref, input.repo.as_deref(), &state.config)?;
    let mut artifacts = Vec::new();

    let issue_comments: Vec<GhIssueComment> = gh_api_json(
        &state.config,
        &format!(
            "repos/{}/issues/{}/comments",
            selector.repo, selector.number
        ),
    )
    .await?;
    artifacts.extend(
        issue_comments
            .into_iter()
            .map(|comment| issue_comment_artifact(&selector, comment)),
    );

    let review_comments: Vec<GhReviewComment> = gh_api_json(
        &state.config,
        &format!("repos/{}/pulls/{}/comments", selector.repo, selector.number),
    )
    .await?;
    artifacts.extend(
        review_comments
            .into_iter()
            .map(|comment| review_comment_artifact(&selector, comment)),
    );

    let reviews: Vec<GhReview> = gh_api_json(
        &state.config,
        &format!("repos/{}/pulls/{}/reviews", selector.repo, selector.number),
    )
    .await?;
    artifacts.extend(reviews.into_iter().filter_map(|review| {
        if review
            .body
            .as_ref()
            .is_some_and(|body| !body.trim().is_empty())
        {
            Some(review_artifact(&selector, review))
        } else {
            None
        }
    }));

    artifacts.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(ReadPrCommentsOutput {
        pr_ref: selector.pr_ref,
        artifacts,
        trace_id: ctx.trace_id.to_string(),
    })
}

async fn reply_to_comment(
    payload: &[u8],
    state: &RepoState,
    ctx: &TraceCtx,
) -> Result<ReplyToCommentOutput, RepoError> {
    let input: ReplyToCommentInput = serde_json::from_slice(payload).map_err(|err| {
        RepoError::protocol(
            "invalid-input",
            format!("tool.repo.reply-to-comment payload is invalid JSON: {err}"),
            "Send {\"artifact_id\":\"github-review-comment:owner/repo#123:456\",\"text\":\"...\"}.",
            "api-reply-to-comment",
        )
    })?;
    validate_comment_text(&input.text)?;
    let artifact = ReviewArtifactId::parse(&input.artifact_id)?;
    let endpoint = artifact.reply_endpoint();
    let response: serde_json::Value =
        gh_api_post_body(&state.config, &endpoint, &input.text).await?;
    Ok(ReplyToCommentOutput {
        artifact_id: input.artifact_id,
        status: "posted".into(),
        url: response
            .get("html_url")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned),
        trace_id: ctx.trace_id.to_string(),
    })
}

async fn mark_review_artifact_handled(
    payload: &[u8],
    state: &RepoState,
    ctx: &TraceCtx,
    nats: &JamNats,
) -> Result<MarkReviewArtifactHandledOutput, RepoError> {
    let input: MarkReviewArtifactHandledInput = serde_json::from_slice(payload).map_err(|err| {
        RepoError::protocol(
            "invalid-input",
            format!("tool.repo.mark-review-artifact-handled payload is invalid JSON: {err}"),
            "Send artifact_id, status, and reasoning.",
            "api-mark-review-artifact-handled",
        )
    })?;
    validate_handled_status(&input.status)?;
    validate_comment_text(&input.reasoning)?;
    ReviewArtifactId::parse(&input.artifact_id)?;
    let handled_at = Utc::now();
    let output = MarkReviewArtifactHandledOutput {
        artifact_id: input.artifact_id,
        status: input.status,
        handled_at,
        trace_id: ctx.trace_id.to_string(),
    };
    append_handled_artifact(&state.config, &output, &input.reasoning)?;
    nats.publish_traced("journal.review-artifact.handled", &output, ctx)
        .await
        .map_err(|err| {
            RepoError::protocol(
                "journal-publish-failed",
                err.to_string(),
                "Verify NATS is running and the journal bridge is healthy.",
                "principle-failure-surfaces-immediately",
            )
        })?;
    Ok(output)
}

async fn request_review(
    payload: &[u8],
    state: &RepoState,
    ctx: &TraceCtx,
    nats: &JamNats,
) -> Result<RequestReviewOutput, RepoError> {
    let input: RequestReviewInput = serde_json::from_slice(payload).map_err(|err| {
        RepoError::protocol(
            "invalid-input",
            format!("tool.repo.request-review payload is invalid JSON: {err}"),
            "Send pr_ref, reviewer_id, worktree_path, and optional base/task_id.",
            "api-request-review",
        )
    })?;
    if input.reviewer_id != "codex-review" {
        return Err(RepoError::protocol(
            "unsupported-reviewer",
            format!(
                "reviewer_id {:?} is not available in this local slice",
                input.reviewer_id
            ),
            "Use reviewer_id=codex-review, or enable the corresponding reviewer adapter first.",
            "api-request-review",
        ));
    }
    let selector = PrApiSelector::parse(&input.pr_ref, input.repo.as_deref(), &state.config)?;
    let worktree = input
        .worktree_path
        .as_deref()
        .ok_or_else(|| {
            RepoError::protocol(
                "missing-worktree-path",
                "codex-review requires worktree_path so the service cannot review the wrong tree",
                "Pass the Picker worktree path for the PR branch.",
                "api-request-review",
            )
        })
        .and_then(validate_worktree_path)?;
    let base = input
        .base
        .unwrap_or_else(|| state.config.default_base.clone());
    validate_branch("base", &base)?;

    let review = run_codex_review(&state.config, &worktree, &base).await?;
    let artifacts = if review.trim().is_empty() {
        Vec::new()
    } else {
        vec![codex_review_artifact(&selector, ctx, review)]
    };
    if !artifacts.is_empty() {
        publish_review_received(
            nats,
            input.task_id.as_deref().unwrap_or(&selector.pr_ref),
            &selector.pr_ref,
            &input.reviewer_id,
            artifacts.len(),
            ctx,
        )
        .await?;
    }
    Ok(RequestReviewOutput {
        pr_ref: selector.pr_ref,
        reviewer_id: input.reviewer_id,
        status: "completed".into(),
        artifacts,
        trace_id: ctx.trace_id.to_string(),
    })
}

async fn prepare_merge(
    payload: &[u8],
    state: &RepoState,
    ctx: &TraceCtx,
) -> Result<PrepareMergeOutput, RepoError> {
    let input: PrepareMergeInput = serde_json::from_slice(payload).map_err(|err| {
        RepoError::protocol(
            "invalid-input",
            format!("tool.repo.prepare-merge payload is invalid JSON: {err}"),
            "Send {\"pr_ref\":\"owner/repo#123\"} or a GitHub PR URL.",
            "api-prepare-merge",
        )
    })?;
    let selector = PrSelector::parse(&input.pr_ref, input.repo.as_deref(), &state.config)?;
    prepare_merge_selector(&state.config, &selector, ctx).await
}

async fn request_human_merge(
    payload: &[u8],
    state: &RepoState,
    ctx: &TraceCtx,
    nats: &JamNats,
) -> Result<RequestHumanMergeOutput, RepoError> {
    let input: RequestHumanMergeInput = serde_json::from_slice(payload).map_err(|err| {
        RepoError::protocol(
            "invalid-input",
            format!("tool.repo.request-human-merge payload is invalid JSON: {err}"),
            "Send {\"pr_ref\":\"owner/repo#123\",\"summary\":\"...\"}.",
            "api-request-human-merge",
        )
    })?;
    validate_comment_text(&input.summary)?;
    let selector = PrSelector::parse(&input.pr_ref, input.repo.as_deref(), &state.config)?;
    let prepare = prepare_merge_selector(&state.config, &selector, ctx).await?;
    let notification = serde_json::json!({
        "urgency": if prepare.ready { "high" } else { "medium" },
        "summary": format!("Human merge requested for {}", prepare.pr_ref),
        "payload": {
            "pr_ref": &prepare.pr_ref,
            "url": &prepare.url,
            "ready": prepare.ready,
            "checks_passed": prepare.checks_passed,
            "merge_state_status": &prepare.merge_state_status,
            "review_decision": &prepare.review_decision,
            "manager_summary": &input.summary,
            "trace_id": ctx.trace_id.to_string(),
        },
    });
    let response: serde_json::Value = nats
        .request_traced(
            state.config.notify_human_subject.clone(),
            &notification,
            ctx,
            state.config.timeout,
        )
        .await
        .map_err(|err| {
            RepoError::protocol(
                "notify-human-request-failed",
                err.to_string(),
                "Verify jam-svc-supervise is running and subscribed to tool.supervise.notify-human.",
                "api-request-human-merge",
            )
        })?;
    if let Some(error) = response.get("error") {
        return Err(RepoError::protocol(
            "notify-human-failed",
            error.to_string(),
            "Fix the notification service error, then retry request-human-merge.",
            "api-request-human-merge",
        ));
    }
    Ok(RequestHumanMergeOutput {
        pr_ref: prepare.pr_ref.clone(),
        status: "notification-requested".into(),
        notification_subject: state.config.notify_human_subject.clone(),
        notification_response: response,
        prepare,
        trace_id: ctx.trace_id.to_string(),
    })
}

async fn pr_status_selector(
    config: &RepoConfig,
    selector: &PrSelector,
    ctx: &TraceCtx,
) -> Result<PrStatusOutput, RepoError> {
    let mut command = Command::new(&config.gh_bin);
    command.arg("pr");
    command.arg("view");
    command.arg(&selector.selector);
    if let Some(repo) = &selector.repo {
        command.arg("--repo");
        command.arg(repo);
    }
    command.arg("--json");
    command.arg("number,url,state,title,headRefName,isDraft");
    let output = run_command(command, config.timeout, "gh-pr-view").await?;
    let view: GhPrView = serde_json::from_slice(&output.stdout).map_err(|err| {
        RepoError::protocol(
            "gh-output-invalid",
            format!("gh pr view returned invalid JSON: {err}"),
            "Upgrade gh or update jam-svc-repo's parser.",
            "api-pr-status",
        )
    })?;
    let pr_ref = selector
        .pr_ref
        .clone()
        .or_else(|| pr_ref_from_url(&view.url))
        .unwrap_or_else(|| {
            format!(
                "{}#{}",
                selector.repo.as_deref().unwrap_or("unknown/repo"),
                view.number
            )
        });
    Ok(PrStatusOutput {
        pr_ref,
        url: view.url,
        number: view.number,
        state: view.state.to_ascii_lowercase(),
        title: view.title,
        branch: view.head_ref_name,
        draft: view.is_draft,
        trace_id: ctx.trace_id.to_string(),
    })
}

async fn prepare_merge_selector(
    config: &RepoConfig,
    selector: &PrSelector,
    ctx: &TraceCtx,
) -> Result<PrepareMergeOutput, RepoError> {
    let view = gh_prepare_merge_view(config, selector).await?;
    let checks = gh_pr_checks(config, selector)
        .await?
        .into_iter()
        .map(merge_check_output)
        .collect::<Vec<_>>();
    let checks_passed = checks.iter().all(|check| check.passed);
    let merge_state_ok = view
        .merge_state_status
        .as_deref()
        .is_none_or(merge_state_allows_merge);
    let review_ok = view
        .review_decision
        .as_deref()
        .is_none_or(review_decision_allows_merge);
    let ready = view.state.eq_ignore_ascii_case("open")
        && !view.is_draft
        && checks_passed
        && merge_state_ok
        && review_ok;
    let pr_ref = selector
        .pr_ref
        .clone()
        .or_else(|| pr_ref_from_url(&view.url))
        .unwrap_or_else(|| {
            format!(
                "{}#{}",
                selector.repo.as_deref().unwrap_or(&config.github_repo),
                view.number
            )
        });
    Ok(PrepareMergeOutput {
        pr_ref,
        url: view.url,
        state: view.state.to_ascii_lowercase(),
        title: view.title,
        branch: view.head_ref_name,
        draft: view.is_draft,
        merge_state_status: view.merge_state_status,
        review_decision: view.review_decision,
        checks_passed,
        ready,
        checks,
        trace_id: ctx.trace_id.to_string(),
    })
}

async fn gh_prepare_merge_view(
    config: &RepoConfig,
    selector: &PrSelector,
) -> Result<GhPrepareMergeView, RepoError> {
    let mut command = Command::new(&config.gh_bin);
    command.arg("pr");
    command.arg("view");
    command.arg(&selector.selector);
    if let Some(repo) = &selector.repo {
        command.arg("--repo");
        command.arg(repo);
    }
    command.arg("--json");
    command.arg("number,url,state,title,headRefName,isDraft,mergeStateStatus,reviewDecision");
    if let Some(token) = github_app_installation_token(config).await? {
        command.env("GH_TOKEN", token.expose_secret());
    }
    let output = run_command(command, config.timeout, "gh-pr-prepare-view").await?;
    serde_json::from_slice(&output.stdout).map_err(|err| {
        RepoError::protocol(
            "gh-output-invalid",
            format!("gh pr view returned invalid merge-preflight JSON: {err}"),
            "Upgrade gh or update jam-svc-repo's prepare-merge parser.",
            "api-prepare-merge",
        )
    })
}

async fn gh_pr_checks(
    config: &RepoConfig,
    selector: &PrSelector,
) -> Result<Vec<GhPrCheck>, RepoError> {
    let mut command = Command::new(&config.gh_bin);
    command.arg("pr");
    command.arg("checks");
    command.arg(&selector.selector);
    if let Some(repo) = &selector.repo {
        command.arg("--repo");
        command.arg(repo);
    }
    command.arg("--json");
    command.arg("name,state,conclusion,bucket,link,description");
    if let Some(token) = github_app_installation_token(config).await? {
        command.env("GH_TOKEN", token.expose_secret());
    }
    let output = run_command(command, config.timeout, "gh-pr-checks").await?;
    serde_json::from_slice(&output.stdout).map_err(|err| {
        RepoError::protocol(
            "gh-output-invalid",
            format!("gh pr checks returned invalid JSON: {err}"),
            "Upgrade gh or update jam-svc-repo's checks parser.",
            "api-prepare-merge",
        )
    })
}

fn merge_check_output(check: GhPrCheck) -> MergeCheckOutput {
    let passed = merge_check_passed(&check);
    MergeCheckOutput {
        name: check.name,
        state: check.state,
        conclusion: check.conclusion,
        bucket: check.bucket,
        passed,
        link: check.link,
        description: check.description,
    }
}

fn merge_check_passed(check: &GhPrCheck) -> bool {
    let tokens = [
        check.state.as_deref(),
        check.conclusion.as_deref(),
        check.bucket.as_deref(),
    ];
    let mut saw_passing = false;
    for token in tokens.into_iter().flatten() {
        match token.to_ascii_uppercase().as_str() {
            "FAILURE" | "FAILED" | "ERROR" | "CANCELLED" | "TIMED_OUT" | "ACTION_REQUIRED"
            | "PENDING" | "QUEUED" | "IN_PROGRESS" | "EXPECTED" => return false,
            "SUCCESS" | "SUCCESSFUL" | "PASSING" | "PASSED" | "NEUTRAL" | "SKIPPED" => {
                saw_passing = true;
            }
            _ => {}
        }
    }
    saw_passing
}

fn merge_state_allows_merge(state: &str) -> bool {
    matches!(state.to_ascii_uppercase().as_str(), "CLEAN" | "HAS_HOOKS")
}

fn review_decision_allows_merge(decision: &str) -> bool {
    !matches!(
        decision.to_ascii_uppercase().as_str(),
        "CHANGES_REQUESTED" | "REVIEW_REQUIRED"
    )
}

async fn gh_pr_create(
    config: &RepoConfig,
    repo: &str,
    base: &str,
    input: &OpenPrInput,
    draft: bool,
    worktree_path: Option<&Path>,
) -> Result<(String, TokenKind), RepoError> {
    let mut command = Command::new(&config.gh_bin);
    command.arg("pr");
    command.arg("create");
    command.arg("--repo");
    command.arg(repo);
    command.arg("--base");
    command.arg(base);
    command.arg("--head");
    command.arg(&input.branch);
    command.arg("--title");
    command.arg(&input.title);
    command.arg("--body");
    command.arg(input.body.as_deref().unwrap_or(""));
    if draft {
        command.arg("--draft");
    }
    if let Some(path) = worktree_path {
        command.current_dir(path);
    }
    let token_kind = if let Some((token, kind)) = resolve_write_token(config).await? {
        command.env("GH_TOKEN", token.expose_secret());
        kind
    } else {
        // Treat unauthenticated `gh` as an Installation-style fallback: the
        // user-attribution gate is off, so reviewer bots will skip. This is
        // the same behavior as the previous code path.
        TokenKind::Installation
    };
    let output = run_command(command, config.timeout, "gh-pr-create").await?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let url = stdout
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with("https://github.com/") && line.contains("/pull/"))
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            RepoError::protocol(
                "gh-output-invalid",
                format!("gh pr create did not print a PR URL: {}", stdout.trim()),
                "Verify gh pr create completed successfully and printed the created PR URL.",
                "api-open-pr",
            )
        })?;
    Ok((url, token_kind))
}

async fn gh_api_json<T>(config: &RepoConfig, endpoint: &str) -> Result<T, RepoError>
where
    T: for<'de> Deserialize<'de>,
{
    let mut command = Command::new(&config.gh_bin);
    command.arg("api");
    command.arg(endpoint);
    if let Some(token) = github_app_installation_token(config).await? {
        command.env("GH_TOKEN", token.expose_secret());
    }
    let output = run_command(command, config.timeout, "gh-api").await?;
    serde_json::from_slice(&output.stdout).map_err(|err| {
        RepoError::protocol(
            "gh-output-invalid",
            format!("gh api {endpoint} returned invalid JSON: {err}"),
            "Upgrade gh or update jam-svc-repo's parser.",
            "comp-jam-svc-repo",
        )
    })
}

async fn gh_api_post_body(
    config: &RepoConfig,
    endpoint: &str,
    body: &str,
) -> Result<serde_json::Value, RepoError> {
    let mut command = Command::new(&config.gh_bin);
    command.arg("api");
    command.arg("--method");
    command.arg("POST");
    command.arg(endpoint);
    command.arg("-f");
    command.arg(format!("body={body}"));
    if let Some((token, _)) = resolve_write_token(config).await? {
        command.env("GH_TOKEN", token.expose_secret());
    }
    let output = run_command(command, config.timeout, "gh-api-post").await?;
    serde_json::from_slice(&output.stdout).map_err(|err| {
        RepoError::protocol(
            "gh-output-invalid",
            format!("gh api POST {endpoint} returned invalid JSON: {err}"),
            "Upgrade gh or update jam-svc-repo's parser.",
            "comp-jam-svc-repo",
        )
    })
}

async fn run_codex_review(
    config: &RepoConfig,
    worktree: &Path,
    base: &str,
) -> Result<String, RepoError> {
    let mut command = Command::new(&config.codex_bin);
    command.arg("-C");
    command.arg(worktree);
    command.arg("review");
    command.arg("--base");
    command.arg(base);
    let output = run_command(command, config.timeout, "codex-review").await?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

async fn git_push(
    config: &RepoConfig,
    worktree_path: &Path,
    branch: &str,
) -> Result<(), RepoError> {
    // HTTPS auth with `x-access-token:<token>` works for both
    // installation tokens (ghs_*) and user-to-server tokens (ghu_*); the
    // credential helper reads the same env var either way.
    let token = resolve_write_token(config).await?.map(|(t, _)| t);
    let mut command = Command::new(&config.git_bin);
    command.arg("-C");
    command.arg(worktree_path);
    command.env("GIT_TERMINAL_PROMPT", "0");
    if let Some(token) = token {
        command.arg("-c");
        command.arg("credential.helper=");
        command.arg("-c");
        command.arg(format!("credential.helper={GIT_APP_CREDENTIAL_HELPER}"));
        command.env(GIT_APP_TOKEN_ENV, token.expose_secret());
    }
    command.arg("push");
    command.arg("-u");
    command.arg("origin");
    command.arg(branch);
    run_command(command, config.timeout, "git-push").await?;
    Ok(())
}

async fn publish_pr_opened(
    nats: &JamNats,
    output: &OpenPrOutput,
    ctx: &TraceCtx,
) -> Result<(), RepoError> {
    let payload = PrOpened {
        task_id: output.task_id.clone(),
        pr_ref: output.pr_ref.clone(),
        branch: output.branch.clone(),
        title: output.title.clone(),
        draft: output.draft,
        opened_at: output.opened_at,
    };
    let envelope = EventEnvelope::new(
        PrOpened::EVENT_TYPE,
        PrOpened::EVENT_SUBTYPE_VERSION,
        0,
        ctx.trace_id.to_string(),
        SERVICE_NAME,
        payload,
    );
    nats.publish_traced("journal.pr.opened", &envelope, ctx)
        .await
        .map_err(|err| {
            RepoError::protocol(
                "journal-publish-failed",
                err.to_string(),
                "Verify NATS is running and the journal bridge is healthy.",
                "principle-failure-surfaces-immediately",
            )
        })
}

async fn publish_review_received(
    nats: &JamNats,
    task_id: &str,
    pr_ref: &str,
    reviewer: &str,
    artifact_count: usize,
    ctx: &TraceCtx,
) -> Result<(), RepoError> {
    let payload = PrReviewReceived {
        task_id: task_id.to_owned(),
        pr_ref: pr_ref.to_owned(),
        reviewer: reviewer.to_owned(),
        artifact_count: u32::try_from(artifact_count).unwrap_or(u32::MAX),
        received_at: Utc::now(),
    };
    let envelope = EventEnvelope::new(
        PrReviewReceived::EVENT_TYPE,
        PrReviewReceived::EVENT_SUBTYPE_VERSION,
        0,
        ctx.trace_id.to_string(),
        SERVICE_NAME,
        payload,
    );
    nats.publish_traced("journal.pr.review-received", &envelope, ctx)
        .await
        .map_err(|err| {
            RepoError::protocol(
                "journal-publish-failed",
                err.to_string(),
                "Verify NATS is running and the journal bridge is healthy.",
                "principle-failure-surfaces-immediately",
            )
        })
}

struct CommandOutput {
    stdout: Vec<u8>,
}

async fn run_command(
    mut command: Command,
    timeout: Duration,
    kind: &'static str,
) -> Result<CommandOutput, RepoError> {
    let output = tokio::time::timeout(timeout, command.output())
        .await
        .map_err(|_| {
            RepoError::protocol(
                "command-timeout",
                format!("{kind} exceeded {}s", timeout.as_secs()),
                "Check network connectivity and GitHub CLI authentication.",
                "principle-failure-surfaces-immediately",
            )
        })?
        .map_err(|err| {
            RepoError::protocol(
                "command-exec-failed",
                format!("failed to run {kind}: {err}"),
                "Verify git and gh are installed for the service user.",
                "principle-failure-surfaces-immediately",
            )
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        return Err(RepoError::protocol(
            "command-failed",
            format!(
                "{kind} failed: {}",
                if stderr.is_empty() { stdout } else { stderr }
            ),
            "Fix the command failure and retry the tool call.",
            "principle-failure-surfaces-immediately",
        ));
    }
    Ok(CommandOutput {
        stdout: output.stdout,
    })
}

struct PrSelector {
    selector: String,
    repo: Option<String>,
    pr_ref: Option<String>,
}

impl PrSelector {
    fn parse(
        raw: &str,
        repo_override: Option<&str>,
        config: &RepoConfig,
    ) -> Result<Self, RepoError> {
        if raw.trim().is_empty() {
            return Err(RepoError::protocol(
                "invalid-pr-ref",
                "pr_ref must not be empty",
                "Send owner/repo#number, a GitHub PR URL, or a branch/number selector.",
                "api-pr-status",
            ));
        }
        if let Some((repo, number)) = raw.rsplit_once('#') {
            validate_repo(repo)?;
            validate_pr_number(number)?;
            return Ok(Self {
                selector: number.to_owned(),
                repo: Some(repo.to_owned()),
                pr_ref: Some(raw.to_owned()),
            });
        }
        if raw.starts_with("https://github.com/") {
            return Ok(Self {
                selector: raw.to_owned(),
                repo: repo_override.map(ToOwned::to_owned),
                pr_ref: pr_ref_from_url(raw),
            });
        }
        Ok(Self {
            selector: raw.to_owned(),
            repo: Some(repo_override.unwrap_or(&config.github_repo).to_owned()),
            pr_ref: None,
        })
    }
}

struct PrApiSelector {
    repo: String,
    number: u64,
    pr_ref: String,
}

impl PrApiSelector {
    fn parse(
        raw: &str,
        repo_override: Option<&str>,
        config: &RepoConfig,
    ) -> Result<Self, RepoError> {
        let selector = PrSelector::parse(raw, repo_override, config)?;
        if let Some(pr_ref) = selector.pr_ref {
            return Self::from_pr_ref(&pr_ref);
        }
        let repo = selector
            .repo
            .clone()
            .unwrap_or_else(|| config.github_repo.clone());
        validate_repo(&repo)?;
        let number = selector.selector.parse::<u64>().map_err(|err| {
            RepoError::protocol(
                "invalid-pr-ref",
                format!(
                    "read/reply PR comment tools require a PR number, got {:?}: {err}",
                    selector.selector
                ),
                "Send owner/repo#number or a GitHub PR URL.",
                "api-read-pr-comments",
            )
        })?;
        Ok(Self {
            pr_ref: format!("{repo}#{number}"),
            repo,
            number,
        })
    }

    fn from_pr_ref(pr_ref: &str) -> Result<Self, RepoError> {
        let Some((repo, number)) = pr_ref.rsplit_once('#') else {
            return Err(RepoError::protocol(
                "invalid-pr-ref",
                format!("expected owner/repo#number, got {pr_ref}"),
                "Send owner/repo#number or a GitHub PR URL.",
                "api-read-pr-comments",
            ));
        };
        validate_repo(repo)?;
        let number = number.parse::<u64>().map_err(|err| {
            RepoError::protocol(
                "invalid-pr-ref",
                format!("PR number must be decimal digits in {pr_ref}: {err}"),
                "Send owner/repo#number or a GitHub PR URL.",
                "api-read-pr-comments",
            )
        })?;
        Ok(Self {
            repo: repo.to_owned(),
            number,
            pr_ref: pr_ref.to_owned(),
        })
    }
}

enum ReviewArtifactId {
    IssueComment {
        selector: PrApiSelector,
        _comment_id: u64,
    },
    ReviewComment {
        selector: PrApiSelector,
        comment_id: u64,
    },
    Review {
        selector: PrApiSelector,
        _review_id: u64,
    },
}

impl ReviewArtifactId {
    fn parse(raw: &str) -> Result<Self, RepoError> {
        let parts: Vec<&str> = raw.split(':').collect();
        if parts.len() != 3 {
            return Err(RepoError::protocol(
                "invalid-artifact-id",
                format!("artifact id must be kind:owner/repo#number:id, got {raw}"),
                "Use an artifact id returned by read-pr-comments.",
                "api-reply-to-comment",
            ));
        }
        let selector = PrApiSelector::from_pr_ref(parts[1])?;
        let comment_id = parts[2].parse::<u64>().map_err(|err| {
            RepoError::protocol(
                "invalid-artifact-id",
                format!("artifact id comment/review id must be decimal digits: {err}"),
                "Use an artifact id returned by read-pr-comments.",
                "api-reply-to-comment",
            )
        })?;
        match parts[0] {
            "github-issue-comment" => Ok(Self::IssueComment {
                selector,
                _comment_id: comment_id,
            }),
            "github-review-comment" => Ok(Self::ReviewComment {
                selector,
                comment_id,
            }),
            "github-review" => Ok(Self::Review {
                selector,
                _review_id: comment_id,
            }),
            _ => Err(RepoError::protocol(
                "invalid-artifact-id",
                format!("unknown review artifact kind in {raw}"),
                "Use an artifact id returned by read-pr-comments.",
                "api-reply-to-comment",
            )),
        }
    }

    fn reply_endpoint(&self) -> String {
        match self {
            Self::IssueComment { selector, .. } | Self::Review { selector, .. } => {
                format!(
                    "repos/{}/issues/{}/comments",
                    selector.repo, selector.number
                )
            }
            Self::ReviewComment {
                selector,
                comment_id,
            } => {
                format!(
                    "repos/{}/pulls/comments/{comment_id}/replies",
                    selector.repo
                )
            }
        }
    }
}

fn issue_comment_artifact(
    selector: &PrApiSelector,
    comment: GhIssueComment,
) -> ReviewArtifactOutput {
    let untrusted_body = Untrusted::new(comment.body.unwrap_or_default());
    ReviewArtifactOutput {
        id: format!("github-issue-comment:{}:{}", selector.pr_ref, comment.id),
        reviewer: comment
            .user
            .and_then(|user| user.login)
            .unwrap_or_else(|| "github".into()),
        kind: "issue-comment",
        status: "Open",
        body: untrusted_body.as_ref_for_analysis().to_owned(),
        body_trust: "untrusted",
        url: comment.html_url,
        path: None,
        line: None,
        created_at: comment.created_at,
    }
}

fn review_comment_artifact(
    selector: &PrApiSelector,
    comment: GhReviewComment,
) -> ReviewArtifactOutput {
    let untrusted_body = Untrusted::new(comment.body.unwrap_or_default());
    ReviewArtifactOutput {
        id: format!("github-review-comment:{}:{}", selector.pr_ref, comment.id),
        reviewer: comment
            .user
            .and_then(|user| user.login)
            .unwrap_or_else(|| "github".into()),
        kind: "review-comment",
        status: "Open",
        body: untrusted_body.as_ref_for_analysis().to_owned(),
        body_trust: "untrusted",
        url: comment.html_url,
        path: comment.path,
        line: comment.line.or(comment.original_line),
        created_at: comment.created_at,
    }
}

fn review_artifact(selector: &PrApiSelector, review: GhReview) -> ReviewArtifactOutput {
    let untrusted_body = Untrusted::new(review.body.unwrap_or_default());
    ReviewArtifactOutput {
        id: format!("github-review:{}:{}", selector.pr_ref, review.id),
        reviewer: review
            .user
            .and_then(|user| user.login)
            .unwrap_or_else(|| "github".into()),
        kind: "review",
        status: "Open",
        body: untrusted_body.as_ref_for_analysis().to_owned(),
        body_trust: "untrusted",
        url: review.html_url,
        path: None,
        line: None,
        created_at: review.submitted_at,
    }
}

fn codex_review_artifact(
    selector: &PrApiSelector,
    ctx: &TraceCtx,
    body: String,
) -> ReviewArtifactOutput {
    let untrusted_body = Untrusted::new(body);
    let trace = ctx.trace_id.to_string();
    ReviewArtifactOutput {
        id: format!(
            "codex-review:{}:{}",
            selector.pr_ref,
            &trace[trace.len() - 8..]
        ),
        reviewer: "codex-review".into(),
        kind: "review-summary",
        status: "Open",
        body: untrusted_body.as_ref_for_analysis().to_owned(),
        body_trust: "untrusted",
        url: None,
        path: None,
        line: None,
        created_at: Some(Utc::now()),
    }
}

fn parse_open_pr_input(payload: &[u8]) -> Result<OpenPrInput, RepoError> {
    serde_json::from_slice(payload).map_err(|err| {
        RepoError::protocol(
            "invalid-input",
            format!("tool.repo.open-pr payload is invalid JSON: {err}"),
            "Send a JSON object with task_id, branch, title, and optional body/draft/base/worktree_path.",
            "api-open-pr",
        )
    })
}

fn validate_worktree_path(path: &str) -> Result<PathBuf, RepoError> {
    let raw = PathBuf::from(path);
    if !raw.is_absolute() || is_windows_mount(&raw) {
        return Err(RepoError::protocol(
            "invalid-worktree-path",
            format!(
                "worktree path must be native Linux absolute path: {}",
                raw.display()
            ),
            "Pass a Linux-native Picker worktree path.",
            "principle-native-fs-only",
        ));
    }
    if raw.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::CurDir | Component::Prefix(_)
        )
    }) {
        return Err(RepoError::protocol(
            "invalid-worktree-path",
            format!(
                "worktree path contains unsafe components: {}",
                raw.display()
            ),
            "Pass a canonical Picker worktree path.",
            "principle-native-fs-only",
        ));
    }
    raw.canonicalize().map_err(|err| {
        RepoError::protocol(
            "worktree-not-found",
            format!("failed to canonicalize {}: {err}", raw.display()),
            "Verify the Picker worktree still exists.",
            "api-open-pr",
        )
    })
}

fn validate_title(title: &str) -> Result<(), RepoError> {
    if title.trim().is_empty() || title.len() > TITLE_MAX_LEN {
        return Err(RepoError::protocol(
            "invalid-title",
            format!("title must be 1-{TITLE_MAX_LEN} characters"),
            "Send a concise PR title.",
            "api-open-pr",
        ));
    }
    Ok(())
}

fn format_jam_pr_title(raw: &str) -> Result<String, RepoError> {
    let title = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    let title = title
        .strip_prefix("[jam]")
        .or_else(|| title.strip_prefix("[JAM]"))
        .map(str::trim)
        .unwrap_or(title.as_str());
    if title.is_empty() {
        return Err(RepoError::protocol(
            "invalid-title",
            "PR title must describe the change after the [jam] prefix",
            "Send a concise PR title focused on what the PR changes.",
            "api-open-pr",
        ));
    }
    Ok(format!("[jam] {title}"))
}

fn validate_comment_text(text: &str) -> Result<(), RepoError> {
    if text.trim().is_empty() || text.len() > COMMENT_MAX_LEN || text.contains('\0') {
        return Err(RepoError::protocol(
            "invalid-comment-text",
            format!("comment text must be 1-{COMMENT_MAX_LEN} characters and contain no NUL"),
            "Send concise plain text.",
            "api-reply-to-comment",
        ));
    }
    Ok(())
}

fn validate_handled_status(status: &str) -> Result<(), RepoError> {
    if matches!(status, "Open" | "Acknowledged" | "Addressed" | "Dismissed") {
        return Ok(());
    }
    Err(RepoError::protocol(
        "invalid-status",
        format!("unsupported review artifact status: {status}"),
        "Use Open, Acknowledged, Addressed, or Dismissed.",
        "api-mark-review-artifact-handled",
    ))
}

fn append_handled_artifact(
    config: &RepoConfig,
    output: &MarkReviewArtifactHandledOutput,
    reasoning: &str,
) -> Result<(), RepoError> {
    let path = &config.handled_artifacts_path;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| {
            RepoError::protocol(
                "review-artifact-state-write-failed",
                format!("create {}: {err}", parent.display()),
                "Verify JAM_REVIEW_ARTIFACT_STATE_PATH is writable.",
                "api-mark-review-artifact-handled",
            )
        })?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|err| {
            RepoError::protocol(
                "review-artifact-state-write-failed",
                format!("open {}: {err}", path.display()),
                "Verify JAM_REVIEW_ARTIFACT_STATE_PATH is writable.",
                "api-mark-review-artifact-handled",
            )
        })?;
    let line = serde_json::to_string(&serde_json::json!({
        "artifact_id": output.artifact_id,
        "status": output.status,
        "reasoning": reasoning,
        "handled_at": output.handled_at,
        "trace_id": output.trace_id,
    }))
    .expect("handled artifact serializes");
    writeln!(file, "{line}").map_err(|err| {
        RepoError::protocol(
            "review-artifact-state-write-failed",
            format!("write {}: {err}", path.display()),
            "Verify JAM_REVIEW_ARTIFACT_STATE_PATH is writable.",
            "api-mark-review-artifact-handled",
        )
    })
}

fn validate_token(name: &'static str, value: &str, max_len: usize) -> Result<(), RepoError> {
    if value.is_empty() || value.len() > max_len {
        return Err(RepoError::protocol(
            "invalid-token",
            format!("{name} must be 1-{max_len} characters"),
            "Use slug-like values with letters, numbers, dots, underscores, and dashes.",
            "api-open-pr",
        ));
    }
    if value == "." || value == ".." || value.contains("..") {
        return Err(RepoError::protocol(
            "invalid-token",
            format!("{name} may not contain parent-directory segments: {value}"),
            "Use slug-like values with letters, numbers, dots, underscores, and dashes.",
            "api-open-pr",
        ));
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(RepoError::protocol(
            "invalid-token",
            format!("{name} contains unsafe characters: {value}"),
            "Use slug-like values with letters, numbers, dots, underscores, and dashes.",
            "api-open-pr",
        ));
    }
    Ok(())
}

fn validate_branch(name: &'static str, branch: &str) -> Result<(), RepoError> {
    if branch.is_empty()
        || branch.len() > TOKEN_MAX_LEN
        || branch.starts_with('-')
        || branch.starts_with('/')
        || branch.ends_with('/')
        || branch.contains("..")
        || branch.contains("//")
        || branch.contains('@')
        || branch.contains('\\')
    {
        return Err(RepoError::protocol(
            "invalid-branch",
            format!("{name} is not a safe branch name: {branch}"),
            "Use a local branch name like task/<task-id>.",
            "api-open-pr",
        ));
    }
    if !branch
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b'/'))
    {
        return Err(RepoError::protocol(
            "invalid-branch",
            format!("{name} contains unsafe characters: {branch}"),
            "Use a local branch name like task/<task-id>.",
            "api-open-pr",
        ));
    }
    Ok(())
}

fn validate_repo(repo: &str) -> Result<(), RepoError> {
    let Some((owner, name)) = repo.split_once('/') else {
        return Err(RepoError::protocol(
            "invalid-repo",
            format!("repo must be owner/name, got {repo}"),
            "Set JAM_GITHUB_REPO to the Blueberry repository.",
            "dec-single-project-per-instance",
        ));
    };
    validate_repo_part("repo owner", owner)?;
    validate_repo_part("repo name", name)?;
    Ok(())
}

fn validate_repo_part(name: &'static str, value: &str) -> Result<(), RepoError> {
    if value.is_empty()
        || value.starts_with('-')
        || value.ends_with('-')
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(RepoError::protocol(
            "invalid-repo",
            format!("{name} is unsafe: {value}"),
            "Set JAM_GITHUB_REPO to the Blueberry repository.",
            "dec-single-project-per-instance",
        ));
    }
    Ok(())
}

fn validate_pr_number(number: &str) -> Result<(), RepoError> {
    if number.is_empty() || !number.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(RepoError::protocol(
            "invalid-pr-ref",
            format!("PR number must be decimal digits, got {number}"),
            "Send owner/repo#number or a GitHub PR URL.",
            "api-pr-status",
        ));
    }
    Ok(())
}

fn pr_ref_from_url(url: &str) -> Option<String> {
    let prefix = "https://github.com/";
    let rest = url.strip_prefix(prefix)?;
    let mut parts = rest.split('/');
    let owner = parts.next()?;
    let repo = parts.next()?;
    if parts.next()? != "pull" {
        return None;
    }
    let number = parts.next()?.trim_end_matches('/');
    if number.is_empty() || !number.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    Some(format!("{owner}/{repo}#{number}"))
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

fn error_response(err: RepoError) -> Response {
    match err {
        RepoError::Protocol {
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

fn first_env(keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        std::env::var(key)
            .ok()
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
    })
}

fn first_secret(
    env_keys: &[&str],
    file_key: &'static str,
    pass_key: &'static str,
) -> Result<Option<String>, RepoError> {
    if let Some(value) = first_env(env_keys) {
        return Ok(Some(value));
    }
    read_secret_key(file_key, pass_key)
}

fn read_secret_key(
    file_key: &'static str,
    pass_key: &'static str,
) -> Result<Option<String>, RepoError> {
    if let Some(path) = std::env::var_os("JAM_SECRETS_FILE").filter(|value| !value.is_empty()) {
        let backend = FileBackend::new(path);
        match backend.get(&SecretKey::new(file_key)) {
            Ok(secret) => return Ok(Some(secret.expose_secret().trim_end().to_owned())),
            Err(SecretError::NotFound(_)) => {}
            Err(err) => {
                return Err(RepoError::protocol(
                    "invalid-github-app-config",
                    format!("failed reading {file_key} from JAM_SECRETS_FILE: {err}"),
                    "Fix the configured secrets file or unset JAM_SECRETS_FILE and use maestro pass/env config.",
                    "task-github-app-registration",
                ));
            }
        }
    }
    let backend = PassBackend::new("jam");
    match backend.get(&SecretKey::new(pass_key)) {
        Ok(secret) => Ok(Some(secret.expose_secret().trim_end().to_owned())),
        Err(_) => Ok(None),
    }
}

fn read_optional_file_env(key: &str) -> Option<String> {
    let path = std::env::var_os(key).filter(|value| !value.is_empty())?;
    std::fs::read_to_string(path).ok()
}

fn parse_required_u64(name: &'static str, value: Option<String>) -> Result<u64, RepoError> {
    let value = value.ok_or_else(|| {
        RepoError::protocol(
            "invalid-github-app-config",
            format!("{name} is required when GitHub App config is partially set"),
            "Set all of JAM_GITHUB_APP_ID, JAM_GITHUB_APP_INSTALLATION_ID, and JAM_GITHUB_APP_PRIVATE_KEY, or seed jam/pickers/github-app-id, jam/pickers/github-app-installation-id, and jam/pickers/github-app-key.",
            "task-github-app-registration",
        )
    })?;
    value.parse::<u64>().map_err(|err| {
        RepoError::protocol(
            "invalid-github-app-config",
            format!("{name}={value:?} is not an unsigned integer: {err}"),
            "Use the numeric GitHub App and installation IDs from the App settings.",
            "task-github-app-registration",
        )
    })
}

fn default_jam_home() -> PathBuf {
    if let Some(jam_home) = std::env::var_os("JAM_HOME") {
        return PathBuf::from(jam_home);
    }
    std::env::var_os("HOME").map_or_else(
        || PathBuf::from("/home/maestro/.jam"),
        |home| PathBuf::from(home).join(".jam"),
    )
}

fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("jam_svc_repo=info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::Path as AxumPath;
    use axum::http::HeaderMap;
    use axum::routing::post;
    use axum::{Json, Router};
    use std::fs;
    use tempfile::TempDir;
    use tokio::net::TcpListener;

    #[test]
    fn parses_github_pr_url_to_ref() {
        assert_eq!(
            pr_ref_from_url("https://github.com/cleak/blueberry/pull/42"),
            Some("cleak/blueberry#42".into())
        );
        assert_eq!(
            pr_ref_from_url("https://github.com/cleak/blueberry/issues/42"),
            None
        );
    }

    #[test]
    fn versioned_subject_prefix_subscription_keeps_methods_parseable() {
        let prefix = "tool.repo.v047";

        assert_eq!(format!("{prefix}.>"), "tool.repo.v047.>");
        assert_eq!(
            method_from_subject("tool.repo.v047.open-pr"),
            Some("open-pr")
        );
        assert_eq!(method_from_subject("tool.repo.v047.ping"), Some("ping"));
    }

    #[test]
    fn rejects_unsafe_branch_names() {
        for branch in ["", "-x", "/x", "x/", "x..y", "x//y", "x@y", "x\\y", "x y"] {
            assert!(validate_branch("branch", branch).is_err(), "{branch}");
        }
        assert!(validate_branch("branch", "task/session-smoke").is_ok());
    }

    #[test]
    fn pr_titles_are_jam_prefixed_once() {
        assert_eq!(
            format_jam_pr_title("Improve terrain manifest loading").unwrap(),
            "[jam] Improve terrain manifest loading"
        );
        assert_eq!(
            format_jam_pr_title("[jam] Improve terrain manifest loading").unwrap(),
            "[jam] Improve terrain manifest loading"
        );
    }

    #[test]
    fn parses_owner_repo_pr_selector() {
        let config = RepoConfig {
            git_bin: PathBuf::from("git"),
            gh_bin: PathBuf::from("gh"),
            codex_bin: PathBuf::from("codex"),
            github_repo: "cleak/blueberry".into(),
            default_base: "main".into(),
            notify_human_subject: DEFAULT_NOTIFY_HUMAN_SUBJECT.into(),
            timeout: Duration::from_secs(1),
            handled_artifacts_path: PathBuf::from("/tmp/review-artifacts.jsonl"),
            github_app: None,
            github_user_token: None,
        };
        let selector = PrSelector::parse("cleak/blueberry#42", None, &config).unwrap();
        assert_eq!(selector.selector, "42");
        assert_eq!(selector.repo.as_deref(), Some("cleak/blueberry"));
        assert_eq!(selector.pr_ref.as_deref(), Some("cleak/blueberry#42"));
    }

    #[tokio::test]
    async fn pr_status_uses_gh_json() {
        let fixture = GhFixture::new();
        fixture.write_script(
            r#"#!/usr/bin/env bash
set -euo pipefail
if [[ "$1 $2 $3" == "pr view 42" ]]; then
  printf '{"number":42,"url":"https://github.com/cleak/blueberry/pull/42","state":"OPEN","title":"Test PR","headRefName":"task/test","isDraft":true}\n'
  exit 0
fi
printf 'unexpected args: %s\n' "$*" >&2
exit 1
"#,
        );
        let config = fixture.config();
        let ctx = TraceCtx::new_root("test", "repo status");
        let selector = PrSelector::parse("cleak/blueberry#42", None, &config).unwrap();

        let status = pr_status_selector(&config, &selector, &ctx).await.unwrap();

        assert_eq!(status.pr_ref, "cleak/blueberry#42");
        assert_eq!(status.state, "open");
        assert_eq!(status.branch, "task/test");
        assert!(status.draft);
    }

    #[tokio::test]
    async fn prepare_merge_reads_view_and_checks_without_merging() {
        let fixture = GhFixture::new();
        fixture.write_script(
            r#"#!/usr/bin/env bash
set -euo pipefail
if [[ "$1 $2 $3" == "pr view 42" ]]; then
  printf '{"number":42,"url":"https://github.com/cleak/blueberry/pull/42","state":"OPEN","title":"Ready PR","headRefName":"task/test","isDraft":false,"mergeStateStatus":"CLEAN","reviewDecision":"APPROVED"}\n'
  exit 0
fi
if [[ "$1 $2 $3" == "pr checks 42" ]]; then
  printf '[{"name":"ci","state":"SUCCESS","conclusion":"SUCCESS","bucket":"pass","link":"https://ci.example/42","description":"ok"}]\n'
  exit 0
fi
printf 'unexpected args: %s\n' "$*" >&2
exit 1
"#,
        );
        let config = fixture.config();
        let ctx = TraceCtx::new_root("test", "prepare merge");
        let selector = PrSelector::parse("cleak/blueberry#42", None, &config).unwrap();

        let output = prepare_merge_selector(&config, &selector, &ctx)
            .await
            .unwrap();

        assert_eq!(output.pr_ref, "cleak/blueberry#42");
        assert!(output.ready);
        assert!(output.checks_passed);
        assert_eq!(output.checks[0].name, "ci");
        assert!(output.checks[0].passed);
    }

    #[test]
    fn merge_preflight_is_conservative_for_pending_or_blocked_state() {
        let pending = GhPrCheck {
            name: "ci".into(),
            state: Some("IN_PROGRESS".into()),
            conclusion: None,
            bucket: None,
            link: None,
            description: None,
        };

        assert!(!merge_check_passed(&pending));
        assert!(!merge_state_allows_merge("DIRTY"));
        assert!(!review_decision_allows_merge("CHANGES_REQUESTED"));
    }

    #[tokio::test]
    async fn read_pr_comments_normalizes_untrusted_artifacts() {
        let fixture = GhFixture::new();
        fixture.write_script(
            r#"#!/usr/bin/env bash
set -euo pipefail
if [[ "$1" == "api" && "$2" == "repos/cleak/blueberry/issues/42/comments" ]]; then
  printf '[{"id":10,"body":"issue body","html_url":"https://github.com/cleak/blueberry/pull/42#issuecomment-10","user":{"login":"coderabbitai"},"created_at":"2026-05-06T12:00:00Z"}]\n'
  exit 0
fi
if [[ "$1" == "api" && "$2" == "repos/cleak/blueberry/pulls/42/comments" ]]; then
  printf '[{"id":20,"body":"review body","html_url":"https://github.com/cleak/blueberry/pull/42#discussion_r20","user":{"login":"github-actions"},"path":"src/lib.rs","line":7,"created_at":"2026-05-06T12:01:00Z"}]\n'
  exit 0
fi
if [[ "$1" == "api" && "$2" == "repos/cleak/blueberry/pulls/42/reviews" ]]; then
  printf '[{"id":30,"body":"summary body","html_url":"https://github.com/cleak/blueberry/pull/42#pullrequestreview-30","user":{"login":"reviewer"},"state":"COMMENTED","submitted_at":"2026-05-06T12:02:00Z"}]\n'
  exit 0
fi
printf 'unexpected args: %s\n' "$*" >&2
exit 1
"#,
        );
        let state = RepoState {
            config: fixture.config(),
        };
        let ctx = TraceCtx::new_root("test", "read comments");
        let payload = br#"{"pr_ref":"cleak/blueberry#42"}"#;

        let output = read_pr_comments(payload, &state, &ctx).await.unwrap();

        assert_eq!(output.pr_ref, "cleak/blueberry#42");
        assert_eq!(output.artifacts.len(), 3);
        assert!(output
            .artifacts
            .iter()
            .all(|artifact| artifact.body_trust == "untrusted"));
        assert!(output.artifacts.iter().any(|artifact| artifact.id
            == "github-review-comment:cleak/blueberry#42:20"
            && artifact.path.as_deref() == Some("src/lib.rs")
            && artifact.line == Some(7)));
    }

    #[tokio::test]
    async fn reply_to_review_comment_posts_threaded_reply() {
        let fixture = GhFixture::new();
        fixture.write_script(
            r#"#!/usr/bin/env bash
set -euo pipefail
if [[ "$1 $2 $3 $4" == "api --method POST repos/cleak/blueberry/pulls/comments/20/replies" ]]; then
  if [[ "$5" == "-f" && "$6" == "body=Addressed." ]]; then
    printf '{"html_url":"https://github.com/cleak/blueberry/pull/42#discussion_r21"}\n'
    exit 0
  fi
fi
printf 'unexpected args: %s\n' "$*" >&2
exit 1
"#,
        );
        let state = RepoState {
            config: fixture.config(),
        };
        let ctx = TraceCtx::new_root("test", "reply comment");
        let payload =
            br#"{"artifact_id":"github-review-comment:cleak/blueberry#42:20","text":"Addressed."}"#;

        let output = reply_to_comment(payload, &state, &ctx).await.unwrap();

        assert_eq!(output.status, "posted");
        assert_eq!(
            output.url.as_deref(),
            Some("https://github.com/cleak/blueberry/pull/42#discussion_r21")
        );
    }

    #[test]
    fn append_handled_artifact_writes_jsonl_state() {
        let fixture = GhFixture::new();
        let config = fixture.config();
        let output = MarkReviewArtifactHandledOutput {
            artifact_id: "github-issue-comment:cleak/blueberry#42:10".into(),
            status: "Addressed".into(),
            handled_at: Utc::now(),
            trace_id: TraceCtx::new_root("test", "handled").trace_id.to_string(),
        };

        append_handled_artifact(&config, &output, "fixed in latest push").unwrap();
        let raw = fs::read_to_string(config.handled_artifacts_path).unwrap();

        assert!(raw.contains("github-issue-comment:cleak/blueberry#42:10"));
        assert!(raw.contains("fixed in latest push"));
    }

    #[tokio::test]
    async fn codex_review_runs_in_worktree_against_base() {
        let fixture = GhFixture::new();
        fixture.write_script(
            r#"#!/usr/bin/env bash
set -euo pipefail
if [[ "$1" == "-C" && "$3 $4 $5" == "review --base main" ]]; then
  test -d "$2"
  printf 'Codex review finding.\n'
  exit 0
fi
printf 'unexpected args: %s\n' "$*" >&2
exit 1
"#,
        );
        let worktree = fixture.tmp.path().join("worktree");
        fs::create_dir_all(&worktree).unwrap();

        let review = run_codex_review(&fixture.config(), &worktree, "main")
            .await
            .unwrap();

        assert_eq!(review, "Codex review finding.");
    }

    #[tokio::test]
    async fn github_app_token_exchange_uses_octocrab_installation_auth() {
        async fn token(
            AxumPath(installation_id): AxumPath<u64>,
            headers: HeaderMap,
        ) -> Json<serde_json::Value> {
            assert_eq!(installation_id, 456);
            let auth = headers
                .get("authorization")
                .and_then(|value| value.to_str().ok())
                .unwrap_or_default();
            assert!(auth.starts_with("Bearer "));
            Json(serde_json::json!({
                "token": "ghs_mock_installation_token",
                "expires_at": "2030-01-01T00:00:00Z",
                "permissions": {
                    "contents": "write",
                    "issues": "write",
                    "metadata": "read",
                },
            }))
        }

        let base_uri = spawn_mock_github(Router::new().route(
            "/app/installations/{installation_id}/access_tokens",
            post(token),
        ))
        .await;
        let config = GitHubAppConfig {
            app_id: 123,
            installation_id: 456,
            private_key_pem: SecretString::from(TEST_RSA_PRIVATE_KEY.to_owned()),
            api_base_uri: Some(base_uri),
        };

        let token = app_installation_token(&config).await.unwrap();

        assert_eq!(token.expose_secret(), "ghs_mock_installation_token");
    }

    #[tokio::test]
    async fn git_push_uses_github_app_token_credential_helper() {
        async fn token(
            AxumPath(installation_id): AxumPath<u64>,
            _headers: HeaderMap,
        ) -> Json<serde_json::Value> {
            assert_eq!(installation_id, 456);
            Json(serde_json::json!({
                "token": "ghs_mock_installation_token",
                "expires_at": "2030-01-01T00:00:00Z",
                "permissions": {
                    "contents": "write",
                    "metadata": "read",
                },
            }))
        }

        let fixture = GhFixture::new();
        fixture.write_script(
            r#"#!/usr/bin/env bash
set -euo pipefail
if [[ "$1" == "-C" && "$3" == "-c" && "$4" == "credential.helper=" && "$5" == "-c" && "$7 $8 $9 ${10}" == "push -u origin task/test" ]]; then
  test -d "$2"
  [[ "$6" == credential.helper='!'* ]]
  [[ "${GIT_TERMINAL_PROMPT:-}" == "0" ]]
  [[ "${JAM_GITHUB_APP_INSTALLATION_TOKEN:-}" == "ghs_mock_installation_token" ]]
  exit 0
fi
printf 'unexpected args: %s\n' "$*" >&2
exit 1
"#,
        );
        let base_uri = spawn_mock_github(Router::new().route(
            "/app/installations/{installation_id}/access_tokens",
            post(token),
        ))
        .await;
        let mut config = fixture.config();
        config.github_app = Some(GitHubAppConfig {
            app_id: 123,
            installation_id: 456,
            private_key_pem: SecretString::from(TEST_RSA_PRIVATE_KEY.to_owned()),
            api_base_uri: Some(base_uri),
        });
        let worktree = fixture.tmp.path().join("worktree");
        fs::create_dir_all(&worktree).unwrap();

        git_push(&config, &worktree, "task/test").await.unwrap();
    }

    #[tokio::test]
    async fn gh_pr_create_reads_url_from_stdout() {
        let fixture = GhFixture::new();
        fixture.write_script(
            r#"#!/usr/bin/env bash
set -euo pipefail
if [[ "$1 $2" == "pr create" ]]; then
  printf 'https://github.com/cleak/blueberry/pull/77\n'
  exit 0
fi
printf 'unexpected args: %s\n' "$*" >&2
exit 1
"#,
        );
        let input = OpenPrInput {
            task_id: "task-1".into(),
            branch: "task/task-1".into(),
            title: "Test PR".into(),
            body: Some("body".into()),
            draft: Some(true),
            base: None,
            repo: None,
            worktree_path: None,
            push: Some(false),
        };

        let (url, token_kind) = gh_pr_create(
            &fixture.config(),
            "cleak/blueberry",
            "main",
            &input,
            true,
            None,
        )
        .await
        .unwrap();

        assert_eq!(url, "https://github.com/cleak/blueberry/pull/77");
        assert_eq!(token_kind, TokenKind::Installation);
    }

    #[tokio::test]
    async fn resolve_write_token_prefers_user_over_installation() {
        let fixture = GhFixture::new();
        let mut config = fixture.config();
        config.github_user_token = Some(SecretString::from("ghu_test_user_token".to_owned()));
        let (token, kind) = resolve_write_token(&config).await.unwrap().unwrap();
        assert_eq!(kind, TokenKind::User);
        assert_eq!(token.expose_secret(), "ghu_test_user_token");
    }

    #[tokio::test]
    async fn resolve_write_token_returns_none_when_unconfigured() {
        let fixture = GhFixture::new();
        let config = fixture.config();
        assert!(resolve_write_token(&config).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn post_coderabbit_trigger_comments_on_pr() {
        let fixture = GhFixture::new();
        fixture.write_script(
            r#"#!/usr/bin/env bash
set -euo pipefail
if [[ "$1 $2 $3 $4" == "api --method POST repos/cleak/blueberry/issues/77/comments" ]]; then
  if [[ "$5" == "-f" && "$6" == "body=@coderabbitai full review" ]]; then
    printf '{"html_url":"https://github.com/cleak/blueberry/pull/77#issuecomment-1"}\n'
    exit 0
  fi
fi
printf 'unexpected args: %s\n' "$*" >&2
exit 1
"#,
        );

        post_coderabbit_trigger(&fixture.config(), "cleak/blueberry#77")
            .await
            .unwrap();
    }

    struct GhFixture {
        tmp: TempDir,
        script: PathBuf,
    }

    impl GhFixture {
        fn new() -> Self {
            let tmp = TempDir::new().unwrap();
            let script = tmp.path().join("gh");
            Self { tmp, script }
        }

        fn config(&self) -> RepoConfig {
            RepoConfig {
                git_bin: self.script.clone(),
                gh_bin: self.script.clone(),
                codex_bin: self.script.clone(),
                github_repo: "cleak/blueberry".into(),
                default_base: "main".into(),
                notify_human_subject: DEFAULT_NOTIFY_HUMAN_SUBJECT.into(),
                timeout: Duration::from_secs(2),
                handled_artifacts_path: self.tmp.path().join("review-artifacts.jsonl"),
                github_app: None,
                github_user_token: None,
            }
        }

        fn write_script(&self, body: &str) {
            assert!(self.tmp.path().is_dir());
            let staged = self.tmp.path().join("gh.staged");
            fs::write(&staged, body).unwrap();
            let mut permissions = fs::metadata(&staged).unwrap().permissions();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                permissions.set_mode(0o755);
            }
            fs::set_permissions(&staged, permissions).unwrap();
            fs::rename(staged, &self.script).unwrap();
        }
    }

    async fn spawn_mock_github(router: Router) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        format!("http://{addr}")
    }

    const TEST_RSA_PRIVATE_KEY: &str = r"-----BEGIN PRIVATE KEY-----
MIIEvgIBADANBgkqhkiG9w0BAQEFAASCBKgwggSkAgEAAoIBAQCzffbqszslBfTp
/xPUGuocZBGCLrDHMx7ApGzvYxvDk9L0s+wlMVFLU38d2fcN7KF3ZSqQ3qvrQ3rv
4gjQ++xSke9ko7jMjjMrIvZ+ILNpgBR0jpzEqs4t6YhnsE723x5TZbmDkMBeekS9
Nu8AM/wbAE6CNmqr7LdDJ9KlXWNXzZVIXOPEIrgyF36EEkv7RZoKspHhnaqquTUb
bf7FyVf8upKcglcYaB6SbsFqKReYf5ad5Q0u5QO3Xh4ynZIX+aAkBBtWQYikjAzw
h2DmXcGS+7bhM6BLJYTT2g5vs5kGiGeMQ9yt8YBAgf9/e/ndDssmQWDhRGPMs8Sa
F19/dLVLAgMBAAECggEAHURkrsgIf1TK0XLbCq1UJMqjrjdVx41hg3uWxavtOtqP
kytZTdvjoRAy0dfDeHzv+8tpAZPwCB20iIkx8yhOKaKLxzxeUCsUUywmxtHR31nX
5q+FsW3BI8GzZmYxt1W8mKWldmXzxWlTbF+Y4KQZWf/BTWsjwU0zuVq0zBZVB1uu
MMJgqVJAPFjJD2Hzd6EYFS3Jl5n4njZCdjVtBGJcjMHal6iTkhuyQItUzB6aAZO7
G3esDNWuLTNtx6Eh97YhYzXLvvAhbHVsJ1LRIfIDLab+99HoPrNiWJXO6EMrba1T
SBbcifVcNG10lPrSRe0G053FNFqSWHM6QgGaaWKZ2QKBgQDYH20DUHNboq/jZY1A
7sg4r9DZbiRnklhXZ3DYnddEXDOIuehdmtjqbg23kp3nn9uRi4aLUGCHY0UiMZa5
p7og3MFTUWzWudLHcVB03hchZyF1pWWjvjQfm2LMUwJHcPLUl5S2LAYnXe9qVIbk
QqyggrGkxWgJbbEUsSstoavYUwKBgQDUnEjWBrPM0hMG27SgtZI5oAW+gK8bPFbu
mNI77L+tqqa7dteoA9Xjyo2i/8jY/3p/0Y+Z455gyB50cUN/CqjOaNbNxEmtK+Jg
rGBuYTa8WhFhEFfUN7tME0UodAsJwFYF8ZIIVsbQZ1WDnAVfXsoJ4iQHKyuOMR7h
8uI+MjKwKQKBgAOZ4nMfsAxi1ZNwab4fPG7VXyGAWFLxeU9bheHWH3QgJSuuDVUh
82NUmh3o74CghUQTkxZXLISU/t3m/Z/yT4OkqgP9Y1bgmcaA+No5qSEBWule7Cai
ULQGHstQxsTx+NnZ/LxcV23ofsjCx8yd38p84wDf2S/vB/hUS2fjPb3JAoGBAIbc
l6sDV1vNyXnpNVtXsWhSJDKh5/ELxkzUrU6Lr05W2CpDiSovPKagnlVNkLZs3+Ri
JofEBXt4lTDhg6H7PfaoM9ET+HQbSR5vWT/K9HBnZWy/dCbOL0VjV9QAP9wwn6Bn
im01tikN0wWHmzTSqK+6PYY6kQdCC0fhzDcNmm95AoGBAMoKqClVQf9FFyZBicQV
JUlzgP+jLloNqfOu+p4wzBhReZu6TDTbmzKPFKDg/EWL4J3wmbhb0VjbiSxwMSvT
LiMcY/xxFVTX8oFlK7B9CjbG06lYQxSuDzWZxBrGO18fm8spIVZyil9sNoXLWc/N
Nq6Qp8hwU+h2VxnbtMeu3oLh
-----END PRIVATE KEY-----";
}
