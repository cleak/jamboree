//! `jam-ui-server` — axum static UI and WebSocket-to-NATS bridge.

#![deny(missing_docs)]

use std::collections::{HashMap, VecDeque};
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path as AxumPath, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use futures::StreamExt;
use jam_nats::JamNats;
use jam_trace::TraceCtx;
use jam_ui_server::auth::{TokenRecord, TokenStore, TokenStoreError};
use jam_ui_server::trace_replay::{
    find_traces_in_journal, trace_replay_from_journal, TraceFindError, TraceReplayError,
};
use serde::{Deserialize, Serialize};
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;
use tracing::{error, info};

const DEFAULT_ALLOW_BIND_ADDRS: &str = "127.0.0.1,100.64.0.0/10";

#[derive(Clone)]
struct AppState {
    nats: JamNats,
    token_store: TokenStore,
    journal_root: PathBuf,
    session_log_root: PathBuf,
    task_graph_root: PathBuf,
    tool_timeout: Duration,
    task_store: Option<Arc<std::sync::Mutex<jam_task_store::TaskStore>>>,
}

#[derive(Debug, thiserror::Error)]
enum UiServerError {
    #[error("nats: {0}")]
    Nats(#[from] jam_nats::NatsError),

    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("static dir missing: {0}")]
    StaticDirMissing(PathBuf),

    #[error("invalid JAM_UI_ALLOW_BIND_ADDRS: {0}")]
    InvalidAllowBindAddrs(String),

    #[error("ui config: {0}")]
    UiConfig(String),

    #[error("bind address {bind} is outside allowed UI bind ranges: {allow}")]
    BindAddrDenied { bind: SocketAddr, allow: String },
}

#[derive(Debug, Deserialize)]
struct TokenQuery {
    token: String,
}

#[derive(Debug, Deserialize)]
struct WsQuery {
    token: String,
    #[serde(default = "default_subject")]
    subject: String,
}

#[derive(Debug, Deserialize)]
struct TraceQuery {
    token: String,
    #[serde(default = "default_max_depth")]
    max_depth: u32,
}

#[derive(Debug, Deserialize)]
struct TraceFindQuery {
    token: String,
    filter: String,
    #[serde(default = "default_trace_find_limit")]
    limit: usize,
}

#[derive(Debug, Deserialize)]
struct RuntimeServicesQuery {
    token: String,
}

#[derive(Debug, Deserialize)]
struct SessionOutputQuery {
    token: String,
    #[serde(default = "default_session_output_limit")]
    limit: usize,
}

#[derive(Debug, Deserialize)]
struct QuotaQuery {
    token: String,
    harness_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RecentEventsQuery {
    token: String,
    #[serde(default = "default_subject")]
    subject: String,
    #[serde(default = "default_recent_events_limit")]
    limit: usize,
}

#[derive(Debug, Deserialize)]
struct SessionMessageInput {
    mode: MessageMode,
    text: Option<String>,
    reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TaskResumeInput {
    prompt: String,
    project: Option<String>,
    harness: Option<String>,
    parent_session_id: Option<String>,
    task_class: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TaskSpawnInput {
    description: String,
    project: Option<String>,
    task_class: Option<String>,
    priority: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct UiConfigFile {
    auth: Option<UiAuthConfig>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct UiAuthConfig {
    allow_bind_addrs: Option<Vec<String>>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
enum MessageMode {
    Queue,
    Interrupt,
    FullStop,
}

impl MessageMode {
    fn tool_method(self) -> &'static str {
        match self {
            Self::Queue => "enqueue-message",
            Self::Interrupt => "interrupt-with-message",
            Self::FullStop => "full-stop",
        }
    }
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
}

#[derive(Debug, Serialize)]
struct TaskSpawnResponse {
    task_id: String,
    trace_id: String,
    status: &'static str,
    subject: &'static str,
}

#[derive(Debug, Serialize)]
struct TaskGraphRow {
    task_id: String,
    description: String,
    project: String,
    task_class: String,
    priority: String,
    status: String,
    requested_by: String,
    pr_ref: String,
    session_id: String,
    harness: String,
    outcome: String,
    trace_id: String,
    updated_at: String,
}

#[derive(Debug, Serialize)]
struct QuotaResponse {
    fetched_at: DateTime<Utc>,
    source: &'static str,
    windows: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: &'static str,
    detail: String,
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    error: &'static str,
    detail: String,
}

impl ApiError {
    fn into_response(self) -> axum::response::Response {
        trace_error_response(self.status, self.error, self.detail)
    }
}

#[derive(Debug, Serialize)]
struct WsEvent<'a> {
    subject: &'a str,
    payload: String,
}

#[derive(Debug, Serialize)]
struct OwnedWsEvent {
    subject: String,
    payload: String,
}

#[derive(Debug, Deserialize)]
struct RecentJournalEnvelope {
    event_type: String,
    timestamp: DateTime<Utc>,
}

#[derive(Debug)]
struct RecentJournalEvent {
    subject: String,
    payload: String,
    timestamp: DateTime<Utc>,
    path: PathBuf,
    line_number: usize,
}

#[derive(Debug, Serialize)]
struct TaskRequestedEnvelope {
    schema_version: u32,
    event_type: &'static str,
    event_subtype_version: u32,
    timestamp: DateTime<Utc>,
    journal_seq: u64,
    trace_id: String,
    actor: String,
    payload: TaskRequestedPayload,
}

#[derive(Debug, Serialize)]
struct TaskRequestedPayload {
    task_id: String,
    description: String,
    project: String,
    task_class: String,
    priority: String,
    requested_by: String,
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    init_tracing();
    if let Err(err) = run().await {
        error!("jam-ui-server fatal: {err}");
        return std::process::ExitCode::FAILURE;
    }
    std::process::ExitCode::SUCCESS
}

async fn run() -> Result<(), UiServerError> {
    let jam_home = jam_tools_core::paths::jam_home();
    let nats_url = std::env::var("NATS_URL").unwrap_or_else(|_| "nats://127.0.0.1:4222".into());
    let nats_token = std::env::var("NATS_TOKEN").ok();
    let static_dir =
        std::env::var("JAM_UI_STATIC_DIR").map_or_else(|_| jam_home.join("ui/dist"), PathBuf::from);
    let bind = std::env::var("JAM_UI_BIND").unwrap_or_else(|_| "127.0.0.1:8787".into());
    let allow_bind_addrs = load_allow_bind_addrs(&jam_home)?;
    let tool_timeout = std::env::var("JAM_UI_TOOL_TIMEOUT_SECS")
        .ok()
        .and_then(|raw| raw.parse().ok())
        .map_or(Duration::from_secs(5), Duration::from_secs);
    let bind: SocketAddr = bind.parse().map_err(|err| {
        UiServerError::Io(std::io::Error::new(std::io::ErrorKind::InvalidInput, err))
    })?;
    let allowlist =
        parse_bind_allowlist(&allow_bind_addrs).map_err(UiServerError::InvalidAllowBindAddrs)?;
    if !bind_addr_allowed(bind.ip(), &allowlist) {
        return Err(UiServerError::BindAddrDenied {
            bind,
            allow: allow_bind_addrs,
        });
    }

    if !static_dir.is_dir() {
        return Err(UiServerError::StaticDirMissing(static_dir));
    }

    let nats = JamNats::connect(&nats_url, nats_token).await?;

    let task_store_path = jam_home.join("task-store.db");
    let task_store = match jam_task_store::TaskStore::open(&task_store_path) {
        Ok(store) => {
            info!(path = %task_store_path.display(), "task event store opened (read)");
            Some(Arc::new(std::sync::Mutex::new(store)))
        }
        Err(err) => {
            info!(
                path = %task_store_path.display(),
                error = %err,
                "task event store not available; falling back to graph-based task queries"
            );
            None
        }
    };

    let state = AppState {
        nats,
        token_store: TokenStore::from_jam_home(&jam_home),
        journal_root: jam_home.join("journal"),
        session_log_root: session_log_root(&jam_home),
        task_graph_root: task_graph_root(&jam_home),
        tool_timeout,
        task_store,
    };

    let app = app(state, &static_dir);
    let listener = tokio::net::TcpListener::bind(bind).await?;
    info!(%bind, "jam-ui-server listening");
    axum::serve(listener, app).await?;
    Ok(())
}

fn app(state: AppState, static_dir: &Path) -> Router {
    let index = static_dir.join("index.html");
    Router::new()
        .route("/api/health", get(health))
        .route("/api/auth/check", get(auth_check))
        .route("/api/events/recent", get(recent_events_handler))
        .route("/api/quota", get(quota_handler))
        .route("/api/runtime/services", get(runtime_services_handler))
        .route(
            "/api/deploy",
            get(deploy_targets_handler).post(deploy_handler),
        )
        .route("/api/tasks", get(tasks_handler).post(task_spawn_handler))
        .route("/api/tasks/{task_id}", get(task_detail_handler))
        .route("/api/tasks/{task_id}/events", get(task_events_handler))
        .route("/api/tasks/{task_id}/resume", post(task_resume_handler))
        .route("/api/trace/{trace_id}", get(trace_replay_handler))
        .route("/api/traces/find", get(trace_find_handler))
        .route(
            "/api/sessions/{session_id}/messages",
            post(session_message_handler),
        )
        .route(
            "/api/sessions/{session_id}/output",
            get(session_output_handler),
        )
        .route("/ws", get(ws_handler))
        .fallback_service(ServeDir::new(static_dir).fallback(ServeFile::new(index)))
        .layer(TraceLayer::new_for_http())
        .with_state(Arc::new(state))
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

async fn auth_check(
    State(state): State<Arc<AppState>>,
    Query(query): Query<TokenQuery>,
) -> impl IntoResponse {
    match token_is_valid(&state.token_store, &query.token) {
        Ok(true) => (StatusCode::OK, Json(HealthResponse { status: "ok" })).into_response(),
        Ok(false) => (
            StatusCode::UNAUTHORIZED,
            Json(HealthResponse {
                status: "unauthorized",
            }),
        )
            .into_response(),
        Err(err) => {
            error!("auth check failed: {err}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(HealthResponse { status: "error" }),
            )
                .into_response()
        }
    }
}

async fn recent_events_handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<RecentEventsQuery>,
) -> impl IntoResponse {
    match token_is_valid(&state.token_store, &query.token) {
        Ok(true) => {}
        Ok(false) => return StatusCode::UNAUTHORIZED.into_response(),
        Err(err) => {
            error!("recent events auth failed: {err}");
            return trace_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "auth-check-failed",
                err.to_string(),
            );
        }
    }

    match recent_journal_events(&state.journal_root, &query.subject, query.limit) {
        Ok(events) => Json(events).into_response(),
        Err(err) => trace_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "recent-events-failed",
            err,
        ),
    }
}

async fn runtime_services_handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<RuntimeServicesQuery>,
) -> impl IntoResponse {
    match token_is_valid(&state.token_store, &query.token) {
        Ok(true) => {}
        Ok(false) => return StatusCode::UNAUTHORIZED.into_response(),
        Err(err) => {
            error!("runtime services auth failed: {err}");
            return trace_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "auth-check-failed",
                err.to_string(),
            );
        }
    }

