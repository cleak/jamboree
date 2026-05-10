//! `jam-patch-agent` - supervises hot-patches after manifest swap.
//!
//! The agent reacts to `patch.applied`, runs deterministic checks first, uses
//! the existing mechanical rollback path on failures, and writes an incident
//! bundle before notifying the Manager when recovery is not possible.

#![deny(missing_docs)]

mod patch_ops;

use chrono::{SecondsFormat, Utc};
use clap::Parser;
use futures::StreamExt;
use jam_events::generated::{
    Event, PatchApplied, PatchConfirmed, PatchFailed, PatchRollbackRequested,
    PatchRolledBackSuccessfully, PatchStaged,
};
use jam_events::EventEnvelope;
use jam_nats::JamNats;
use jam_trace::{TraceCtx, TraceId};
use patch_ops::{ApplyRequest, RollbackRequest};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command as StdCommand, Stdio};
use std::str::FromStr;
use std::time::Duration;
use tokio::process::Command;
use tracing::{error, info, warn};

const SERVICE_NAME: &str = "jam-patch-agent";
const SERVICE_VERSION: &str = env!("CARGO_PKG_VERSION");
const DEFAULT_NATS_URL: &str = "nats://127.0.0.1:4222";
const DEFAULT_REQUEST_TIMEOUT_SECS: u64 = 5;
const DEFAULT_COMMAND_TIMEOUT_SECS: u64 = 30;
const DEFAULT_LLM_TIMEOUT_SECS: u64 = 60;
const DEFAULT_LLM_BUDGET_USD: f64 = 0.50;
const DISPATCH_STATE_BUCKET: &str = "dispatch-state";
const DISPATCH_PAUSED_KEY: &str = "paused";
const DISPATCH_STATE_KEY: &str = "state.json";
const MAX_OUTPUT_BYTES: usize = 12_000;

#[derive(Debug, Parser)]
#[command(name = SERVICE_NAME, version, about = "Supervise hot-patches")]
struct Cli {
    /// NATS URL (defaults to NATS_URL or nats://127.0.0.1:4222).
    #[arg(long)]
    nats_url: Option<String>,

    /// Path to the jam CLI used for mechanical rollback and jam doctor.
    #[arg(long)]
    jam_bin: Option<PathBuf>,

    /// Stop after this many patch.applied events; useful for smoke tests.
    #[arg(long)]
    max_events: Option<u64>,
}

