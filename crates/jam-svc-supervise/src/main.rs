//! `jam-svc-supervise` - supervision tools for human-facing controls.
//!
//! This first slice exposes `tool.supervise.notify-human` and publishes the
//! traced `notify.human` bus event consumed by push and UI surfaces.

#![deny(missing_docs)]

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use chrono::{DateTime, Utc};
use futures::StreamExt;
use jam_nats::async_nats;
use jam_nats::JamNats;
use jam_trace::TraceCtx;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{debug, error, info, warn};

const SERVICE_NAME: &str = "jam-svc-supervise";
const SERVICE_VERSION: &str = env!("CARGO_PKG_VERSION");
const SUBJECT_PREFIX: &str = "tool.supervise";
const SUBJECT_PREFIX_ENV: &str = "JAM_SUPERVISE_SUBJECT_PREFIX";
const MAX_SUMMARY_LEN: usize = 500;
const DISPATCH_STATE_BUCKET: &str = "dispatch-state";
const DISPATCH_PAUSED_KEY: &str = "dispatch-paused";
const DISPATCH_STATE_KEY: &str = "state";

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
enum SuperviseError {
    #[error("{kind}: {detail}")]
    Protocol {
        kind: &'static str,
        detail: String,
        remediation: &'static str,
        tracked_by: &'static str,
    },
}