    match runtime_services() {
        Ok(services) => Json(services).into_response(),
        Err(err) => trace_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "runtime-services-failed",
            err,
        ),
    }
}

#[derive(Debug, Serialize)]
struct DeployTargetView {
    short_name: String,
    crate_name: String,
    binary_name: String,
    strategy: String,
}

async fn deploy_targets_handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<TokenQuery>,
) -> impl IntoResponse {
    match token_is_valid(&state.token_store, &query.token) {
        Ok(true) => {}
        Ok(false) => return StatusCode::UNAUTHORIZED.into_response(),
        Err(err) => {
            error!("deploy targets auth failed: {err}");
            return trace_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "auth-check-failed",
                err.to_string(),
            );
        }
    }
    let targets: Vec<_> = jam_tools_core::deploy_targets::DEPLOY_TARGETS
        .iter()
        .map(|t| DeployTargetView {
            short_name: t.short_name.to_owned(),
            crate_name: t.crate_name.to_owned(),
            binary_name: t.binary_name.to_owned(),
            strategy: match t.strategy {
                jam_tools_core::deploy_targets::DeployStrategy::AtomicSwap => "atomic-swap".into(),
                jam_tools_core::deploy_targets::DeployStrategy::StopReplaceRestart { .. } => {
                    "stop-replace-restart".into()
                }
                jam_tools_core::deploy_targets::DeployStrategy::PythonApp { .. } => {
                    "python-app".into()
                }
                jam_tools_core::deploy_targets::DeployStrategy::CanonicalBinary { .. } => {
                    "canonical-binary".into()
                }
            },
        })
        .collect();
    Json(targets).into_response()
}

#[derive(Debug, Deserialize)]
struct DeployRequest {
    service: String,
    /// Optional override of the staged binary path. Defaults to the caleb
    /// monorepo's `target/release/<binary_name>` (i.e. wherever the operator
    /// last ran `cargo build --release`).
    staging_path: Option<String>,
    /// Optional version override. Defaults to `<workspace>-<content-hash>`.
    version: Option<String>,
}

#[derive(Debug, Serialize)]
struct DeployResponse {
    service: String,
    version: String,
    staging_path: String,
    outcome: String,
    detail: String,
    trace_id: String,
}

const DEFAULT_DEPLOY_WORKSPACE_ROOT: &str = "/home/caleb/jamboree";
const DEPLOY_WAIT_SECS: u64 = 90;

async fn deploy_handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<TokenQuery>,
    Json(input): Json<DeployRequest>,
) -> impl IntoResponse {
    let user = match token_record(&state.token_store, &query.token) {
        Ok(Some(record)) => record,
        Ok(None) => return StatusCode::UNAUTHORIZED.into_response(),
        Err(err) => {
            error!("deploy auth failed: {err}");
            return trace_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "auth-check-failed",
                err.to_string(),
            );
        }
    };

    let service = input.service.trim().to_owned();
    if service.is_empty() {
        return trace_error_response(
            StatusCode::BAD_REQUEST,
            "missing-service",
            "service is required".into(),
        );
    }
    let target = match jam_tools_core::deploy_targets::find(&service) {
        Some(t) => t,
        None => {
            return trace_error_response(
                StatusCode::BAD_REQUEST,
                "unknown-service",
                format!("no deploy target named `{service}`; see GET /api/deploy for the list"),
            );
        }
    };

    use jam_tools_core::deploy_targets::DeployStrategy as Strategy;
    // PythonApp stages a source directory; everything else stages a built
    // binary file. The defaults reflect that.
    let is_python_app = matches!(target.strategy, Strategy::PythonApp { .. });
    let staging_path = match input.staging_path {
        Some(raw) => PathBuf::from(raw),
        None => {
            if is_python_app {
                Path::new(DEFAULT_DEPLOY_WORKSPACE_ROOT).join("maestro")
            } else {
                Path::new(DEFAULT_DEPLOY_WORKSPACE_ROOT)
                    .join("target")
                    .join("release")
                    .join(target.binary_name)
            }
        }
    };
    if is_python_app {
        if !staging_path.is_dir() {
            return trace_error_response(
                StatusCode::BAD_REQUEST,
                "staging-path-missing",
                format!(
                    "no directory at {} (PythonApp deploy expects a source tree with pyproject.toml)",
                    staging_path.display()
                ),
            );
        }
    } else if !staging_path.is_file() {
        return trace_error_response(
            StatusCode::BAD_REQUEST,
            "staging-path-missing",
            format!(
                "no file at {} — build the binary first (cargo build --release -p {})",
                staging_path.display(),
                target.crate_name
            ),
        );
    }

    // For PythonApp the binary_sha256 field is required by the schema but
    // patch-agent ignores it. Use a sentinel so future binary-flow consumers
    // can spot the no-binary case explicitly.
    let binary_sha256 = if is_python_app {
        "python-app-no-binary-hash".to_owned()
    } else {
        match jam_tools_core::hashing::sha256_file_hex(&staging_path) {
            Ok(value) => value,
            Err(err) => {
                return trace_error_response(StatusCode::INTERNAL_SERVER_ERROR, "hash-failed", err);
            }
        }
    };
    let version = match input.version {
        Some(value) => value,
        None => {
            if is_python_app {
                // No content hash for python source; use a timestamp.
                format!("0.1.0-ui-{}", Utc::now().timestamp())
            } else {
                let short = &binary_sha256[..binary_sha256.len().min(7)];
                format!("0.1.0-ui-{short}")
            }
        }
    };
    let trace_ctx = TraceCtx::new_root(
        "ui.deploy",
        format!("UI-initiated deploy of {service} {version}"),
    );
    let trace_id = trace_ctx.trace_id.to_string();
    let staging_path_str = staging_path.display().to_string();

    // Subscribe BEFORE publishing so we don't miss a fast confirmation.
    let nats = state.nats.clone();
    let confirmed_sub = match nats.client().subscribe("patch.confirmed").await {
        Ok(sub) => sub,
        Err(err) => {
            return trace_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "nats-subscribe-failed",
                err.to_string(),
            );
        }
    };
    let failed_sub = match nats.client().subscribe("patch.failed").await {
        Ok(sub) => sub,
        Err(err) => {
            return trace_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "nats-subscribe-failed",
                err.to_string(),
            );
        }
    };

    let staged = jam_events::generated::PatchStaged {
        service: service.clone(),
        version: version.clone(),
        staging_path: staging_path_str.clone(),
        binary_sha256: binary_sha256.clone(),
        requested_by: user.user_id.clone(),
        ts: Utc::now(),
    };
    let envelope = jam_events::EventEnvelope::new(
        "patch.staged",
        1,
        0,
        trace_id.clone(),
        user.user_id.clone(),
        staged,
    );
    if let Err(err) = nats
        .publish_traced("patch.staged", &envelope, &trace_ctx)
        .await
    {
        return trace_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "publish-failed",
            err.to_string(),
        );
    }

    let outcome = wait_for_deploy_terminal(
        confirmed_sub,
        failed_sub,
        &trace_id,
        &service,
        Duration::from_secs(DEPLOY_WAIT_SECS),
    )
    .await;
    match outcome {
        Ok((status, detail)) => Json(DeployResponse {
            service,
            version,
            staging_path: staging_path_str,
            outcome: status,
            detail,
            trace_id,
        })
        .into_response(),
        Err(err) => trace_error_response(StatusCode::GATEWAY_TIMEOUT, "deploy-timeout", err),
    }
}