#[derive(Debug, thiserror::Error)]
enum AgentError {
    #[error("nats: {0}")]
    Nats(#[from] jam_nats::NatsError),

    #[error("subscribe: {0}")]
    Subscribe(String),

    #[error("json: {0}")]
    Json(#[from] serde_json::Error),

    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("trace: {0}")]
    Trace(#[from] jam_trace::TraceIdParseError),

    #[error("config: {0}")]
    Config(String),

    #[error("command: {0}")]
    Command(String),

    #[error("unrecoverable: {0}")]
    Unrecoverable(String),
}

#[derive(Debug, Clone)]
struct Config {
    nats_url: String,
    nats_token: Option<String>,
    jam_home: PathBuf,
    jam_bin: PathBuf,
    request_timeout: Duration,
    command_timeout: Duration,
    llm_timeout: Duration,
    llm_budget_usd: f64,
    llm_command: Option<Vec<String>>,
    doctor_enabled: bool,
    failed_event_check_enabled: bool,
}

impl Config {
    fn from_env_and_cli(cli: &Cli) -> Result<Self, AgentError> {
        let nats_url = cli
            .nats_url
            .clone()
            .or_else(|| std::env::var("NATS_URL").ok())
            .unwrap_or_else(|| DEFAULT_NATS_URL.into());
        let jam_bin = cli
            .jam_bin
            .clone()
            .or_else(|| std::env::var_os("JAM_BIN").map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("jam"));
        let llm_command = std::env::var("JAM_PATCH_AGENT_LLM_CMD")
            .ok()
            .map(|raw| split_command(&raw))
            .transpose()?;
        Ok(Self {
            nats_url,
            nats_token: std::env::var("NATS_TOKEN")
                .ok()
                .filter(|token| !token.trim().is_empty()),
            jam_home: jam_tools_core::paths::jam_home(),
            jam_bin,
            request_timeout: duration_env(
                "JAM_PATCH_AGENT_REQUEST_TIMEOUT_SECS",
                DEFAULT_REQUEST_TIMEOUT_SECS,
            ),
            command_timeout: duration_env(
                "JAM_PATCH_AGENT_COMMAND_TIMEOUT_SECS",
                DEFAULT_COMMAND_TIMEOUT_SECS,
            ),
            llm_timeout: duration_env("JAM_PATCH_AGENT_LLM_TIMEOUT_SECS", DEFAULT_LLM_TIMEOUT_SECS),
            llm_budget_usd: std::env::var("JAM_PATCH_AGENT_LLM_BUDGET_USD")
                .ok()
                .and_then(|raw| raw.parse::<f64>().ok())
                .filter(|value| *value > 0.0)
                .unwrap_or(DEFAULT_LLM_BUDGET_USD),
            llm_command,
            doctor_enabled: !parse_bool_env("JAM_PATCH_AGENT_SKIP_DOCTOR").unwrap_or(false),
            failed_event_check_enabled: !parse_bool_env("JAM_PATCH_AGENT_SKIP_FAILED_EVENT_CHECK")
                .unwrap_or(false),
        })
    }
}

#[derive(Debug, Serialize)]
struct CheckReport {
    stage: String,
    service: String,
    version: String,
    subject_prefix: String,
    passed: bool,
    checks: Vec<CheckResult>,
}

impl CheckReport {
    fn failed_details(&self) -> Vec<String> {
        self.checks
            .iter()
            .filter(|check| check.status == CheckStatus::Failed)
            .map(|check| format!("{}: {}", check.name, check.detail))
            .collect()
    }

    fn checks_run(&self) -> u32 {
        self.checks
            .iter()
            .filter(|check| check.status != CheckStatus::Skipped)
            .count()
            .try_into()
            .unwrap_or(u32::MAX)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum CheckStatus {
    Passed,
    Failed,
    Skipped,
}

#[derive(Debug, Serialize)]
struct CheckResult {
    name: String,
    status: CheckStatus,
    detail: String,
    started_at: String,
    finished_at: String,
}

#[derive(Debug, Serialize)]
struct CommandReport {
    program: String,
    args: Vec<String>,
    status: String,
    stdout: String,
    stderr: String,
}

#[derive(Debug, Serialize)]
struct LlmReport {
    attempted: bool,
    budget_cap_usd: f64,
    status: String,
    suggestion: Option<String>,
    stdout: String,
    stderr: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    recovery: Option<LlmRecoveryReport>,
}

#[derive(Debug, Serialize)]
struct LlmRecoveryReport {
    action: String,
    status: String,
    detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    command: Option<CommandReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    health: Option<CheckReport>,
}

struct LlmRecoveryOutcome {
    recovered: bool,
    report: LlmRecoveryReport,
}

#[derive(Debug, Serialize)]
struct IncidentSummary<'a> {
    incident_id: &'a str,
    service: &'a str,
    attempted_version: &'a str,
    subject_prefix: &'a str,
    summary: &'a str,
}

#[derive(Debug, Serialize)]
struct NotifyHumanDetail<'a> {
    service: &'a str,
    incident_id: &'a str,
    incident_dir: &'a str,
}

#[derive(Debug, Serialize)]
struct DispatchPauseRecord {
    dispatch_paused: bool,
    reason: Option<String>,
    changed_at: chrono::DateTime<Utc>,
    changed_by: String,
}

struct FailureRecord<'a> {
    applied: &'a PatchApplied,
    post_apply: &'a CheckReport,
    post_rollback: Option<&'a CheckReport>,
    rollback: Option<&'a CommandReport>,
    llm: &'a LlmReport,
    summary: &'a str,
}

enum ProcessOutcome {
    Continue,
    Fatal,
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    init_tracing();
    match Box::pin(run()).await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(err) => {
            error!("{err}");
            std::process::ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), AgentError> {
    let cli = Cli::parse();
    let config = Config::from_env_and_cli(&cli)?;
    info!(
        service = SERVICE_NAME,
        version = SERVICE_VERSION,
        nats = %config.nats_url,
        jam_home = %config.jam_home.display(),
        jam_bin = %config.jam_bin.display(),
        "starting",
    );

    let nats = JamNats::connect(&config.nats_url, config.nats_token.clone()).await?;
    jam_nats::ensure_streams(nats.jetstream(), &jam_nats::default_streams()).await?;
    jam_nats::ensure_kv_buckets(nats.jetstream(), &jam_nats::default_kv_buckets()).await?;

    let mut applied_sub = nats
        .client()
        .subscribe(PatchApplied::EVENT_TYPE)
        .await
        .map_err(|err| AgentError::Subscribe(err.to_string()))?;
    let mut staged_sub = nats
        .client()
        .subscribe(PatchStaged::EVENT_TYPE)
        .await
        .map_err(|err| AgentError::Subscribe(err.to_string()))?;
    let mut rollback_sub = nats
        .client()
        .subscribe(PatchRollbackRequested::EVENT_TYPE)
        .await
        .map_err(|err| AgentError::Subscribe(err.to_string()))?;
    info!(
        applied = PatchApplied::EVENT_TYPE,
        staged = PatchStaged::EVENT_TYPE,
        rollback = PatchRollbackRequested::EVENT_TYPE,
        "subscribed"
    );

    let shutdown = tokio::signal::ctrl_c();
    tokio::pin!(shutdown);
    let mut handled = 0_u64;
    let mut seen = HashSet::new();

    loop {
        tokio::select! {
            _ = &mut shutdown => {
                info!("shutdown signal received");
                return Ok(());
            }
            message = applied_sub.next() => {
                let Some(message) = message else {
                    warn!("patch.applied subscription closed");
                    return Ok(());
                };
                let envelope = parse_event_envelope::<PatchApplied>(&message.payload)?;
                let key = format!(
                    "applied:{}:{}:{}",
                    envelope.trace_id, envelope.payload.service, envelope.payload.to_version
                );
                if seen.insert(key) {
                    match process_patch_applied(&nats, &config, envelope).await? {
                        ProcessOutcome::Continue => {}
                        ProcessOutcome::Fatal => return Err(AgentError::Unrecoverable(
                            "patch recovery failed; incident was published".into(),
                        )),
                    }
                }
                handled = handled.saturating_add(1);
                if cli.max_events.is_some_and(|max_events| handled >= max_events) {
                    info!(handled, "max events reached");
                    return Ok(());
                }
            }
            message = staged_sub.next() => {
                let Some(message) = message else {
                    warn!("patch.staged subscription closed");
                    return Ok(());
                };
                let envelope = parse_event_envelope::<PatchStaged>(&message.payload)?;
                let key = format!(
                    "staged:{}:{}:{}",
                    envelope.trace_id, envelope.payload.service, envelope.payload.version
                );
                if seen.insert(key) {
                    process_patch_staged(&nats, &config, envelope).await;
                }
            }
            message = rollback_sub.next() => {
                let Some(message) = message else {
                    warn!("patch.rollback-requested subscription closed");
                    return Ok(());
                };
                let envelope = parse_event_envelope::<PatchRollbackRequested>(&message.payload)?;
                let key = format!(
                    "rollback:{}:{}",
                    envelope.trace_id, envelope.payload.service
                );
                if seen.insert(key) {
                    process_patch_rollback_requested(&nats, envelope).await;
                }
            }
        }
    }
}

fn parse_event_envelope<P>(payload: &[u8]) -> Result<EventEnvelope<P>, AgentError>
where
    P: Event + serde::de::DeserializeOwned,
{
    let envelope: EventEnvelope<P> = serde_json::from_slice(payload)?;
    if envelope.event_type != P::EVENT_TYPE {
        return Err(AgentError::Config(format!(
            "expected event_type {}, got {}",
            P::EVENT_TYPE,
            envelope.event_type
        )));
    }
    Ok(envelope)
}

async fn process_patch_staged(
    nats: &JamNats,
    config: &Config,
    envelope: EventEnvelope<PatchStaged>,
) {
    let ctx = match trace_ctx_from_envelope(&envelope) {
        Ok(ctx) => ctx,
        Err(err) => {
            warn!("patch.staged envelope is malformed: {err}");
            return;
        }
    };
    let staged = envelope.payload;
    info!(
        service = %staged.service,
        version = %staged.version,
        path = %staged.staging_path,
        "patch.staged received; running §20.3 atomic-swap",
    );
    let request = ApplyRequest {
        service: staged.service.clone(),
        version: staged.version.clone(),
        staging_path: PathBuf::from(&staged.staging_path),
        expected_sha256: staged.binary_sha256.clone(),
        requested_by: staged.requested_by.clone(),
        trace_ctx: ctx,
        nats_url: config.nats_url.clone(),
        nats_token: config.nats_token.clone(),
    };
    match patch_ops::apply_staged_patch(nats, request).await {
        Ok(updated) => {
            info!(
                manifest_id = %updated.manifest_id,
                revision = updated.revision,
                "atomic-swap complete; patch.applied emitted",
            );
        }
        Err(err) => {
            warn!("apply_staged_patch failed: {err}");
        }
    }
}

async fn process_patch_rollback_requested(
    nats: &JamNats,
    envelope: EventEnvelope<PatchRollbackRequested>,
) {
    let ctx = match trace_ctx_from_envelope(&envelope) {
        Ok(ctx) => ctx,
        Err(err) => {
            warn!("patch.rollback-requested envelope is malformed: {err}");
            return;
        }
    };
    let req_payload = envelope.payload;
    info!(
        service = %req_payload.service,
        reason = %req_payload.reason,
        "patch.rollback-requested received; running §20.4 rollback",
    );
    let request = RollbackRequest {
        service: req_payload.service.clone(),
        reason: req_payload.reason.clone(),
        requested_by: req_payload.requested_by.clone(),
        trace_ctx: ctx,
    };
    match patch_ops::perform_rollback(nats, request).await {
        Ok(updated) => {
            info!(
                manifest_id = %updated.manifest_id,
                revision = updated.revision,
                "rollback complete; patch.rolled-back emitted",
            );
        }
        Err(err) => {
            warn!("perform_rollback failed: {err}");
        }
    }
}

fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("jam_patch_agent=info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

async fn process_patch_applied(
    nats: &JamNats,
    config: &Config,
    envelope: EventEnvelope<PatchApplied>,
) -> Result<ProcessOutcome, AgentError> {
    let ctx = trace_ctx_from_envelope(&envelope)?;
    let applied = envelope.payload;
    info!(
        service = %applied.service,
        from = %applied.from_version,
        to = %applied.to_version,
        subject_prefix = %applied.subject_prefix,
        "patch applied; running deterministic checks",
    );

    let post_apply = run_health_checks(
        nats,
        config,
        "post-apply",
        &applied.service,
        &applied.to_version,
        &applied.subject_prefix,
        &ctx,
    )
    .await;
    if post_apply.passed {
        publish_event(
            nats,
            PatchConfirmed {
                service: applied.service.clone(),
                version: applied.to_version.clone(),
                checks_run: post_apply.checks_run(),
                ts: Utc::now(),
            },
            &ctx,
        )
        .await?;
        return Ok(ProcessOutcome::Continue);
    }

    recover_failed_patch(nats, config, &ctx, &applied, &post_apply).await
}

async fn recover_failed_patch(
    nats: &JamNats,
    config: &Config,
    ctx: &TraceCtx,
    applied: &PatchApplied,
    post_apply: &CheckReport,
) -> Result<ProcessOutcome, AgentError> {
    warn!(
        service = %applied.service,
        failures = ?post_apply.failed_details(),
        "deterministic checks failed; invoking mechanical rollback",
    );
    let rollback_reason = format!(
        "patch-agent deterministic checks failed for {} {}: {}",
        applied.service,
        applied.to_version,
        post_apply.failed_details().join("; ")
    );
    let rollback_report =
        run_rollback_with_retry(nats, config, &applied.service, &rollback_reason, ctx).await;
    if rollback_report.status != "success" {
        return fail_after_rollback_command(
            nats,
            config,
            ctx,
            applied,
            post_apply,
            &rollback_report,
            "mechanical rollback failed",
        )
        .await;
    }

    let Some(route) = rollback_route(nats, &applied.service).await? else {
        return fail_after_rollback_command(
            nats,
            config,
            ctx,
            applied,
            post_apply,
            &rollback_report,
            "rolled-back manifest has no service route",
        )
        .await;
    };
    confirm_rollback_health(
        nats,
        config,
        ctx,
        applied,
        post_apply,
        &rollback_report,
        &route,
    )
    .await
}

async fn run_rollback_with_retry(
    nats: &JamNats,
    config: &Config,
    service: &str,
    rollback_reason: &str,
    ctx: &TraceCtx,
) -> CommandReport {
    let deadline = tokio::time::Instant::now() + config.command_timeout;
    loop {
        let request = RollbackRequest {
            service: service.into(),
            reason: rollback_reason.into(),
            requested_by: SERVICE_NAME.into(),
            trace_ctx: ctx.clone(),
        };
        let outcome = patch_ops::perform_rollback(nats, request).await;
        match outcome {
            Ok(updated) => {
                return CommandReport {
                    program: "patch_ops::perform_rollback".into(),
                    args: vec![service.into(), rollback_reason.into()],
                    status: "success".into(),
                    stdout: format!(
                        "manifest_id={} revision={} to_version={}",
                        updated.manifest_id, updated.revision, updated.to_version
                    ),
                    stderr: String::new(),
                };
            }
            Err(err) if rollback_lock_busy(&err) && tokio::time::Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
            Err(err) => {
                return CommandReport {
                    program: "patch_ops::perform_rollback".into(),
                    args: vec![service.into(), rollback_reason.into()],
                    status: "failed".into(),
                    stdout: String::new(),
                    stderr: err,
                };
            }
        }
    }
}

fn rollback_lock_busy(err: &str) -> bool {
    err.contains("patch lock is already held") || err.contains("patch-lock/current")
}

async fn fail_after_rollback_command(
    nats: &JamNats,
    config: &Config,
    ctx: &TraceCtx,
    applied: &PatchApplied,
    post_apply: &CheckReport,
    rollback: &CommandReport,
    summary: &str,
) -> Result<ProcessOutcome, AgentError> {
    let mut llm = run_llm_diagnosis(config, applied, post_apply, None, Some(rollback)).await?;
    if let Some(outcome) = run_llm_recovery_action(nats, config, ctx, applied, &llm).await? {
        let recovered = outcome.recovered;
        llm.recovery = Some(outcome.report);
        if recovered {
            return Ok(ProcessOutcome::Continue);
        }
    }
    fail_patch(
        nats,
        config,
        ctx,
        FailureRecord {
            applied,
            post_apply,
            post_rollback: None,
            rollback: Some(rollback),
            llm: &llm,
            summary,
        },
    )
    .await
}

async fn rollback_route(
    nats: &JamNats,
    service: &str,
) -> Result<Option<jam_nats::RoutingService>, AgentError> {
    let Some(current) = jam_nats::load_current_routing_manifest(nats.jetstream()).await? else {
        return Ok(None);
    };
    Ok(current.manifest.services.get(service).cloned())
}

async fn confirm_rollback_health(
    nats: &JamNats,
    config: &Config,
    ctx: &TraceCtx,
    applied: &PatchApplied,
    post_apply: &CheckReport,
    rollback_report: &CommandReport,
    route: &jam_nats::RoutingService,
) -> Result<ProcessOutcome, AgentError> {
    if let Err(err) = ensure_route_process_running(nats, config, &applied.service, route, ctx).await
    {
        warn!(service = %applied.service, "rollback route relaunch failed: {err}");
    }
    let post_rollback = run_health_checks(
        nats,
        config,
        "post-rollback",
        &applied.service,
        &route.current_version,
        &route.subject_prefix,
        ctx,
    )
    .await;
    if post_rollback.passed {
        publish_event(
            nats,
            PatchRolledBackSuccessfully {
                service: applied.service.clone(),
                version: route.current_version.clone(),
                ts: Utc::now(),
            },
            ctx,
        )
        .await?;
        publish_notify_human(
            nats,
            ctx,
            "low",
            "Patch rolled back successfully",
            serde_json::json!({
                "service": applied.service,
                "attempted_version": applied.to_version,
                "restored_version": route.current_version,
            }),
        )
        .await?;
        return Ok(ProcessOutcome::Continue);
    }

    let mut llm = run_llm_diagnosis(
        config,
        applied,
        post_apply,
        Some(&post_rollback),
        Some(rollback_report),
    )
    .await?;
    if let Some(outcome) = run_llm_recovery_action(nats, config, ctx, applied, &llm).await? {
        let recovered = outcome.recovered;
        llm.recovery = Some(outcome.report);
        if recovered {
            return Ok(ProcessOutcome::Continue);
        }
    }
    fail_patch(
        nats,
        config,
        ctx,
        FailureRecord {
            applied,
            post_apply,
            post_rollback: Some(&post_rollback),
            rollback: Some(rollback_report),
            llm: &llm,
            summary: "mechanical rollback did not restore health",
        },
    )
    .await
}

fn trace_ctx_from_envelope<P>(envelope: &EventEnvelope<P>) -> Result<TraceCtx, AgentError> {
    let trace_id = TraceId::from_str(&envelope.trace_id)?;
    let parent_trace_id = envelope
        .parent_trace_id
        .as_deref()
        .map(TraceId::from_str)
        .transpose()?;
    Ok(TraceCtx {
        trace_id,
        parent_trace_id,
        origin_kind: "patch.applied",
        origin_summary: format!("Patch agent handling {}", envelope.event_type),
    })
}

async fn run_health_checks(
    nats: &JamNats,
    config: &Config,
    stage: &str,
    service: &str,
    version: &str,
    subject_prefix: &str,
    ctx: &TraceCtx,
) -> CheckReport {
    let mut checks = Vec::new();
    checks.push(check_ping(nats, config, service, subject_prefix, ctx).await);
    checks.push(check_safe_smoke(nats, config, service, subject_prefix, ctx).await);
    checks.push(check_jam_doctor(config).await);
    checks.push(check_failed_events(nats, config, service).await);
    let passed = checks
        .iter()
        .all(|check| check.status != CheckStatus::Failed);
    CheckReport {
        stage: stage.into(),
        service: service.into(),
        version: version.into(),
        subject_prefix: subject_prefix.into(),
        passed,
        checks,
    }
}

async fn check_ping(
    nats: &JamNats,
    config: &Config,
    service: &str,
    subject_prefix: &str,
    ctx: &TraceCtx,
) -> CheckResult {
    let subject = format!("{subject_prefix}.ping");
    timed_check("ping", async {
        let response: Value = nats
            .request_traced(
                &subject,
                &serde_json::json!({}),
                ctx,
                config.request_timeout,
            )
            .await
            .map_err(|err| format!("request {subject}: {err}"))?;
        if response.get("error").is_some() {
            return Err(format!("ping returned error envelope: {response}"));
        }
        let status = response
            .get("status")
            .and_then(Value::as_str)
            .ok_or_else(|| "ping response missing string status".to_owned())?;
        if status != "ok" {
            return Err(format!("ping status was {status:?}, expected ok"));
        }
        let actual_service = response
            .get("service")
            .and_then(Value::as_str)
            .ok_or_else(|| "ping response missing string service".to_owned())?;
        let expected_service = format!("jam-svc-{service}");
        if actual_service != expected_service {
            return Err(format!(
                "ping came from {actual_service}, expected {expected_service}"
            ));
        }
        Ok(format!("{} responded ok", response_summary(&response)))
    })
    .await
}

async fn check_safe_smoke(
    nats: &JamNats,
    config: &Config,
    service: &str,
    subject_prefix: &str,
    ctx: &TraceCtx,
) -> CheckResult {
    match service {
        "observe" => {
            let subject = format!("{subject_prefix}.list-blockers");
            timed_check("smoke-list-blockers", async {
                let response: Value = nats
                    .request_traced(
                        &subject,
                        &serde_json::json!({"task_id":"patch-agent-smoke"}),
                        ctx,
                        config.request_timeout,
                    )
                    .await
                    .map_err(|err| format!("request {subject}: {err}"))?;
                if response.get("error").is_some() {
                    return Err(format!("list-blockers returned error envelope: {response}"));
                }
                if !response.is_array() {
                    return Err(format!(
                        "list-blockers response must be an array, got {response}"
                    ));
                }
                Ok("list-blockers returned an array".into())
            })
            .await
        }
        _ => CheckResult::skipped(
            "smoke-known-safe-method",
            "no known-safe smoke method is registered for this service yet",
        ),
    }
}

async fn check_jam_doctor(config: &Config) -> CheckResult {
    if !config.doctor_enabled {
        return CheckResult::skipped("jam-doctor", "disabled by JAM_PATCH_AGENT_SKIP_DOCTOR");
    }
    timed_check("jam-doctor", async {
        let report = run_jam_command(config, &["doctor"])
            .await
            .map_err(|err| err.to_string())?;
        if report.status == "success" {
            Ok("jam doctor passed".into())
        } else {
            Err(format!(
                "jam doctor failed; stdout: {}; stderr: {}",
                report.stdout, report.stderr
            ))
        }
    })
    .await
}

async fn check_failed_events(nats: &JamNats, config: &Config, service: &str) -> CheckResult {
    if !config.failed_event_check_enabled {
        return CheckResult::skipped(
            "recent-failed-events",
            "disabled by JAM_PATCH_AGENT_SKIP_FAILED_EVENT_CHECK",
        );
    }
    timed_check("recent-failed-events", async {
        let failures = recent_patch_failures(nats, service).await?;
        if failures.is_empty() {
            Ok("no patch.failed events for this service in the past 60s".into())
        } else {
            Err(format!(
                "recent patch.failed events for {service}: {}",
                failures.join("; ")
            ))
        }
    })
    .await
}

async fn recent_patch_failures(nats: &JamNats, service: &str) -> Result<Vec<String>, String> {
    use jam_nats::async_nats::jetstream::consumer::pull;
    use jam_nats::async_nats::jetstream::consumer::{AckPolicy, DeliverPolicy};

    let stream = nats
        .jetstream()
        .get_stream("patch")
        .await
        .map_err(|err| format!("open patch stream: {err}"))?;
    let start_time = time::OffsetDateTime::now_utc() - time::Duration::seconds(60);
    let consumer = stream
        .create_consumer(pull::Config {
            durable_name: None,
            name: Some(format!("patch-agent-failed-{}", TraceId::new())),
            description: Some("Patch-agent recent patch.failed scan".into()),
            filter_subject: PatchFailed::EVENT_TYPE.into(),
            deliver_policy: DeliverPolicy::ByStartTime { start_time },
            ack_policy: AckPolicy::Explicit,
            ..Default::default()
        })
        .await
        .map_err(|err| format!("create patch.failed scan consumer: {err}"))?;
    let mut messages = consumer
        .batch()
        .max_messages(100)
        .expires(Duration::from_millis(250))
        .messages()
        .await
        .map_err(|err| format!("read patch.failed events: {err}"))?;
    let mut failures = Vec::new();
    while let Some(message) = messages.next().await {
        let message = message.map_err(|err| format!("read patch.failed event: {err}"))?;
        let envelope: EventEnvelope<PatchFailed> =
            serde_json::from_slice(&message.payload).map_err(|err| err.to_string())?;
        message
            .ack()
            .await
            .map_err(|err| format!("ack patch.failed event: {err}"))?;
        if envelope.payload.service == service {
            failures.push(format!(
                "{} {}",
                envelope.payload.incident_id, envelope.payload.summary
            ));
        }
    }
    Ok(failures)
}

async fn timed_check<F>(name: &str, future: F) -> CheckResult
where
    F: std::future::Future<Output = Result<String, String>>,
{
    let started_at = Utc::now();
    let outcome = future.await;
    let finished_at = Utc::now();
    match outcome {
        Ok(detail) => CheckResult {
            name: name.into(),
            status: CheckStatus::Passed,
            detail,
            started_at: format_ts(started_at),
            finished_at: format_ts(finished_at),
        },
        Err(detail) => CheckResult {
            name: name.into(),
            status: CheckStatus::Failed,
            detail,
            started_at: format_ts(started_at),
            finished_at: format_ts(finished_at),
        },
    }
}

impl CheckResult {
    fn skipped(name: &str, detail: &str) -> Self {
        let now = format_ts(Utc::now());
        Self {
            name: name.into(),
            status: CheckStatus::Skipped,
            detail: detail.into(),
            started_at: now.clone(),
            finished_at: now,
        }
    }
}

async fn ensure_route_process_running(
    nats: &JamNats,
    config: &Config,
    service: &str,
    route: &jam_nats::RoutingService,
    ctx: &TraceCtx,
) -> Result<(), AgentError> {
    let ping = check_ping(nats, config, service, &route.subject_prefix, ctx).await;
    if ping.status == CheckStatus::Passed {
        return Ok(());
    }

    warn!(
        service,
        subject_prefix = %route.subject_prefix,
        binary = %route.binary_path.display(),
        reason = %ping.detail,
        "rolled-back route is not responding; relaunching service binary",
    );
    tokio::time::sleep(Duration::from_millis(500)).await;
    start_manifest_route_process(config, service, route)?;

    let deadline = tokio::time::Instant::now() + config.command_timeout;
    loop {
        if tokio::time::Instant::now() >= deadline {
            return Err(AgentError::Command(format!(
                "relaunched {} but {}.ping did not become healthy within {}s",
                route.binary_path.display(),
                route.subject_prefix,
                config.command_timeout.as_secs()
            )));
        }
        let ping = check_ping(nats, config, service, &route.subject_prefix, ctx).await;
        if ping.status == CheckStatus::Passed {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

fn start_manifest_route_process(
    config: &Config,
    service: &str,
    route: &jam_nats::RoutingService,
) -> Result<(), AgentError> {
    if !route.binary_path.is_file() {
        return Err(AgentError::Command(format!(
            "rollback binary is missing: {}",
            route.binary_path.display()
        )));
    }
    let log_dir = config.jam_home.join("logs").join("patch-agent");
    fs::create_dir_all(&log_dir)?;
    let stdout_path = log_dir.join(format!(
        "jam-svc-{service}-{}-restart.stdout.log",
        route.current_version
    ));
    let stderr_path = log_dir.join(format!(
        "jam-svc-{service}-{}-restart.stderr.log",
        route.current_version
    ));
    let stdout = File::create(&stdout_path)?;
    let stderr = File::create(&stderr_path)?;
    let mut command = StdCommand::new(&route.binary_path);
    command
        .env("NATS_URL", &config.nats_url)
        .env("JAM_HOME", &config.jam_home)
        .env("JAM_TOOL_SUBJECT_PREFIX", &route.subject_prefix)
        .env(service_subject_prefix_env(service), &route.subject_prefix)
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    if let Some(token) = &config.nats_token {
        command.env("NATS_TOKEN", token);
    }
    set_process_group(&mut command);
    command.spawn().map_err(|err| {
        AgentError::Command(format!(
            "start {} with prefix {}: {err}",
            route.binary_path.display(),
            route.subject_prefix
        ))
    })?;
    Ok(())
}

#[cfg(unix)]
fn set_process_group(command: &mut StdCommand) {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
}

#[cfg(not(unix))]
fn set_process_group(_command: &mut StdCommand) {}

async fn run_jam_command(config: &Config, args: &[&str]) -> Result<CommandReport, AgentError> {
    let mut command = Command::new(&config.jam_bin);
    command.args(args).env("JAM_HOME", &config.jam_home);
    if let Some(token) = &config.nats_token {
        command.env("NATS_TOKEN", token);
    }
    let output = tokio::time::timeout(config.command_timeout, command.output())
        .await
        .map_err(|_| {
            AgentError::Command(format!(
                "{} {} timed out after {}s",
                config.jam_bin.display(),
                args.join(" "),
                config.command_timeout.as_secs()
            ))
        })?
        .map_err(|err| {
            AgentError::Command(format!("execute {}: {err}", config.jam_bin.display()))
        })?;
    Ok(CommandReport {
        program: config.jam_bin.display().to_string(),
        args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        status: if output.status.success() {
            "success".into()
        } else {
            format!("exit-{}", output.status.code().unwrap_or(-1))
        },
        stdout: truncate_utf8(&output.stdout, MAX_OUTPUT_BYTES),
        stderr: truncate_utf8(&output.stderr, MAX_OUTPUT_BYTES),
    })
}

async fn run_llm_diagnosis(
    config: &Config,
    applied: &PatchApplied,
    post_apply: &CheckReport,
    post_rollback: Option<&CheckReport>,
    rollback_report: Option<&CommandReport>,
) -> Result<LlmReport, AgentError> {
    let recent_journal_events = recent_journal_events(config)?;
    let prompt = serde_json::json!({
        "instruction": "Diagnose the failed Jamboree hot-patch. Reply with one action from [restart-service, rollback-to-version, ntfy-with-incident-dump] and a short reason.",
        "budget_cap_usd": config.llm_budget_usd,
        "patch": applied,
        "post_apply": post_apply,
        "post_rollback": post_rollback,
        "rollback": rollback_report,
        "recent_journal_events": recent_journal_events,
    });
    let Some(command_parts) = &config.llm_command else {
        return Ok(LlmReport {
            attempted: false,
            budget_cap_usd: config.llm_budget_usd,
            status: "not-configured".into(),
            suggestion: None,
            stdout: String::new(),
            stderr: "JAM_PATCH_AGENT_LLM_CMD is not set".into(),
            recovery: None,
        });
    };
    let Some((program, args)) = command_parts.split_first() else {
        return Ok(LlmReport {
            attempted: false,
            budget_cap_usd: config.llm_budget_usd,
            status: "not-configured".into(),
            suggestion: None,
            stdout: String::new(),
            stderr: "JAM_PATCH_AGENT_LLM_CMD was empty".into(),
            recovery: None,
        });
    };

    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| AgentError::Command(format!("start LLM command {program}: {err}")))?;
    if let Some(mut stdin) = child.stdin.take() {
        let bytes = serde_json::to_vec_pretty(&prompt)?;
        tokio::spawn(async move {
            if let Err(err) = tokio::io::AsyncWriteExt::write_all(&mut stdin, &bytes).await {
                warn!("write LLM stdin failed: {err}");
            }
        });
    }
    let output = tokio::time::timeout(config.llm_timeout, child.wait_with_output())
        .await
        .map_err(|_| {
            AgentError::Command(format!(
                "LLM command {program} timed out after {}s",
                config.llm_timeout.as_secs()
            ))
        })?
        .map_err(|err| AgentError::Command(format!("run LLM command {program}: {err}")))?;
    let stdout = truncate_utf8(&output.stdout, MAX_OUTPUT_BYTES);
    let suggestion = parse_llm_suggestion(&stdout);
    Ok(LlmReport {
        attempted: true,
        budget_cap_usd: config.llm_budget_usd,
        status: if output.status.success() {
            "success".into()
        } else {
            format!("exit-{}", output.status.code().unwrap_or(-1))
        },
        suggestion,
        stdout,
        stderr: truncate_utf8(&output.stderr, MAX_OUTPUT_BYTES),
        recovery: None,
    })
}

async fn run_llm_recovery_action(
    nats: &JamNats,
    config: &Config,
    ctx: &TraceCtx,
    applied: &PatchApplied,
    llm: &LlmReport,
) -> Result<Option<LlmRecoveryOutcome>, AgentError> {
    if !llm.attempted || llm.status != "success" {
        return Ok(None);
    }
    let Some(action) = llm.suggestion.as_deref() else {
        return Ok(None);
    };
    match action {
        "restart-service" => restart_current_route(nats, config, ctx, applied, action)
            .await
            .map(Some),
        "rollback-to-version" => retry_rollback_from_llm(nats, config, ctx, applied, action)
            .await
            .map(Some),
        "ntfy-with-incident-dump" => Ok(Some(LlmRecoveryOutcome {
            recovered: false,
            report: LlmRecoveryReport {
                action: action.into(),
                status: "deferred-to-incident".into(),
                detail: "LLM requested incident escalation instead of automated recovery".into(),
                command: None,
                health: None,
            },
        })),
        _ => Ok(None),
    }
}

async fn restart_current_route(
    nats: &JamNats,
    config: &Config,
    ctx: &TraceCtx,
    applied: &PatchApplied,
    action: &str,
) -> Result<LlmRecoveryOutcome, AgentError> {
    let Some(route) = rollback_route(nats, &applied.service).await? else {
        return Ok(LlmRecoveryOutcome {
            recovered: false,
            report: LlmRecoveryReport {
                action: action.into(),
                status: "failed".into(),
                detail: "current routing manifest has no service route".into(),
                command: None,
                health: None,
            },
        });
    };
    if let Err(err) =
        ensure_route_process_running(nats, config, &applied.service, &route, ctx).await
    {
        return Ok(LlmRecoveryOutcome {
            recovered: false,
            report: LlmRecoveryReport {
                action: action.into(),
                status: "failed".into(),
                detail: err.to_string(),
                command: None,
                health: None,
            },
        });
    }
    let health = run_health_checks(
        nats,
        config,
        "post-llm-restart",
        &applied.service,
        &route.current_version,
        &route.subject_prefix,
        ctx,
    )
    .await;
    let recovered = health.passed;
    if recovered {
        publish_llm_recovered(nats, applied, &route, health.checks_run(), ctx).await?;
    }
    Ok(LlmRecoveryOutcome {
        recovered,
        report: LlmRecoveryReport {
            action: action.into(),
            status: if recovered { "recovered" } else { "failed" }.into(),
            detail: if recovered {
                "restart-service restored health"
            } else {
                "restart-service did not restore health"
            }
            .into(),
            command: None,
            health: Some(health),
        },
    })
}

async fn retry_rollback_from_llm(
    nats: &JamNats,
    config: &Config,
    ctx: &TraceCtx,
    applied: &PatchApplied,
    action: &str,
) -> Result<LlmRecoveryOutcome, AgentError> {
    let reason = format!(
        "patch-agent LLM diagnosis suggested rollback-to-version for {} {}",
        applied.service, applied.to_version
    );
    let command = run_rollback_with_retry(nats, config, &applied.service, &reason, ctx).await;
    if command.status != "success" {
        return Ok(LlmRecoveryOutcome {
            recovered: false,
            report: LlmRecoveryReport {
                action: action.into(),
                status: "failed".into(),
                detail: "rollback-to-version command failed".into(),
                command: Some(command),
                health: None,
            },
        });
    }
    let Some(route) = rollback_route(nats, &applied.service).await? else {
        return Ok(LlmRecoveryOutcome {
            recovered: false,
            report: LlmRecoveryReport {
                action: action.into(),
                status: "failed".into(),
                detail: "rollback command succeeded but manifest has no service route".into(),
                command: Some(command),
                health: None,
            },
        });
    };
    if let Err(err) =
        ensure_route_process_running(nats, config, &applied.service, &route, ctx).await
    {
        return Ok(LlmRecoveryOutcome {
            recovered: false,
            report: LlmRecoveryReport {
                action: action.into(),
                status: "failed".into(),
                detail: err.to_string(),
                command: Some(command),
                health: None,
            },
        });
    }
    let health = run_health_checks(
        nats,
        config,
        "post-llm-rollback",
        &applied.service,
        &route.current_version,
        &route.subject_prefix,
        ctx,
    )
    .await;
    let recovered = health.passed;
    if recovered {
        publish_llm_recovered(nats, applied, &route, health.checks_run(), ctx).await?;
    }
    Ok(LlmRecoveryOutcome {
        recovered,
        report: LlmRecoveryReport {
            action: action.into(),
            status: if recovered { "recovered" } else { "failed" }.into(),
            detail: if recovered {
                "rollback-to-version restored health"
            } else {
                "rollback-to-version did not restore health"
            }
            .into(),
            command: Some(command),
            health: Some(health),
        },
    })
}

async fn publish_llm_recovered(
    nats: &JamNats,
    applied: &PatchApplied,
    route: &jam_nats::RoutingService,
    checks_run: u32,
    ctx: &TraceCtx,
) -> Result<(), AgentError> {
    if route.current_version == applied.to_version {
        publish_event(
            nats,
            PatchConfirmed {
                service: applied.service.clone(),
                version: route.current_version.clone(),
                checks_run,
                ts: Utc::now(),
            },
            ctx,
        )
        .await?;
    } else {
        publish_event(
            nats,
            PatchRolledBackSuccessfully {
                service: applied.service.clone(),
                version: route.current_version.clone(),
                ts: Utc::now(),
            },
            ctx,
        )
        .await?;
    }
    publish_notify_human(
        nats,
        ctx,
        "low",
        "Patch recovered after LLM diagnosis",
        serde_json::json!({
            "service": applied.service,
            "attempted_version": applied.to_version,
            "healthy_version": route.current_version,
        }),
    )
    .await
}

async fn fail_patch(
    nats: &JamNats,
    config: &Config,
    ctx: &TraceCtx,
    failure: FailureRecord<'_>,
) -> Result<ProcessOutcome, AgentError> {
    let (incident_id, incident_dir) = write_incident_dump(
        config,
        failure.applied,
        failure.post_apply,
        failure.post_rollback,
        failure.rollback,
        failure.llm,
        failure.summary,
    )?;
    let incident_dir_string = incident_dir.display().to_string();
    publish_event(
        nats,
        PatchFailed {
            service: failure.applied.service.clone(),
            incident_id: incident_id.clone(),
            summary: failure.summary.into(),
            incident_dir: incident_dir_string.clone(),
            ts: Utc::now(),
        },
        ctx,
    )
    .await?;
    publish_notify_human(
        nats,
        ctx,
        "critical",
        failure.summary,
        serde_json::to_value(NotifyHumanDetail {
            service: &failure.applied.service,
            incident_id: &incident_id,
            incident_dir: &incident_dir_string,
        })?,
    )
    .await?;
    pause_dispatch(nats, failure.summary).await?;
    Ok(ProcessOutcome::Fatal)
}

fn write_incident_dump(
    config: &Config,
    applied: &PatchApplied,
    post_apply: &CheckReport,
    post_rollback: Option<&CheckReport>,
    rollback: Option<&CommandReport>,
    llm: &LlmReport,
    summary: &str,
) -> Result<(String, PathBuf), AgentError> {
    let incident_id = format!("incident-{}", TraceId::new());
    let incident_dir = config.jam_home.join("incidents").join(&incident_id);
    fs::create_dir_all(&incident_dir)?;
    write_json(
        &incident_dir.join("summary.json"),
        &IncidentSummary {
            incident_id: &incident_id,
            service: &applied.service,
            attempted_version: &applied.to_version,
            subject_prefix: &applied.subject_prefix,
            summary,
        },
    )?;
    write_json(&incident_dir.join("patch-applied.json"), applied)?;
    write_json(&incident_dir.join("health-post-apply.json"), post_apply)?;
    if let Some(report) = post_rollback {
        write_json(&incident_dir.join("health-post-rollback.json"), report)?;
    }
    if let Some(report) = rollback {
        write_json(&incident_dir.join("rollback-command.json"), report)?;
    }
    write_json(&incident_dir.join("llm-diagnosis.json"), llm)?;
    write_recent_journal_events(config, &incident_dir)?;
    Ok((incident_id, incident_dir))
}

fn write_recent_journal_events(config: &Config, incident_dir: &Path) -> Result<(), AgentError> {
    let events = recent_journal_events(config)?;
    let mut file = File::create(incident_dir.join("last-1000-journal-events.jsonl"))?;
    for line in &events {
        writeln!(file, "{line}")?;
    }
    Ok(())
}

fn recent_journal_events(config: &Config) -> Result<Vec<String>, AgentError> {
    let mut events = Vec::new();
    let journal_root = config.jam_home.join("journal");
    if journal_root.is_dir() {
        collect_jsonl_lines(&journal_root, &mut events)?;
    }
    let keep_from = events.len().saturating_sub(1_000);
    Ok(events[keep_from..].to_vec())
}

fn collect_jsonl_lines(path: &Path, events: &mut Vec<String>) -> Result<(), AgentError> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_jsonl_lines(&path, events)?;
        } else if path.extension().is_some_and(|ext| ext == "jsonl") {
            let contents = fs::read_to_string(&path)?;
            events.extend(contents.lines().map(ToOwned::to_owned));
        }
    }
    Ok(())
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), AgentError> {
    let bytes = serde_json::to_vec_pretty(value)?;
    fs::write(path, bytes)?;
    Ok(())
}

async fn publish_event<P>(nats: &JamNats, payload: P, ctx: &TraceCtx) -> Result<(), AgentError>
where
    P: Event,
{
    let envelope = EventEnvelope::new(
        P::EVENT_TYPE,
        P::EVENT_SUBTYPE_VERSION,
        0,
        ctx.trace_id.to_string(),
        SERVICE_NAME,
        payload,
    );
    let envelope = if let Some(parent) = ctx.parent_trace_id {
        envelope.with_parent_trace(parent.to_string())
    } else {
        envelope
    };
    nats.publish_traced(P::EVENT_TYPE, &envelope, ctx).await?;
    Ok(())
}

async fn publish_notify_human(
    nats: &JamNats,
    ctx: &TraceCtx,
    urgency: &str,
    summary: &str,
    payload: Value,
) -> Result<(), AgentError> {
    let body = serde_json::json!({
        "urgency": urgency,
        "summary": summary,
        "payload": payload,
    });
    nats.publish_traced("notify.human", &body, ctx).await?;
    Ok(())
}

async fn pause_dispatch(nats: &JamNats, reason: &str) -> Result<(), AgentError> {
    let record = DispatchPauseRecord {
        dispatch_paused: true,
        reason: Some(reason.to_owned()),
        changed_at: Utc::now(),
        changed_by: SERVICE_NAME.into(),
    };
    let kv = nats
        .jetstream()
        .get_key_value(DISPATCH_STATE_BUCKET)
        .await
        .map_err(|err| {
            AgentError::Command(format!("open {DISPATCH_STATE_BUCKET} KV bucket: {err}"))
        })?;
    kv.put(DISPATCH_PAUSED_KEY, "true".into())
        .await
        .map_err(|err| AgentError::Command(format!("write {DISPATCH_PAUSED_KEY}: {err}")))?;
    kv.put(DISPATCH_STATE_KEY, serde_json::to_vec(&record)?.into())
        .await
        .map_err(|err| AgentError::Command(format!("write {DISPATCH_STATE_KEY}: {err}")))?;
    Ok(())
}

fn service_subject_prefix_env(service: &str) -> String {
    let token: String = service
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect();
    format!("JAM_{token}_SUBJECT_PREFIX")
}

fn response_summary(value: &Value) -> String {
    value.get("version").and_then(Value::as_str).map_or_else(
        || "version=<missing>".into(),
        |version| format!("version={version}"),
    )
}

fn parse_llm_suggestion(stdout: &str) -> Option<String> {
    [
        "restart-service",
        "rollback-to-version",
        "ntfy-with-incident-dump",
    ]
    .iter()
    .find(|candidate| stdout.contains(**candidate))
    .map(|value| (*value).to_owned())
}

fn split_command(raw: &str) -> Result<Vec<String>, AgentError> {
    let parts: Vec<String> = raw
        .split_whitespace()
        .filter(|part| !part.trim().is_empty())
        .map(ToOwned::to_owned)
        .collect();
    if parts.is_empty() {
        Err(AgentError::Config(
            "JAM_PATCH_AGENT_LLM_CMD must not be empty".into(),
        ))
    } else {
        Ok(parts)
    }
}

fn duration_env(name: &str, default_secs: u64) -> Duration {
    std::env::var(name)
        .ok()
        .and_then(|raw| raw.parse::<u64>().ok())
        .filter(|secs| *secs > 0)
        .map_or(Duration::from_secs(default_secs), Duration::from_secs)
}

fn parse_bool_env(name: &str) -> Option<bool> {
    let raw = std::env::var(name).ok()?;
    match raw.as_str() {
        "1" | "true" | "TRUE" | "yes" | "YES" => Some(true),
        "0" | "false" | "FALSE" | "no" | "NO" => Some(false),
        _ => None,
    }
}

fn format_ts(ts: chrono::DateTime<Utc>) -> String {
    ts.to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn truncate_utf8(bytes: &[u8], max: usize) -> String {
    let text = String::from_utf8_lossy(bytes);
    if text.len() <= max {
        text.into_owned()
    } else {
        format!("{}...<truncated>", &text[..max])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_subject_prefix_env_matches_patch_apply() {
        assert_eq!(
            service_subject_prefix_env("observe"),
            "JAM_OBSERVE_SUBJECT_PREFIX"
        );
        assert_eq!(
            service_subject_prefix_env("review-agent"),
            "JAM_REVIEW_AGENT_SUBJECT_PREFIX"
        );
    }

    #[test]
    fn llm_suggestion_parses_menu_items() {
        assert_eq!(
            parse_llm_suggestion("The right action is restart-service."),
            Some("restart-service".into())
        );
        assert_eq!(parse_llm_suggestion("no menu item here"), None);
    }

    #[test]
    fn check_report_only_fails_on_failed_checks() {
        let report = CheckReport {
            stage: "test".into(),
            service: "observe".into(),
            version: "1".into(),
            subject_prefix: "tool.observe.v1".into(),
            passed: true,
            checks: vec![
                CheckResult::skipped("future", "not wired"),
                CheckResult {
                    name: "ping".into(),
                    status: CheckStatus::Passed,
                    detail: "ok".into(),
                    started_at: "now".into(),
                    finished_at: "now".into(),
                },
            ],
        };
        assert_eq!(report.failed_details(), Vec::<String>::new());
        assert_eq!(report.checks_run(), 1);
    }

    #[test]
    fn split_command_rejects_empty() {
        assert!(split_command("  ").is_err());
        assert_eq!(
            split_command("codex exec").unwrap(),
            vec!["codex".to_owned(), "exec".to_owned()]
        );
    }
}