impl SuperviseError {
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

#[derive(Debug, Deserialize)]
struct NotifyHumanInput {
    #[serde(default = "default_urgency")]
    urgency: String,
    summary: String,
    #[serde(default)]
    payload: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct PauseDispatchInput {
    reason: String,
    changed_by: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct ResumeDispatchInput {
    changed_by: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct DispatchPauseRecord {
    dispatch_paused: bool,
    reason: Option<String>,
    changed_at: DateTime<Utc>,
    changed_by: String,
}

#[derive(Debug, Serialize)]
struct NotifyHumanEvent {
    urgency: String,
    summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    payload: Option<Value>,
}

#[derive(Debug, Serialize)]
struct NotifyHumanOutput {
    status: &'static str,
    subject: &'static str,
    urgency: String,
    trace_id: String,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum Response {
    Ok(Value),
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
        error!("jam-svc-supervise fatal: {err}");
        return std::process::ExitCode::FAILURE;
    }
    std::process::ExitCode::SUCCESS
}

async fn run() -> Result<(), ServiceError> {
    init_tracing();

    let nats_url = std::env::var("NATS_URL").unwrap_or_else(|_| "nats://127.0.0.1:4222".into());
    let nats_token = std::env::var("NATS_TOKEN").ok();
    let subject_prefix = configured_subject_prefix(SUBJECT_PREFIX_ENV, SUBJECT_PREFIX);

    info!(
        service = %SERVICE_NAME,
        version = %SERVICE_VERSION,
        nats = %nats_url,
        subject_prefix = %subject_prefix,
        "starting",
    );

    let nats = JamNats::connect(&nats_url, nats_token).await?;
    info!("connected to NATS");

    let mut sub = nats
        .client()
        .subscribe(format!("{subject_prefix}.>"))
        .await
        .map_err(|err| ServiceError::Subscribe(err.to_string()))?;
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
                let draining = draining.clone();
                let active_requests = active_requests.clone();
                active_requests.fetch_add(1, Ordering::SeqCst);
                tokio::spawn(async move {
                    let result = handle_request(&nats, &message, &draining).await;
                    active_requests.fetch_sub(1, Ordering::SeqCst);
                    if let Err(err) = result {
                        warn!(subject = %message.subject, "handle: {err}");
                    }
                });
            }
        }
    }
}

fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("jam_svc_supervise=info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

async fn handle_request(
    nats: &JamNats,
    msg: &async_nats::Message,
    draining: &Arc<AtomicBool>,
) -> Result<(), ServiceError> {
    let method = method_from_subject(msg.subject.as_str()).unwrap_or("");
    debug!(subject = %msg.subject, method = %method, "received request");

    let response_ctx = msg
        .headers
        .as_ref()
        .and_then(jam_nats::extract_trace_from_headers);

    let response = match &response_ctx {
        Some(ctx) => dispatch(method, &msg.payload, ctx, nats).await,
        None => Response::Error {
            error: ResponseError {
                kind: "missing-trace".into(),
                detail: "tool.supervise requests must include Trace-Id headers".into(),
                remediation: "Use JamNats::request_traced for tool calls.".into(),
                tracked_by: "principle-tracing-chains-end-to-end",
            },
        },
    };

    let Some(reply_subject) = msg.reply.as_ref() else {
        warn!(subject = %msg.subject, method = %method, "no reply subject");
        return Ok(());
    };

    let payload = serde_json::to_vec(&response).expect("response serializes");
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

async fn dispatch(method: &str, payload: &[u8], ctx: &TraceCtx, nats: &JamNats) -> Response {
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
        "notify-human" => match notify_human(payload, ctx, nats).await {
            Ok(output) => Response::Ok(serde_json::to_value(output).expect("output serializes")),
            Err(err) => error_response(err),
        },
        "pause-dispatch" => match pause_dispatch(payload, ctx, nats).await {
            Ok(record) => Response::Ok(serde_json::to_value(record).expect("record serializes")),
            Err(err) => error_response(err),
        },
        "resume-dispatch" => match resume_dispatch(payload, ctx, nats).await {
            Ok(record) => Response::Ok(serde_json::to_value(record).expect("record serializes")),
            Err(err) => error_response(err),
        },
        unknown => Response::Error {
            error: ResponseError {
                kind: "unknown-method".into(),
                detail: format!("{SUBJECT_PREFIX}.{unknown} is not a recognized supervise method"),
                remediation: "Use tool.supervise.notify-human.".into(),
                tracked_by: "api-notify-human",
            },
        },
    }
}

async fn notify_human(
    payload: &[u8],
    ctx: &TraceCtx,
    nats: &JamNats,
) -> Result<NotifyHumanOutput, SuperviseError> {
    let input = parse_notify_human_input(payload)?;
    let event = validate_notify_human(input)?;
    let urgency = event.urgency.clone();
    nats.publish_traced("notify.human", &event, ctx)
        .await
        .map_err(|err| {
            SuperviseError::protocol(
                "notify-publish-failed",
                err.to_string(),
                "Verify NATS is running and the notify stream exists.",
                "principle-failure-surfaces-immediately",
            )
        })?;

    Ok(NotifyHumanOutput {
        status: "published",
        subject: "notify.human",
        urgency,
        trace_id: ctx.trace_id.to_string(),
    })
}

fn parse_notify_human_input(payload: &[u8]) -> Result<NotifyHumanInput, SuperviseError> {
    serde_json::from_slice(payload).map_err(|err| {
        SuperviseError::protocol(
            "invalid-input",
            format!("tool.supervise.notify-human payload is invalid JSON: {err}"),
            "Send {\"urgency\":\"high\",\"summary\":\"...\"}.",
            "api-notify-human",
        )
    })
}

fn validate_notify_human(input: NotifyHumanInput) -> Result<NotifyHumanEvent, SuperviseError> {
    let urgency = normalize_urgency(&input.urgency)?;
    let summary = input.summary.trim();
    if summary.is_empty() {
        return Err(SuperviseError::protocol(
            "invalid-summary",
            "summary must not be empty",
            "Send a short explanation of what the Manager needs to do.",
            "api-notify-human",
        ));
    }
    if summary.len() > MAX_SUMMARY_LEN {
        return Err(SuperviseError::protocol(
            "invalid-summary",
            format!("summary must be at most {MAX_SUMMARY_LEN} bytes"),
            "Move details into the optional payload object.",
            "api-notify-human",
        ));
    }
    if summary.contains('\0') {
        return Err(SuperviseError::protocol(
            "invalid-summary",
            "summary may not contain NUL",
            "Remove control characters before notifying the Manager.",
            "api-notify-human",
        ));
    }
    Ok(NotifyHumanEvent {
        urgency,
        summary: summary.to_owned(),
        payload: input.payload,
    })
}

fn normalize_urgency(raw: &str) -> Result<String, SuperviseError> {
    let urgency = raw.trim().to_ascii_lowercase();
    match urgency.as_str() {
        "low" | "medium" | "high" | "critical" => Ok(urgency),
        _ => Err(SuperviseError::protocol(
            "invalid-urgency",
            format!("urgency {raw:?} is not one of low, medium, high, critical"),
            "Use one of the api-notify-human urgency levels.",
            "api-notify-human",
        )),
    }
}

fn default_urgency() -> String {
    "medium".into()
}

async fn pause_dispatch(
    payload: &[u8],
    ctx: &TraceCtx,
    nats: &JamNats,
) -> Result<DispatchPauseRecord, SuperviseError> {
    let input = parse_pause_dispatch_input(payload)?;
    let reason = validate_reason(&input.reason)?;
    let changed_by = validated_changed_by(input.changed_by, ctx)?;
    set_dispatch_pause_state(nats, true, Some(reason), changed_by).await
}

async fn resume_dispatch(
    payload: &[u8],
    ctx: &TraceCtx,
    nats: &JamNats,
) -> Result<DispatchPauseRecord, SuperviseError> {
    let input = parse_resume_dispatch_input(payload)?;
    let changed_by = validated_changed_by(input.changed_by, ctx)?;
    set_dispatch_pause_state(nats, false, None, changed_by).await
}

fn parse_pause_dispatch_input(payload: &[u8]) -> Result<PauseDispatchInput, SuperviseError> {
    serde_json::from_slice(payload).map_err(|err| {
        SuperviseError::protocol(
            "invalid-input",
            format!("tool.supervise.pause-dispatch payload is invalid JSON: {err}"),
            "Send {\"reason\":\"...\"}.",
            "api-pause-dispatch",
        )
    })
}

fn parse_resume_dispatch_input(payload: &[u8]) -> Result<ResumeDispatchInput, SuperviseError> {
    if payload.is_empty() {
        return Ok(ResumeDispatchInput::default());
    }
    serde_json::from_slice(payload).map_err(|err| {
        SuperviseError::protocol(
            "invalid-input",
            format!("tool.supervise.resume-dispatch payload is invalid JSON: {err}"),
            "Send {} or {\"changed_by\":\"human:<user-id>\"}.",
            "api-pause-dispatch",
        )
    })
}

fn validate_reason(reason: &str) -> Result<String, SuperviseError> {
    let reason = reason.trim();
    if reason.is_empty() {
        return Err(SuperviseError::protocol(
            "invalid-reason",
            "pause-dispatch reason must not be empty",
            "Explain why new Picker dispatch should stop.",
            "api-pause-dispatch",
        ));
    }
    if reason.len() > MAX_SUMMARY_LEN {
        return Err(SuperviseError::protocol(
            "invalid-reason",
            format!("pause-dispatch reason must be at most {MAX_SUMMARY_LEN} bytes"),
            "Move details into the associated notify-human payload.",
            "api-pause-dispatch",
        ));
    }
    if reason.contains('\0') {
        return Err(SuperviseError::protocol(
            "invalid-reason",
            "pause-dispatch reason may not contain NUL",
            "Remove control characters before pausing dispatch.",
            "api-pause-dispatch",
        ));
    }
    Ok(reason.to_owned())
}

fn validated_changed_by(
    changed_by: Option<String>,
    ctx: &TraceCtx,
) -> Result<String, SuperviseError> {
    let changed_by = changed_by.unwrap_or_else(|| format!("maestro:{}", ctx.trace_id));
    let changed_by = changed_by.trim();
    if changed_by.is_empty() {
        return Err(SuperviseError::protocol(
            "invalid-changed-by",
            "changed_by must not be empty",
            "Use human:<user-id> or maestro:<session-id> attribution.",
            "api-pause-dispatch",
        ));
    }
    if changed_by.len() > 200 || changed_by.contains('\0') {
        return Err(SuperviseError::protocol(
            "invalid-changed-by",
            "changed_by is too long or contains NUL",
            "Use a short actor id such as human:caleb.",
            "api-pause-dispatch",
        ));
    }
    Ok(changed_by.to_owned())
}

async fn set_dispatch_pause_state(
    nats: &JamNats,
    dispatch_paused: bool,
    reason: Option<String>,
    changed_by: String,
) -> Result<DispatchPauseRecord, SuperviseError> {
    jam_nats::ensure_kv_buckets(nats.jetstream(), &jam_nats::default_kv_buckets())
        .await
        .map_err(|err| {
            SuperviseError::protocol(
                "dispatch-state-kv-unavailable",
                err.to_string(),
                "Verify NATS JetStream is enabled and setup has created KV buckets.",
                "principle-failure-surfaces-immediately",
            )
        })?;
    let record = DispatchPauseRecord {
        dispatch_paused,
        reason,
        changed_at: Utc::now(),
        changed_by,
    };
    write_dispatch_pause_record(nats, &record).await?;
    Ok(record)
}

async fn write_dispatch_pause_record(
    nats: &JamNats,
    record: &DispatchPauseRecord,
) -> Result<(), SuperviseError> {
    let kv = nats
        .jetstream()
        .get_key_value(DISPATCH_STATE_BUCKET)
        .await
        .map_err(|err| {
            SuperviseError::protocol(
                "dispatch-state-kv-unavailable",
                format!("open {DISPATCH_STATE_BUCKET} KV bucket: {err}"),
                "Run jam setup or ensure jam-nats bootstrap created the dispatch-state KV bucket.",
                "api-pause-dispatch",
            )
        })?;
    let paused = if record.dispatch_paused {
        "true"
    } else {
        "false"
    };
    kv.put(DISPATCH_PAUSED_KEY, paused.into())
        .await
        .map_err(|err| {
            SuperviseError::protocol(
                "dispatch-state-write-failed",
                format!("write {DISPATCH_PAUSED_KEY}: {err}"),
                "Verify NATS JetStream storage is writable.",
                "principle-failure-surfaces-immediately",
            )
        })?;
    let state = serde_json::to_vec(record).map_err(|err| {
        SuperviseError::protocol(
            "dispatch-state-serialize-failed",
            err.to_string(),
            "Update the dispatch state record schema or serializer.",
            "api-pause-dispatch",
        )
    })?;
    kv.put(DISPATCH_STATE_KEY, state.into())
        .await
        .map_err(|err| {
            SuperviseError::protocol(
                "dispatch-state-write-failed",
                format!("write {DISPATCH_STATE_KEY}: {err}"),
                "Verify NATS JetStream storage is writable.",
                "principle-failure-surfaces-immediately",
            )
        })?;
    Ok(())
}

fn error_response(err: SuperviseError) -> Response {
    match err {
        SuperviseError::Protocol {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_notify_human_input() {
        let event = validate_notify_human(NotifyHumanInput {
            urgency: "HIGH".into(),
            summary: "Investigate failed write".into(),
            payload: Some(serde_json::json!({ "write_id": "write-1" })),
        })
        .unwrap();

        assert_eq!(event.urgency, "high");
        assert_eq!(event.summary, "Investigate failed write");
    }

    #[test]
    fn rejects_invalid_urgency() {
        let err = normalize_urgency("later").unwrap_err();
        assert!(err.to_string().contains("invalid-urgency"));
    }

    #[test]
    fn validates_pause_reason() {
        assert_eq!(
            validate_reason("  all quota exhausted  ").unwrap(),
            "all quota exhausted"
        );
        assert!(validate_reason(" ").is_err());
        assert!(validate_reason("bad\0reason").is_err());
    }

    #[test]
    fn defaults_changed_by_to_maestro_trace() {
        let ctx = TraceCtx::new_root("test.pause", "pause test");

        let actor = validated_changed_by(None, &ctx).unwrap();

        assert_eq!(actor, format!("maestro:{}", ctx.trace_id));
    }

    #[test]
    fn parses_versioned_health_subjects() {
        assert_eq!(
            method_from_subject("tool.supervise.ping.v001"),
            Some("ping")
        );
        assert_eq!(
            method_from_subject("tool.supervise.notify-human"),
            Some("notify-human")
        );
    }

    #[test]
    fn versioned_subject_prefix_subscription_keeps_methods_parseable() {
        let prefix = "tool.supervise.v047";

        assert_eq!(format!("{prefix}.>"), "tool.supervise.v047.>");
        assert_eq!(
            method_from_subject("tool.supervise.v047.notify-human"),
            Some("notify-human")
        );
        assert_eq!(
            method_from_subject("tool.supervise.v047.ping"),
            Some("ping")
        );
    }
}