async fn wait_for_deploy_terminal(
    mut confirmed_sub: jam_nats::async_nats::Subscriber,
    mut failed_sub: jam_nats::async_nats::Subscriber,
    expected_trace_id: &str,
    expected_service: &str,
    timeout: Duration,
) -> Result<(String, String), String> {
    use jam_events::generated::PatchConfirmed;
    let deadline = tokio::time::sleep(timeout);
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            () = &mut deadline => {
                return Err(format!(
                    "no terminal patch event for trace {expected_trace_id} within {}s",
                    timeout.as_secs()
                ));
            }
            msg = confirmed_sub.next() => {
                let Some(msg) = msg else {
                    return Err("patch.confirmed subscription closed unexpectedly".into());
                };
                if let Some((service, version, checks_run, _trace_id)) =
                    decode_patch_event::<PatchConfirmed>(&msg.payload, expected_trace_id, expected_service)
                {
                    let detail = if checks_run == 0 {
                        format!("version {version} already current (no-op)")
                    } else {
                        format!("version {version} confirmed by {checks_run} checks")
                    };
                    return Ok(("confirmed".into(), format!("{service}: {detail}")));
                }
            }
            msg = failed_sub.next() => {
                let Some(msg) = msg else {
                    return Err("patch.failed subscription closed unexpectedly".into());
                };
                if let Some((service, _version, _checks_run, summary)) =
                    decode_patch_failed(&msg.payload, expected_trace_id, expected_service)
                {
                    return Ok(("failed".into(), format!("{service}: {summary}")));
                }
            }
        }
    }
}

fn decode_patch_event<P>(
    payload: &[u8],
    expected_trace_id: &str,
    expected_service: &str,
) -> Option<(String, String, u32, String)>
where
    P: serde::de::DeserializeOwned,
    P: PatchEventLike,
{
    let envelope: serde_json::Value = serde_json::from_slice(payload).ok()?;
    let trace_id = envelope.get("trace_id")?.as_str()?;
    if trace_id != expected_trace_id {
        return None;
    }
    let payload_val = envelope.get("payload")?;
    let payload: P = serde_json::from_value(payload_val.clone()).ok()?;
    if payload.service() != expected_service {
        return None;
    }
    Some((
        payload.service().to_owned(),
        payload.version().to_owned(),
        payload.checks_run(),
        trace_id.to_owned(),
    ))
}

fn decode_patch_failed(
    payload: &[u8],
    expected_trace_id: &str,
    expected_service: &str,
) -> Option<(String, String, u32, String)> {
    let envelope: serde_json::Value = serde_json::from_slice(payload).ok()?;
    let trace_id = envelope.get("trace_id")?.as_str()?;
    if trace_id != expected_trace_id {
        return None;
    }
    let payload_val = envelope.get("payload")?;
    let service = payload_val.get("service")?.as_str()?;
    if service != expected_service {
        return None;
    }
    let summary = payload_val
        .get("summary")
        .and_then(|v| v.as_str())
        .unwrap_or("(no summary)");
    Some((service.to_owned(), String::new(), 0, summary.to_owned()))
}

trait PatchEventLike {
    fn service(&self) -> &str;
    fn version(&self) -> &str;
    fn checks_run(&self) -> u32;
}
impl PatchEventLike for jam_events::generated::PatchConfirmed {
    fn service(&self) -> &str {
        &self.service
    }
    fn version(&self) -> &str {
        &self.version
    }
    fn checks_run(&self) -> u32 {
        self.checks_run
    }
}

async fn tasks_handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<TokenQuery>,
) -> impl IntoResponse {
    match token_is_valid(&state.token_store, &query.token) {
        Ok(true) => {}
        Ok(false) => return StatusCode::UNAUTHORIZED.into_response(),
        Err(err) => {
            error!("tasks auth failed: {err}");
            return trace_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "auth-check-failed",
                err.to_string(),
            );
        }
    }

    // Prefer the event store when available — it has richer, pre-computed
    // state with proper continuation tracking. Fall back to the Tempyr graph
    // files for backward compatibility when the store isn't initialized yet.
    if let Some(store) = &state.task_store {
        match store.lock() {
            Ok(s) => {
                match s.list(&jam_task_store::TaskFilter::default()) {
                    Ok(summaries) => return Json(summaries).into_response(),
                    Err(err) => {
                        info!(error = %err, "task store query failed; falling back to graph files");
                    }
                }
            }
            Err(err) => {
                info!(error = %err, "task store lock poisoned; falling back to graph files");
            }
        }
    }

    match task_graph_rows(&state.task_graph_root) {
        Ok(rows) => Json(rows).into_response(),
        Err(err) => trace_error_response(StatusCode::INTERNAL_SERVER_ERROR, "tasks-failed", err),
    }
}

/// Get a single task's full state from the event store.
async fn task_detail_handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<TokenQuery>,
    AxumPath(task_id): AxumPath<String>,
) -> impl IntoResponse {
    match token_is_valid(&state.token_store, &query.token) {
        Ok(true) => {}
        Ok(false) => return StatusCode::UNAUTHORIZED.into_response(),
        Err(err) => {
            error!("task detail auth failed: {err}");
            return trace_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "auth-check-failed",
                err.to_string(),
            );
        }
    }

    let Some(store) = &state.task_store else {
        return trace_error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "store-unavailable",
            "task event store not initialized".to_string(),
        );
    };

    match store.lock() {
        Ok(s) => match s.get_summary(&task_id) {
            Ok(Some(summary)) => Json(summary).into_response(),
            Ok(None) => StatusCode::NOT_FOUND.into_response(),
            Err(err) => trace_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "task-detail-failed",
                err.to_string(),
            ),
        },
        Err(err) => trace_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store-lock-failed",
            err.to_string(),
        ),
    }
}

/// Get the full event history for a task from the event store.
async fn task_events_handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<TokenQuery>,
    AxumPath(task_id): AxumPath<String>,
) -> impl IntoResponse {
    match token_is_valid(&state.token_store, &query.token) {
        Ok(true) => {}
        Ok(false) => return StatusCode::UNAUTHORIZED.into_response(),
        Err(err) => {
            error!("task events auth failed: {err}");
            return trace_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "auth-check-failed",
                err.to_string(),
            );
        }
    }

    let Some(store) = &state.task_store else {
        return trace_error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "store-unavailable",
            "task event store not initialized".to_string(),
        );
    };

    match store.lock() {
        Ok(s) => match s.events(&task_id) {
            Ok(events) => {
                let response: Vec<serde_json::Value> = events
                    .into_iter()
                    .map(|e| {
                        serde_json::json!({
                            "version": e.version,
                            "event_type": e.event_type,
                            "payload": serde_json::from_str::<serde_json::Value>(&e.payload).unwrap_or_default(),
                            "trace_id": e.trace_id,
                            "timestamp": e.timestamp,
                            "idempotency_key": e.idempotency_key,
                        })
                    })
                    .collect();
                Json(response).into_response()
            }
            Err(err) => trace_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "task-events-failed",
                err.to_string(),
            ),
        },
        Err(err) => trace_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store-lock-failed",
            err.to_string(),
        ),
    }
}

async fn task_spawn_handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<TokenQuery>,
    Json(input): Json<TaskSpawnInput>,
) -> impl IntoResponse {
    let user = match token_record(&state.token_store, &query.token) {
        Ok(Some(record)) => record,
        Ok(None) => return StatusCode::UNAUTHORIZED.into_response(),
        Err(err) => {
            error!("task spawn auth failed: {err}");
            return trace_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "auth-check-failed",
                err.to_string(),
            );
        }
    };

    let description = match normalize_required_input(&input.description, "description") {
        Ok(value) => value,
        Err(err) => return err.into_response(),
    };
    let project = match normalize_project(input.project.as_deref()) {
        Ok(value) => value,
        Err(err) => return err.into_response(),
    };
    let task_class =
        match normalize_optional_input(input.task_class.as_deref(), "light-edit", "task-class") {
            Ok(value) => value,
            Err(err) => return err.into_response(),
        };
    let priority = match normalize_priority(input.priority.as_deref().unwrap_or("normal")) {
        Ok(value) => value,
        Err(err) => return err.into_response(),
    };

    let trace_ctx = TraceCtx::new_root(
        "ui.task.spawn",
        format!("user spawned task from UI: {description}"),
    );
    let task_id = task_id_for(&description, &trace_ctx);
    let trace_id = trace_ctx.trace_id.to_string();
    let actor = user.user_id;
    let envelope = TaskRequestedEnvelope {
        schema_version: 1,
        event_type: "task.requested",
        event_subtype_version: 1,
        timestamp: Utc::now(),
        journal_seq: 0,
        trace_id: trace_id.clone(),
        actor: actor.clone(),
        payload: TaskRequestedPayload {
            task_id: task_id.clone(),
            description,
            project,
            task_class,
            priority,
            requested_by: actor,
        },
    };

    if let Err(err) = state
        .nats
        .publish_traced("journal.task.requested", &envelope, &trace_ctx)
        .await
    {
        return trace_error_response(
            StatusCode::BAD_GATEWAY,
            "task-spawn-failed",
            err.to_string(),
        );
    }

    Json(TaskSpawnResponse {
        task_id,
        trace_id,
        status: "requested",
        subject: "journal.task.requested",
    })
    .into_response()
}

async fn task_resume_handler(
    State(state): State<Arc<AppState>>,
    AxumPath(task_id): AxumPath<String>,
    Query(query): Query<TokenQuery>,
    Json(input): Json<TaskResumeInput>,
) -> impl IntoResponse {
    match token_record(&state.token_store, &query.token) {
        Ok(Some(_record)) => {}
        Ok(None) => return StatusCode::UNAUTHORIZED.into_response(),
        Err(err) => {
            error!("task resume auth failed: {err}");
            return trace_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "auth-check-failed",
                err.to_string(),
            );
        }
    };

    let prompt = match normalize_required_input(&input.prompt, "prompt") {
        Ok(value) => value,
        Err(err) => return err.into_response(),
    };
    if let Err(detail) = validate_task_id(&task_id) {
        return trace_error_response(StatusCode::BAD_REQUEST, "invalid-task-id", detail);
    }
    if let Some(parent_session_id) = input.parent_session_id.as_deref() {
        if let Err(detail) = validate_session_id(parent_session_id) {
            return trace_error_response(StatusCode::BAD_REQUEST, "invalid-session-id", detail);
        }
    }
    let task_class =
        match normalize_optional_input(input.task_class.as_deref(), "light-edit", "task-class") {
            Ok(value) => value,
            Err(err) => return err.into_response(),
        };

    let trace_ctx = TraceCtx::new_root(
        "ui.task.resume",
        format!("user resumed task from UI: {task_id}"),
    );
    let payload = task_resume_payload(task_id, prompt, input, task_class);
    let response: serde_json::Value = match state
        .nats
        .request_traced(
            "tool.session.resume-picker",
            &payload,
            &trace_ctx,
            state.tool_timeout,
        )
        .await
    {
        Ok(response) => response,
        Err(err) => {
            return trace_error_response(
                StatusCode::BAD_GATEWAY,
                "task-resume-failed",
                err.to_string(),
            );
        }
    };
    if let Some(error) = response.get("error") {
        return trace_error_response(
            StatusCode::CONFLICT,
            "task-resume-rejected",
            error.to_string(),
        );
    }
    Json(response).into_response()
}

fn task_resume_payload(
    task_id: String,
    prompt: String,
    input: TaskResumeInput,
    task_class: String,
) -> serde_json::Value {
    serde_json::json!({
        "task_id": task_id,
        "prompt": prompt,
        "project": input.project,
        "harness": input.harness,
        "parent_session_id": input.parent_session_id,
        "task_class": task_class,
    })
}

async fn quota_handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<QuotaQuery>,
) -> impl IntoResponse {
    match token_is_valid(&state.token_store, &query.token) {
        Ok(true) => {}
        Ok(false) => return StatusCode::UNAUTHORIZED.into_response(),
        Err(err) => {
            error!("quota auth failed: {err}");
            return trace_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "auth-check-failed",
                err.to_string(),
            );
        }
    }

    let harness_id = query
        .harness_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let trace_ctx = TraceCtx::new_root(
        "ui.quota.refresh",
        harness_id.as_deref().map_or_else(
            || "quota refresh all harnesses".to_owned(),
            |id| format!("quota refresh {id}"),
        ),
    );
    let payload = serde_json::json!({ "harness_id": harness_id });
    let windows: serde_json::Value = match state
        .nats
        .request_traced(
            "tool.observe.query-quota",
            &payload,
            &trace_ctx,
            state.tool_timeout,
        )
        .await
    {
        Ok(response) => response,
        Err(err) => {
            return trace_error_response(
                StatusCode::BAD_GATEWAY,
                "quota-refresh-failed",
                err.to_string(),
            );
        }
    };

    if let Some(error) = windows.get("error") {
        return trace_error_response(
            StatusCode::BAD_GATEWAY,
            "quota-refresh-rejected",
            error.to_string(),
        );
    }

    Json(QuotaResponse {
        fetched_at: Utc::now(),
        source: "tool.observe.query-quota",
        windows,
    })
    .into_response()
}

async fn trace_replay_handler(
    State(state): State<Arc<AppState>>,
    AxumPath(trace_id): AxumPath<String>,
    Query(query): Query<TraceQuery>,
) -> impl IntoResponse {
    match token_is_valid(&state.token_store, &query.token) {
        Ok(true) => {}
        Ok(false) => return StatusCode::UNAUTHORIZED.into_response(),
        Err(err) => {
            error!("trace replay auth failed: {err}");
            return trace_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "auth-check-failed",
                err.to_string(),
            );
        }
    }

    match trace_replay_from_journal(&state.journal_root, &trace_id, query.max_depth) {
        Ok(replay) => Json(replay).into_response(),
        Err(err) => {
            let status = match err {
                TraceReplayError::InvalidTraceId { .. } => StatusCode::BAD_REQUEST,
                TraceReplayError::JournalRootMissing { .. }
                | TraceReplayError::NoEntries { .. } => StatusCode::NOT_FOUND,
                TraceReplayError::ReadDir { .. }
                | TraceReplayError::Open { .. }
                | TraceReplayError::ReadLine { .. }
                | TraceReplayError::ParseLine { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            };
            trace_error_response(status, "trace-replay-failed", err.to_string())
        }
    }
}

async fn trace_find_handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<TraceFindQuery>,
) -> impl IntoResponse {
    match token_is_valid(&state.token_store, &query.token) {
        Ok(true) => {}
        Ok(false) => return StatusCode::UNAUTHORIZED.into_response(),
        Err(err) => {
            error!("trace find auth failed: {err}");
            return trace_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "auth-check-failed",
                err.to_string(),
            );
        }
    }

    match find_traces_in_journal(&state.journal_root, &query.filter, query.limit) {
        Ok(result) => Json(result).into_response(),
        Err(err) => {
            let status = match &err {
                TraceFindError::InvalidLimit { .. } | TraceFindError::InvalidFilter { .. } => {
                    StatusCode::BAD_REQUEST
                }
                TraceFindError::Journal { source } => match source {
                    TraceReplayError::InvalidTraceId { .. } => StatusCode::BAD_REQUEST,
                    TraceReplayError::JournalRootMissing { .. }
                    | TraceReplayError::NoEntries { .. } => StatusCode::NOT_FOUND,
                    TraceReplayError::ReadDir { .. }
                    | TraceReplayError::Open { .. }
                    | TraceReplayError::ReadLine { .. }
                    | TraceReplayError::ParseLine { .. } => StatusCode::INTERNAL_SERVER_ERROR,
                },
            };
            trace_error_response(status, "trace-find-failed", err.to_string())
        }
    }
}

async fn session_message_handler(
    State(state): State<Arc<AppState>>,
    AxumPath(session_id): AxumPath<String>,
    Query(query): Query<TokenQuery>,
    Json(input): Json<SessionMessageInput>,
) -> impl IntoResponse {
    let user = match token_record(&state.token_store, &query.token) {
        Ok(Some(record)) => record,
        Ok(None) => return StatusCode::UNAUTHORIZED.into_response(),
        Err(err) => {
            error!("session message auth failed: {err}");
            return trace_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "auth-check-failed",
                err.to_string(),
            );
        }
    };

    if let Err(detail) = validate_session_id(&session_id) {
        return trace_error_response(StatusCode::BAD_REQUEST, "invalid-session-id", detail);
    }

    let mode = input.mode;
    let ctx = TraceCtx::new_root(
        "ui.session-message",
        format!("{} message to {session_id}", mode.tool_method()),
    );
    let payload = match message_tool_payload(&session_id, &input, &user.user_id) {
        Ok(payload) => payload,
        Err(err) => return err.into_response(),
    };
    let subject = format!("tool.message.{}", mode.tool_method());
    let response: serde_json::Value = match state
        .nats
        .request_traced(subject, &payload, &ctx, state.tool_timeout)
        .await
    {
        Ok(response) => response,
        Err(err) => {
            return trace_error_response(
                StatusCode::BAD_GATEWAY,
                "message-request-failed",
                err.to_string(),
            );
        }
    };
    if let Some(error) = response.get("error") {
        return trace_error_response(StatusCode::CONFLICT, "message-rejected", error.to_string());
    }
    Json(response).into_response()
}

async fn session_output_handler(
    State(state): State<Arc<AppState>>,
    AxumPath(session_id): AxumPath<String>,
    Query(query): Query<SessionOutputQuery>,
) -> impl IntoResponse {
    match token_is_valid(&state.token_store, &query.token) {
        Ok(true) => {}
        Ok(false) => return StatusCode::UNAUTHORIZED.into_response(),
        Err(err) => {
            error!("session output auth failed: {err}");
            return trace_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "auth-check-failed",
                err.to_string(),
            );
        }
    }

    if let Err(detail) = validate_session_id(&session_id) {
        return trace_error_response(StatusCode::BAD_REQUEST, "invalid-session-id", detail);
    }

    match session_output_records(&state.session_log_root, &session_id, query.limit) {
        Ok(records) => Json(records).into_response(),
        Err(err) => trace_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "session-output-failed",
            err,
        ),
    }
}

fn message_tool_payload(
    session_id: &str,
    input: &SessionMessageInput,
    user_id: &str,
) -> Result<serde_json::Value, ApiError> {
    match input.mode {
        MessageMode::Queue | MessageMode::Interrupt => {
            let Some(text) = input
                .text
                .as_deref()
                .map(str::trim)
                .filter(|text| !text.is_empty())
            else {
                return Err(ApiError {
                    status: StatusCode::BAD_REQUEST,
                    error: "invalid-message",
                    detail: "message text may not be empty".into(),
                });
            };
            Ok(serde_json::json!({
                "session_id": session_id,
                "text": text,
                "from": user_id,
            }))
        }
        MessageMode::FullStop => {
            let Some(reason) = input
                .reason
                .as_deref()
                .or(input.text.as_deref())
                .map(str::trim)
                .filter(|reason| !reason.is_empty())
            else {
                return Err(ApiError {
                    status: StatusCode::BAD_REQUEST,
                    error: "invalid-message",
                    detail: "full-stop reason may not be empty".into(),
                });
            };
            Ok(serde_json::json!({
                "session_id": session_id,
                "reason": reason,
                "from": user_id,
            }))
        }
    }
}

async fn ws_handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<WsQuery>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    match token_is_valid(&state.token_store, &query.token) {
        Ok(true) => ws.on_upgrade(move |socket| stream_nats(socket, state, query.subject)),
        Ok(false) => StatusCode::UNAUTHORIZED.into_response(),
        Err(err) => {
            error!("websocket auth failed: {err}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn stream_nats(mut socket: WebSocket, state: Arc<AppState>, subject: String) {
    let subscription = state.nats.client().subscribe(subject.clone()).await;
    let Ok(mut subscription) = subscription else {
        let _ = socket
            .send(Message::Text(
                serde_json::json!({
                    "error": "nats-subscribe-failed",
                    "subject": subject,
                })
                .to_string()
                .into(),
            ))
            .await;
        return;
    };

    while let Some(message) = subscription.next().await {
        let payload = String::from_utf8_lossy(&message.payload);
        let event = WsEvent {
            subject: message.subject.as_str(),
            payload: payload.into_owned(),
        };
        match serde_json::to_string(&event) {
            Ok(rendered) => {
                if socket.send(Message::Text(rendered.into())).await.is_err() {
                    return;
                }
            }
            Err(err) => {
                error!("websocket event serialize failed: {err}");
                return;
            }
        }
    }
}

fn token_is_valid(store: &TokenStore, token: &str) -> Result<bool, TokenStoreError> {
    store.verify(token).map(|record| record.is_some())
}

fn token_record(store: &TokenStore, token: &str) -> Result<Option<TokenRecord>, TokenStoreError> {
    store.verify(token)
}

fn runtime_services() -> Result<serde_json::Value, String> {
    let binary = std::env::var("JAM_PROCESS_COMPOSE_BIN")
        .unwrap_or_else(|_| "/opt/jam/bin/process-compose".into());
    let socket = std::env::var("JAM_PROCESS_COMPOSE_SOCKET")
        .unwrap_or_else(|_| "/home/maestro/.jam/process-compose.sock".into());
    let output = Command::new(binary)
        .args(["-U", "-u", &socket, "list", "-o", "json"])
        .output()
        .map_err(|err| format!("run process-compose list: {err}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("process-compose list failed: {stderr}"));
    }
    let mut services = serde_json::from_slice::<serde_json::Value>(&output.stdout)
        .map_err(|err| format!("parse process-compose list: {err}"))?;
    append_current_ui_server_if_missing(&mut services);
    Ok(services)
}

fn task_graph_root(jam_home: &Path) -> PathBuf {
    let worktree = std::env::var_os("JAM_CANONICAL_TEMPYR_WORKTREE")
        .or_else(|| std::env::var_os("JAM_TEMPYR_WORKTREE"))
        .map(PathBuf::from);
    let graph_relpath =
        std::env::var_os("JAM_GRAPH_RELPATH").map_or_else(|| PathBuf::from("graph"), PathBuf::from);
    if graph_relpath.is_absolute() {
        return graph_relpath.join("tasks");
    }
    worktree
        .unwrap_or_else(|| jam_home.to_path_buf())
        .join(graph_relpath)
        .join("tasks")
}

fn session_log_root(jam_home: &Path) -> PathBuf {
    std::env::var_os("JAM_SESSION_LOG_ROOT")
        .map_or_else(|| jam_home.join("session-logs"), PathBuf::from)
}

fn task_graph_rows(root: &Path) -> Result<Vec<TaskGraphRow>, String> {
    if !root.is_dir() {
        return Ok(Vec::new());
    }
    let mut rows = Vec::new();
    let entries = fs::read_dir(root).map_err(|err| format!("read {}: {err}", root.display()))?;
    for entry in entries {
        let entry = entry.map_err(|err| format!("read {} entry: {err}", root.display()))?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
            continue;
        }
        if path.file_name().and_then(|name| name.to_str()) == Some(".gitkeep") {
            continue;
        }
        let contents =
            fs::read_to_string(&path).map_err(|err| format!("read {}: {err}", path.display()))?;
        rows.push(task_graph_row_from_markdown(&path, &contents));
    }
    rows.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    Ok(rows)
}

fn task_graph_row_from_markdown(path: &Path, contents: &str) -> TaskGraphRow {
    let fields = frontmatter_fields(contents);
    let fallback_id = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("unknown")
        .to_owned();
    let task_id = fields.get("id").cloned().unwrap_or(fallback_id);
    let description = fields
        .get("description")
        .cloned()
        .or_else(|| first_markdown_heading(contents))
        .unwrap_or_else(|| task_id.clone());
    TaskGraphRow {
        task_id,
        description,
        project: fields.get("project").cloned().unwrap_or_else(|| "-".into()),
        task_class: fields
            .get("task-class")
            .cloned()
            .or_else(|| fields.get("task_class").cloned())
            .unwrap_or_else(|| "-".into()),
        priority: fields
            .get("priority")
            .cloned()
            .unwrap_or_else(|| "-".into()),
        status: fields
            .get("status")
            .cloned()
            .unwrap_or_else(|| "unknown".into()),
        requested_by: fields
            .get("requested-by")
            .cloned()
            .or_else(|| fields.get("requested_by").cloned())
            .unwrap_or_else(|| "-".into()),
        pr_ref: fields
            .get("pr-ref")
            .cloned()
            .or_else(|| fields.get("pr_ref").cloned())
            .unwrap_or_else(|| "-".into()),
        session_id: fields
            .get("session-id")
            .cloned()
            .or_else(|| fields.get("session_id").cloned())
            .unwrap_or_else(|| "-".into()),
        harness: fields.get("harness").cloned().unwrap_or_else(|| "-".into()),
        outcome: fields.get("outcome").cloned().unwrap_or_else(|| "-".into()),
        trace_id: fields
            .get("trace-id")
            .cloned()
            .or_else(|| fields.get("trace_id").cloned())
            .unwrap_or_else(|| "-".into()),
        updated_at: fields
            .get("last-updated")
            .cloned()
            .or_else(|| fields.get("updated").cloned())
            .unwrap_or_else(|| "-".into()),
    }
}

fn frontmatter_fields(contents: &str) -> HashMap<String, String> {
    let mut lines = contents.lines();
    if lines.next() != Some("---") {
        return HashMap::new();
    }
    let mut fields = HashMap::new();
    for line in lines {
        if line.trim() == "---" {
            break;
        }
        if line.starts_with(' ') || line.starts_with('-') {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim();
        let value = clean_frontmatter_value(value);
        if !key.is_empty() && !value.is_empty() {
            fields.insert(key.to_owned(), value);
        }
    }
    fields
}

fn clean_frontmatter_value(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.len() >= 2
        && ((trimmed.starts_with('\'') && trimmed.ends_with('\''))
            || (trimmed.starts_with('"') && trimmed.ends_with('"')))
    {
        trimmed[1..trimmed.len() - 1].to_owned()
    } else {
        trimmed.to_owned()
    }
}

fn first_markdown_heading(contents: &str) -> Option<String> {
    contents
        .lines()
        .filter_map(|line| line.strip_prefix("# "))
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| {
            line.strip_prefix("Task: ")
                .map_or_else(|| line.to_owned(), ToOwned::to_owned)
        })
}

fn normalize_required_input(value: &str, field: &'static str) -> Result<String, ApiError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(ApiError {
            status: StatusCode::BAD_REQUEST,
            error: "invalid-task",
            detail: format!("{field} must not be empty"),
        });
    }
    Ok(trimmed.to_owned())
}

fn normalize_optional_input(
    value: Option<&str>,
    default: &str,
    field: &'static str,
) -> Result<String, ApiError> {
    match value.map(str::trim).filter(|value| !value.is_empty()) {
        Some(value) => normalize_required_input(value, field),
        None => Ok(default.to_owned()),
    }
}

fn normalize_priority(value: &str) -> Result<String, ApiError> {
    let priority = normalize_required_input(value, "priority")?;
    match priority.as_str() {
        "low" | "normal" | "high" => Ok(priority),
        _ => Err(ApiError {
            status: StatusCode::BAD_REQUEST,
            error: "invalid-task",
            detail: "priority must be low, normal, or high".into(),
        }),
    }
}

fn normalize_project(value: Option<&str>) -> Result<String, ApiError> {
    let project = normalize_optional_input(value, "blueberry", "project")?;
    match project.as_str() {
        "blueberry" | "jamboree" => Ok(project),
        _ => Err(ApiError {
            status: StatusCode::BAD_REQUEST,
            error: "invalid-task",
            detail: "project must be blueberry or jamboree".into(),
        }),
    }
}

fn task_id_for(_description: &str, trace_ctx: &TraceCtx) -> String {
    let date = Utc::now().format("%y%m%d");
    let trace = trace_ctx.trace_id.to_string().to_ascii_lowercase();
    let suffix = &trace[trace.len() - 8..];
    format!("t-{date}-{suffix}")
}

fn append_current_ui_server_if_missing(services: &mut serde_json::Value) {
    if let Some(items) = services.as_array_mut() {
        if items.iter().any(service_is_running_ui_server) {
            return;
        }
        items.push(serde_json::json!({
            "name": "ui-server-current",
            "namespace": "default",
            "status": "Running",
            "system_time": "-",
            "age": 0,
            "is_ready": "Ready",
            "restarts": 0,
            "exit_code": 0,
            "pid": std::process::id(),
            "IsRunning": true
        }));
    }
}

fn service_is_running_ui_server(service: &serde_json::Value) -> bool {
    service.get("name").and_then(serde_json::Value::as_str) == Some("ui-server")
        && (service
            .get("IsRunning")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
            || service.get("status").and_then(serde_json::Value::as_str) == Some("Running"))
}

fn recent_journal_events(
    journal_root: &Path,
    subject_filter: &str,
    limit: usize,
) -> Result<Vec<OwnedWsEvent>, String> {
    if limit == 0 {
        return Err("limit must be greater than zero".into());
    }
    let limit = limit.min(500);
    if !journal_root.exists() {
        return Ok(Vec::new());
    }

    let mut events = Vec::new();
    for path in journal_jsonl_paths(journal_root)? {
        let file = File::open(&path).map_err(|err| format!("open {}: {err}", path.display()))?;
        for (idx, line) in BufReader::new(file).lines().enumerate() {
            let line_number = idx + 1;
            let line =
                line.map_err(|err| format!("read {} line {line_number}: {err}", path.display()))?;
            if line.trim().is_empty() {
                continue;
            }
            let envelope = serde_json::from_str::<RecentJournalEnvelope>(&line)
                .map_err(|err| format!("parse {} line {line_number}: {err}", path.display()))?;
            let subject = format!("journal.{}", envelope.event_type);
            if !subject_matches(subject_filter, &subject) {
                continue;
            }
            events.push(RecentJournalEvent {
                subject,
                payload: line,
                timestamp: envelope.timestamp,
                path: path.clone(),
                line_number,
            });
        }
    }

    events.sort_by(|left, right| {
        right
            .timestamp
            .cmp(&left.timestamp)
            .then_with(|| right.path.cmp(&left.path))
            .then_with(|| right.line_number.cmp(&left.line_number))
    });

    Ok(events
        .into_iter()
        .take(limit)
        .map(|event| OwnedWsEvent {
            subject: event.subject,
            payload: event.payload,
        })
        .collect())
}

fn session_output_records(
    root: &Path,
    session_id: &str,
    limit: usize,
) -> Result<Vec<serde_json::Value>, String> {
    if limit == 0 {
        return Err("limit must be greater than zero".into());
    }
    let limit = limit.min(1000);
    let path = root.join(format!("{session_id}.jsonl"));
    if !path.exists() {
        return Ok(Vec::new());
    }
    let file = File::open(&path).map_err(|err| format!("open {}: {err}", path.display()))?;
    let mut records = VecDeque::with_capacity(limit);
    for (idx, line) in BufReader::new(file).lines().enumerate() {
        let line_number = idx + 1;
        let line =
            line.map_err(|err| format!("read {} line {line_number}: {err}", path.display()))?;
        if line.trim().is_empty() {
            continue;
        }
        let Ok(record) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        if records.len() == limit {
            records.pop_front();
        }
        records.push_back(record);
    }
    Ok(records.into_iter().collect())
}

fn journal_jsonl_paths(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut paths = Vec::new();
    for day in fs::read_dir(root).map_err(|err| format!("read {}: {err}", root.display()))? {
        let day = day.map_err(|err| format!("read {}: {err}", root.display()))?;
        let day_path = day.path();
        if !day_path.is_dir() {
            continue;
        }
        for entry in
            fs::read_dir(&day_path).map_err(|err| format!("read {}: {err}", day_path.display()))?
        {
            let entry = entry.map_err(|err| format!("read {}: {err}", day_path.display()))?;
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "jsonl") {
                paths.push(path);
            }
        }
    }
    paths.sort();
    Ok(paths)
}

fn subject_matches(filter: &str, subject: &str) -> bool {
    let filter_tokens: Vec<_> = filter.split('.').collect();
    let subject_tokens: Vec<_> = subject.split('.').collect();
    let mut subject_idx = 0;

    for token in filter_tokens {
        if token == ">" {
            return true;
        }
        if subject_idx >= subject_tokens.len() {
            return false;
        }
        if token != "*" && token != subject_tokens[subject_idx] {
            return false;
        }
        subject_idx += 1;
    }

    subject_idx == subject_tokens.len()
}

fn validate_session_id(session_id: &str) -> Result<(), String> {
    if session_id.is_empty() || session_id.len() > 128 {
        return Err("session_id must be 1-128 characters".into());
    }
    if session_id.contains("..") {
        return Err("session_id may not contain '..'".into());
    }
    if !session_id
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | ':'))
    {
        return Err("session_id may only contain ASCII letters, numbers, '-', '_', and ':'".into());
    }
    Ok(())
}

fn validate_task_id(task_id: &str) -> Result<(), String> {
    if task_id.is_empty() || task_id.len() > 128 {
        return Err("task_id must be 1-128 characters".into());
    }
    if task_id.contains("..") {
        return Err("task_id may not contain '..'".into());
    }
    if !task_id
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
    {
        return Err("task_id may only contain ASCII letters, numbers, '-', and '_'".into());
    }
    Ok(())
}

fn load_allow_bind_addrs(jam_home: &Path) -> Result<String, UiServerError> {
    if let Ok(raw) = std::env::var("JAM_UI_ALLOW_BIND_ADDRS") {
        return Ok(raw);
    }
    let config_path = std::env::var_os("JAM_UI_CONFIG")
        .map_or_else(|| jam_home.join("config").join("ui.toml"), PathBuf::from);
    if !config_path.exists() {
        return Ok(DEFAULT_ALLOW_BIND_ADDRS.into());
    }
    let raw = fs::read_to_string(&config_path).map_err(|err| {
        UiServerError::UiConfig(format!("failed to read {}: {err}", config_path.display()))
    })?;
    let config: UiConfigFile = toml::from_str(&raw).map_err(|err| {
        UiServerError::UiConfig(format!("failed to parse {}: {err}", config_path.display()))
    })?;
    Ok(config
        .auth
        .and_then(|auth| auth.allow_bind_addrs)
        .filter(|addrs| !addrs.is_empty())
        .map_or_else(|| DEFAULT_ALLOW_BIND_ADDRS.into(), |addrs| addrs.join(",")))
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum BindAllow {
    Exact(IpAddr),
    Ipv4Cidr { network: u32, mask: u32 },
}

fn parse_bind_allowlist(raw: &str) -> Result<Vec<BindAllow>, String> {
    let mut allowlist = Vec::new();
    for token in raw.split(',').map(str::trim) {
        if token.is_empty() {
            return Err("allow-bind-addrs contains an empty entry".into());
        }
        allowlist.push(parse_bind_allow(token)?);
    }
    if allowlist.is_empty() {
        return Err("allow-bind-addrs must contain at least one address or CIDR".into());
    }
    Ok(allowlist)
}

fn parse_bind_allow(token: &str) -> Result<BindAllow, String> {
    if let Some((addr, prefix)) = token.split_once('/') {
        let addr: Ipv4Addr = addr
            .parse()
            .map_err(|err| format!("{token} has invalid IPv4 CIDR address: {err}"))?;
        let prefix: u8 = prefix
            .parse()
            .map_err(|err| format!("{token} has invalid CIDR prefix: {err}"))?;
        if prefix > 32 {
            return Err(format!("{token} has CIDR prefix > 32"));
        }
        let mask = if prefix == 0 {
            0
        } else {
            u32::MAX << (32 - u32::from(prefix))
        };
        return Ok(BindAllow::Ipv4Cidr {
            network: u32::from(addr) & mask,
            mask,
        });
    }
    let addr = token
        .parse()
        .map_err(|err| format!("{token} is not an IP address or IPv4 CIDR: {err}"))?;
    Ok(BindAllow::Exact(addr))
}

fn bind_addr_allowed(addr: IpAddr, allowlist: &[BindAllow]) -> bool {
    allowlist.iter().any(|allowed| match (addr, allowed) {
        (IpAddr::V4(addr), BindAllow::Ipv4Cidr { network, mask }) => {
            (u32::from(addr) & mask) == *network
        }
        (addr, BindAllow::Exact(allowed)) => addr == *allowed,
        (IpAddr::V6(_), BindAllow::Ipv4Cidr { .. }) => false,
    })
}

fn default_subject() -> String {
    "journal.>".into()
}

const fn default_max_depth() -> u32 {
    5
}

const fn default_trace_find_limit() -> usize {
    25
}

const fn default_recent_events_limit() -> usize {
    200
}

const fn default_session_output_limit() -> usize {
    300
}

fn trace_error_response(
    status: StatusCode,
    error: &'static str,
    detail: String,
) -> axum::response::Response {
    (status, Json(ErrorResponse { error, detail })).into_response()
}

fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        tracing_subscriber::EnvFilter::new("jam_ui_server=info,tower_http=info")
    });
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_verifier_accepts_issued_token() {
        let tmp = tempfile::tempdir().unwrap();
        let store = TokenStore::from_jam_home(tmp.path());
        let issued = store.issue("human:caleb").unwrap();

        assert!(token_is_valid(&store, &issued.token).unwrap());
    }

    #[test]
    fn token_verifier_rejects_revoked_token() {
        let tmp = tempfile::tempdir().unwrap();
        let store = TokenStore::from_jam_home(tmp.path());
        let issued = store.issue("human:caleb").unwrap();

        assert!(store.revoke(&issued.id).unwrap());
        assert!(!token_is_valid(&store, &issued.token).unwrap());
    }

    #[test]
    fn session_id_validation_allows_picker_handles_but_rejects_subject_wildcards() {
        assert!(validate_session_id("codex-cli:01BRZ3NDEKTSV4RRFFQ69G5FAV").is_ok());
        assert!(validate_session_id("codex.cli:bad").is_err());
        assert!(validate_session_id("codex-cli:>").is_err());
        assert!(validate_session_id("../codex").is_err());
    }

    #[test]
    fn task_resume_payload_carries_project_harness_and_parent_session() {
        let payload = task_resume_payload(
            "task-resume-claude".into(),
            "continue and create the PR".into(),
            TaskResumeInput {
                prompt: "continue and create the PR".into(),
                project: Some("jamboree".into()),
                harness: Some("claude-code".into()),
                parent_session_id: Some("claude-code:old-session".into()),
                task_class: Some("jamboree-self-modification".into()),
            },
            "jamboree-self-modification".into(),
        );

        assert_eq!(
            payload,
            serde_json::json!({
                "task_id": "task-resume-claude",
                "prompt": "continue and create the PR",
                "project": "jamboree",
                "harness": "claude-code",
                "parent_session_id": "claude-code:old-session",
                "task_class": "jamboree-self-modification",
            })
        );
    }

    #[test]
    fn generated_task_ids_are_short_and_referenceable() {
        let trace = TraceCtx::new_root("test.task", "spawn a task");
        let task_id = task_id_for(
            "Pick a small task and implement or fix it with a long description",
            &trace,
        );

        assert!(task_id.starts_with("t-"));
        assert!(task_id.len() <= 18, "{task_id}");
        assert!(validate_task_id(&task_id).is_ok());
        assert!(!task_id.contains("pick-a-small-task"));
    }

    #[test]
    fn bind_allowlist_accepts_localhost_and_tailscale_cgnat() {
        let allowlist = parse_bind_allowlist(DEFAULT_ALLOW_BIND_ADDRS).unwrap();

        assert!(bind_addr_allowed("127.0.0.1".parse().unwrap(), &allowlist));
        assert!(bind_addr_allowed("100.64.0.1".parse().unwrap(), &allowlist));
        assert!(bind_addr_allowed(
            "100.127.255.254".parse().unwrap(),
            &allowlist
        ));
    }

    #[test]
    fn bind_allowlist_rejects_public_and_private_lan_addrs_by_default() {
        let allowlist = parse_bind_allowlist(DEFAULT_ALLOW_BIND_ADDRS).unwrap();

        assert!(!bind_addr_allowed("0.0.0.0".parse().unwrap(), &allowlist));
        assert!(!bind_addr_allowed(
            "192.168.1.10".parse().unwrap(),
            &allowlist
        ));
        assert!(!bind_addr_allowed("8.8.8.8".parse().unwrap(), &allowlist));
    }

    #[test]
    fn bind_allowlist_rejects_invalid_cidr() {
        let err = parse_bind_allowlist("127.0.0.1,100.64.0.0/33").unwrap_err();

        assert!(err.contains("prefix > 32"));
    }

    #[test]
    fn subject_filter_supports_nats_wildcards() {
        assert!(subject_matches("journal.>", "journal.task.requested"));
        assert!(subject_matches(
            "journal.*.requested",
            "journal.task.requested"
        ));
        assert!(subject_matches("notify.human", "notify.human"));
        assert!(!subject_matches("journal.task", "journal.task.requested"));
        assert!(!subject_matches(
            "journal.*.requested",
            "journal.test.smoke"
        ));
    }

    #[test]
    fn recent_journal_events_returns_newest_matching_events() {
        let tmp = tempfile::tempdir().unwrap();
        let day = tmp.path().join("journal").join("2026-05-07");
        std::fs::create_dir_all(&day).unwrap();
        std::fs::write(
            day.join("journal.task.jsonl"),
            concat!(
                r#"{"schema_version":1,"event_type":"task.requested","event_subtype_version":1,"timestamp":"2026-05-07T00:00:00Z","journal_seq":0,"trace_id":"01KR0000000000000000000000","actor":"test","payload":{"task_id":"old"}}"#,
                "\n",
                r#"{"schema_version":1,"event_type":"task.requested","event_subtype_version":1,"timestamp":"2026-05-07T00:01:00Z","journal_seq":0,"trace_id":"01KR0000000000000000000001","actor":"test","payload":{"task_id":"new"}}"#,
                "\n"
            ),
        )
        .unwrap();
        std::fs::write(
            day.join("journal.test.jsonl"),
            concat!(
                r#"{"schema_version":1,"event_type":"test.smoke","event_subtype_version":1,"timestamp":"2026-05-07T00:02:00Z","journal_seq":0,"trace_id":"01KR0000000000000000000002","actor":"test","payload":{}}"#,
                "\n"
            ),
        )
        .unwrap();

        let events = recent_journal_events(&tmp.path().join("journal"), "journal.task.>", 10)
            .expect("recent events");

        assert_eq!(events.len(), 2);
        assert_eq!(events[0].subject, "journal.task.requested");
        assert!(events[0].payload.contains(r#""task_id":"new""#));
        assert!(events[1].payload.contains(r#""task_id":"old""#));
    }

    #[test]
    fn runtime_services_adds_current_ui_server_only_when_missing() {
        let mut missing = serde_json::json!([
            {
                "name": "nats",
                "status": "Running",
                "IsRunning": true
            }
        ]);

        append_current_ui_server_if_missing(&mut missing);

        assert_eq!(missing.as_array().unwrap().len(), 2);
        assert_eq!(missing[1]["name"], "ui-server-current");

        let mut listed = serde_json::json!([
            {
                "name": "ui-server",
                "status": "Running",
                "IsRunning": true
            }
        ]);

        append_current_ui_server_if_missing(&mut listed);

        assert_eq!(listed.as_array().unwrap().len(), 1);
        assert_eq!(listed[0]["name"], "ui-server");
    }

    #[test]
    fn task_project_normalization_requires_explicit_supported_target() {
        assert_eq!(normalize_project(None).unwrap(), "blueberry");
        assert_eq!(normalize_project(Some(" jamboree ")).unwrap(), "jamboree");

        let err = normalize_project(Some("autoberry")).unwrap_err();

        assert_eq!(err.error, "invalid-task");
        assert!(err.detail.contains("blueberry or jamboree"));
    }

    #[test]
    fn task_graph_rows_parse_lifecycle_frontmatter() {
        let tmp = tempfile::tempdir().unwrap();
        let tasks = tmp.path().join("graph").join("tasks");
        std::fs::create_dir_all(&tasks).unwrap();
        std::fs::write(
            tasks.join("task-1.md"),
            r#"---
id: task-1
type: task
status: merged
updated: 2026-05-08T04:39:15Z
description: 'Smoke task'
project: blueberry
task-class: light-edit
priority: low
requested-by: human:caleb
pr-ref: cleak/blueberry#389
---
Lifecycle task node.
"#,
        )
        .unwrap();

        let rows = task_graph_rows(&tasks).unwrap();

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].task_id, "task-1");
        assert_eq!(rows[0].description, "Smoke task");
        assert_eq!(rows[0].status, "merged");
        assert_eq!(rows[0].pr_ref, "cleak/blueberry#389");
    }

    #[test]
    fn task_graph_row_uses_heading_when_description_missing() {
        let row = task_graph_row_from_markdown(
            Path::new("task-root-motion.md"),
            r#"---
id: task-root-motion
status: in_progress
---
# Task: Root Motion Authority
"#,
        );

        assert_eq!(row.description, "Root Motion Authority");
        assert_eq!(row.status, "in_progress");
    }

    #[test]
    fn allow_bind_addrs_loads_from_ui_config() {
        let tmp = tempfile::tempdir().unwrap();
        let config_dir = tmp.path().join("config");
        std::fs::create_dir(&config_dir).unwrap();
        std::fs::write(
            config_dir.join("ui.toml"),
            r#"[auth]
allow-bind-addrs = ["127.0.0.1", "100.64.0.0/10"]
"#,
        )
        .unwrap();

        let loaded = load_allow_bind_addrs(tmp.path()).unwrap();

        assert_eq!(loaded, DEFAULT_ALLOW_BIND_ADDRS);
    }
}
