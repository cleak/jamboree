//! `jam-svc-message` - three message modes for Picker sessions (§5.7).
//!
//! The service owns the tool-shaped boundary for `enqueue-message`,
//! `interrupt-with-message`, and `full-stop`. Queue/interrupt currently publish
//! traced session-scoped command subjects and initial status events; true
//! prompt-boundary delivery is completed by future harness stdin/FIFO support.

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

const SERVICE_NAME: &str = "jam-svc-message";
const SERVICE_VERSION: &str = env!("CARGO_PKG_VERSION");
const SUBJECT_PREFIX: &str = "tool.message";
const SUBJECT_PREFIX_ENV: &str = "JAM_MESSAGE_SUBJECT_PREFIX";
const DEFAULT_SESSION_FULL_STOP_SUBJECT: &str = "tool.session.full-stop";
const DEFAULT_REQUEST_TIMEOUT_SECS: u64 = 30;
const TOKEN_MAX_LEN: usize = 128;
const TEXT_MAX_LEN: usize = 8_000;

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
enum MessageError {
    #[error("{kind}: {detail}")]
    Protocol {
        kind: &'static str,
        detail: String,
        remediation: &'static str,
        tracked_by: &'static str,
    },
}

impl MessageError {
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

#[derive(Debug, Clone)]
struct MessageConfig {
    session_full_stop_subject: String,
    request_timeout: Duration,
}

impl MessageConfig {
    fn from_env() -> Self {
        let session_full_stop_subject = std::env::var("JAM_SESSION_FULL_STOP_SUBJECT")
            .unwrap_or_else(|_| DEFAULT_SESSION_FULL_STOP_SUBJECT.into());
        let request_timeout = std::env::var("JAM_MESSAGE_REQUEST_TIMEOUT_SECS")
            .ok()
            .and_then(|raw| raw.parse().ok())
            .map_or(
                Duration::from_secs(DEFAULT_REQUEST_TIMEOUT_SECS),
                Duration::from_secs,
            );
        Self {
            session_full_stop_subject,
            request_timeout,
        }
    }
}

#[derive(Debug, Deserialize)]
struct MessageInput {
    session_id: String,
    text: String,
    #[serde(default = "default_from")]
    from: String,
}

#[derive(Debug, Deserialize)]
struct FullStopInput {
    session_id: String,
    reason: String,
    #[serde(default = "default_from", alias = "requested_by")]
    from: String,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "kebab-case")]
enum MessageMode {
    Queue,
    Interrupt,
    FullStop,
}

impl MessageMode {
    const fn command_token(self) -> &'static str {
        match self {
            Self::Queue => "queue",
            Self::Interrupt => "interrupt",
            Self::FullStop => "kill",
        }
    }

    const fn initial_status(self) -> &'static str {
        match self {
            Self::Queue => "queued",
            Self::Interrupt => "interrupt-requested",
            Self::FullStop => "kill-requested",
        }
    }
}

#[derive(Debug, Serialize)]
struct PickerMessagePayload<'a> {
    message_id: &'a str,
    session_id: &'a str,
    mode: MessageMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<&'a str>,
    from: &'a str,
    requested_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
struct PickerMessageStatusPayload<'a> {
    message_id: &'a str,
    session_id: &'a str,
    mode: MessageMode,
    status: &'static str,
    from: &'a str,
    detail: &'a Value,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
struct FullStopRequest<'a> {
    session_id: &'a str,
    reason: &'a str,
    requested_by: &'a str,
}

#[derive(Debug, Serialize)]
struct MessageOutput {
    message_id: String,
    session_id: String,
    mode: MessageMode,
    status: &'static str,
    subject: String,
    trace_id: String,
    detail: Value,
}

struct StatusUpdate<'a> {
    session_id: &'a str,
    mode: MessageMode,
    message_id: &'a str,
    status: &'static str,
    from: &'a str,
    detail: &'a Value,
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
        error!("jam-svc-message fatal: {err}");
        return std::process::ExitCode::FAILURE;
    }
    std::process::ExitCode::SUCCESS
}

async fn run() -> Result<(), ServiceError> {
    init_tracing();
    let nats_url = std::env::var("NATS_URL").unwrap_or_else(|_| "nats://127.0.0.1:4222".into());
    let nats_token = std::env::var("NATS_TOKEN").ok();
    let subject_prefix = configured_subject_prefix(SUBJECT_PREFIX_ENV, SUBJECT_PREFIX);
    let config = MessageConfig::from_env();

    info!(
        service = %SERVICE_NAME,
        version = %SERVICE_VERSION,
        nats = %nats_url,
        subject_prefix = %subject_prefix,
        "starting",
    );
    let nats = JamNats::connect(&nats_url, nats_token).await?;
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
                let config = config.clone();
                let draining = draining.clone();
                let active_requests = active_requests.clone();
                active_requests.fetch_add(1, Ordering::SeqCst);
                tokio::spawn(async move {
                    let result = handle_request(&nats, &message, &config, &draining).await;
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
    config: &MessageConfig,
    draining: &Arc<AtomicBool>,
) -> Result<(), ServiceError> {
    let method = method_from_subject(msg.subject.as_str()).unwrap_or("");
    debug!(subject = %msg.subject, method = %method, "received request");

    let response_ctx = msg
        .headers
        .as_ref()
        .and_then(jam_nats::extract_trace_from_headers);
    let response = match &response_ctx {
        Some(ctx) => dispatch(method, &msg.payload, ctx, nats, config).await,
        None => Response::Error {
            error: ResponseError {
                kind: "missing-trace".into(),
                detail: "tool.message requests must include Trace-Id headers".into(),
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

async fn dispatch(
    method: &str,
    payload: &[u8],
    ctx: &TraceCtx,
    nats: &JamNats,
    config: &MessageConfig,
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
        "enqueue-message" => tool_message(payload, MessageMode::Queue, ctx, nats).await,
        "interrupt-with-message" => tool_message(payload, MessageMode::Interrupt, ctx, nats).await,
        "full-stop" => full_stop(payload, ctx, nats, config).await,
        unknown => Response::Error {
            error: ResponseError {
                kind: "unknown-method".into(),
                detail: format!("{SUBJECT_PREFIX}.{unknown} is not a recognized message method"),
                remediation:
                    "Use tool.message.enqueue-message, interrupt-with-message, or full-stop.".into(),
                tracked_by: "comp-jam-svc-message",
            },
        },
    }
}

async fn tool_message(
    payload: &[u8],
    mode: MessageMode,
    ctx: &TraceCtx,
    nats: &JamNats,
) -> Response {
    let input = match parse_message_input(payload, mode) {
        Ok(input) => input,
        Err(err) => return error_response(err),
    };
    match publish_command_and_status(
        nats,
        &input.session_id,
        mode,
        &input.text,
        None,
        &input.from,
        ctx,
    )
    .await
    {
        Ok(output) => Response::Ok(serde_json::to_value(output).expect("output serializes")),
        Err(err) => error_response(err),
    }
}

async fn full_stop(
    payload: &[u8],
    ctx: &TraceCtx,
    nats: &JamNats,
    config: &MessageConfig,
) -> Response {
    let input = match parse_full_stop_input(payload) {
        Ok(input) => input,
        Err(err) => return error_response(err),
    };
    let command = publish_command_and_status(
        nats,
        &input.session_id,
        MessageMode::FullStop,
        "",
        Some(&input.reason),
        &input.from,
        ctx,
    )
    .await;
    let Ok(mut output) = command else {
        return error_response(command.unwrap_err());
    };

    let request = FullStopRequest {
        session_id: &input.session_id,
        reason: &input.reason,
        requested_by: &input.from,
    };
    let response: Value = match nats
        .request_traced(
            config.session_full_stop_subject.clone(),
            &request,
            ctx,
            config.request_timeout,
        )
        .await
    {
        Ok(response) => response,
        Err(err) => {
            let detail = serde_json::json!({ "error": err.to_string() });
            let _ = publish_status(
                nats,
                StatusUpdate {
                    session_id: &input.session_id,
                    mode: MessageMode::FullStop,
                    message_id: &output.message_id,
                    status: "delivery-failed",
                    from: &input.from,
                    detail: &detail,
                },
                ctx,
            )
            .await;
            return error_response(MessageError::protocol(
                "full-stop-request-failed",
                err.to_string(),
                "Verify jam-svc-session is running and subscribed to tool.session.full-stop.",
                "task-message-modes-ui",
            ));
        }
    };

    if let Some(error) = response.get("error") {
        let _ = publish_status(
            nats,
            StatusUpdate {
                session_id: &input.session_id,
                mode: MessageMode::FullStop,
                message_id: &output.message_id,
                status: "delivery-failed",
                from: &input.from,
                detail: error,
            },
            ctx,
        )
        .await;
        return error_response(MessageError::protocol(
            "full-stop-rejected",
            error.to_string(),
            "Use tool.session.list-active to verify the session is running.",
            "task-message-modes-ui",
        ));
    }

    if let Err(err) = publish_status(
        nats,
        StatusUpdate {
            session_id: &input.session_id,
            mode: MessageMode::FullStop,
            message_id: &output.message_id,
            status: "kill-confirmed",
            from: &input.from,
            detail: &response,
        },
        ctx,
    )
    .await
    {
        return error_response(err);
    }
    output.status = "kill-confirmed";
    output.subject = config.session_full_stop_subject.clone();
    output.detail = response;
    Response::Ok(serde_json::to_value(output).expect("output serializes"))
}

async fn publish_command_and_status(
    nats: &JamNats,
    session_id: &str,
    mode: MessageMode,
    text: &str,
    reason: Option<&str>,
    from: &str,
    ctx: &TraceCtx,
) -> Result<MessageOutput, MessageError> {
    let message_id = message_id(ctx);
    let subject = picker_message_subject(session_id, mode);
    let payload = PickerMessagePayload {
        message_id: &message_id,
        session_id,
        mode,
        text: (!text.is_empty()).then_some(text),
        reason,
        from,
        requested_at: Utc::now(),
    };
    nats.publish_traced(&subject, &payload, ctx)
        .await
        .map_err(|err| {
            MessageError::protocol(
                "message-publish-failed",
                err.to_string(),
                "Verify NATS is reachable and the picker subject is valid.",
                "comp-jam-svc-message",
            )
        })?;

    let status = mode.initial_status();
    let detail = serde_json::json!({});
    publish_status(
        nats,
        StatusUpdate {
            session_id,
            mode,
            message_id: &message_id,
            status,
            from,
            detail: &detail,
        },
        ctx,
    )
    .await?;
    Ok(MessageOutput {
        message_id,
        session_id: session_id.into(),
        mode,
        status,
        subject,
        trace_id: ctx.trace_id.to_string(),
        detail,
    })
}

async fn publish_status(
    nats: &JamNats,
    update: StatusUpdate<'_>,
    ctx: &TraceCtx,
) -> Result<(), MessageError> {
    let payload = PickerMessageStatusPayload {
        message_id: update.message_id,
        session_id: update.session_id,
        mode: update.mode,
        status: update.status,
        from: update.from,
        detail: update.detail,
        updated_at: Utc::now(),
    };
    nats.publish_traced(picker_status_subject(update.session_id), &payload, ctx)
        .await
        .map_err(|err| {
            MessageError::protocol(
                "message-status-publish-failed",
                err.to_string(),
                "Verify NATS is reachable and the picker status subject is valid.",
                "comp-jam-svc-message",
            )
        })
}

fn parse_message_input(payload: &[u8], mode: MessageMode) -> Result<MessageInput, MessageError> {
    let input: MessageInput = serde_json::from_slice(payload).map_err(|err| {
        MessageError::protocol(
            "invalid-input",
            format!(
                "tool.message.{} payload is invalid JSON: {err}",
                mode.command_token()
            ),
            "Send {\"session_id\":\"...\",\"text\":\"...\"}.",
            "api-enqueue-message",
        )
    })?;
    validate_session_id(&input.session_id)?;
    validate_text("text", &input.text)?;
    validate_from(&input.from)?;
    Ok(input)
}

fn parse_full_stop_input(payload: &[u8]) -> Result<FullStopInput, MessageError> {
    let input: FullStopInput = serde_json::from_slice(payload).map_err(|err| {
        MessageError::protocol(
            "invalid-input",
            format!("tool.message.full-stop payload is invalid JSON: {err}"),
            "Send {\"session_id\":\"...\",\"reason\":\"...\"}.",
            "api-full-stop",
        )
    })?;
    validate_session_id(&input.session_id)?;
    validate_text("reason", &input.reason)?;
    validate_from(&input.from)?;
    Ok(input)
}

fn validate_session_id(session_id: &str) -> Result<(), MessageError> {
    if session_id.is_empty() || session_id.len() > TOKEN_MAX_LEN {
        return Err(MessageError::protocol(
            "invalid-input",
            "session_id must be 1-128 characters",
            "Use tool.session.list-active to copy an existing session_id.",
            "comp-jam-svc-message",
        ));
    }
    if session_id.contains("..")
        || !session_id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | ':'))
    {
        return Err(MessageError::protocol(
            "invalid-input",
            format!("session_id contains unsafe characters: {session_id}"),
            "Use the exact session_id returned by spawn-picker/list-active.",
            "comp-jam-svc-message",
        ));
    }
    Ok(())
}

fn validate_text(field: &'static str, value: &str) -> Result<(), MessageError> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.len() > TEXT_MAX_LEN {
        return Err(MessageError::protocol(
            "invalid-input",
            format!("{field} must be 1-{TEXT_MAX_LEN} characters"),
            "Send a non-empty message within the UI text limit.",
            "comp-jam-svc-message",
        ));
    }
    Ok(())
}

fn validate_from(value: &str) -> Result<(), MessageError> {
    if value.is_empty() || value.len() > TOKEN_MAX_LEN {
        return Err(MessageError::protocol(
            "invalid-input",
            "from must be 1-128 characters",
            "Use a stable source identity such as human:caleb or maestro:<session>.",
            "comp-jam-svc-message",
        ));
    }
    if !value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | ':'))
    {
        return Err(MessageError::protocol(
            "invalid-input",
            format!("from contains unsafe characters: {value}"),
            "Use a stable source identity such as human:caleb or maestro:<session>.",
            "comp-jam-svc-message",
        ));
    }
    Ok(())
}

fn error_response(err: MessageError) -> Response {
    match err {
        MessageError::Protocol {
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

fn picker_message_subject(session_id: &str, mode: MessageMode) -> String {
    format!("picker.{session_id}.msg.{}", mode.command_token())
}

fn picker_status_subject(session_id: &str) -> String {
    format!("picker.{session_id}.msg.status")
}

fn message_id(ctx: &TraceCtx) -> String {
    format!("msg:{}", ctx.trace_id)
}

fn default_from() -> String {
    "maestro".into()
}

fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("jam_svc_message=info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versioned_subject_prefix_subscription_keeps_methods_parseable() {
        let prefix = "tool.message.v047";

        assert_eq!(format!("{prefix}.>"), "tool.message.v047.>");
        assert_eq!(
            method_from_subject("tool.message.v047.enqueue-message"),
            Some("enqueue-message")
        );
        assert_eq!(method_from_subject("tool.message.v047.ping"), Some("ping"));
    }

    #[test]
    fn session_id_validation_allows_picker_handles_but_rejects_subject_wildcards() {
        assert!(validate_session_id("codex-cli:01BRZ3NDEKTSV4RRFFQ69G5FAV").is_ok());
        assert!(validate_session_id("codex.cli:bad").is_err());
        assert!(validate_session_id("codex-cli:>").is_err());
        assert!(validate_session_id("../codex").is_err());
    }

    #[test]
    fn message_subjects_use_session_scoped_channels() {
        assert_eq!(
            picker_message_subject("codex-cli:abc", MessageMode::Queue),
            "picker.codex-cli:abc.msg.queue"
        );
        assert_eq!(
            picker_message_subject("codex-cli:abc", MessageMode::Interrupt),
            "picker.codex-cli:abc.msg.interrupt"
        );
        assert_eq!(
            picker_status_subject("codex-cli:abc"),
            "picker.codex-cli:abc.msg.status"
        );
    }

    #[test]
    fn message_input_validation_rejects_empty_text() {
        let payload = serde_json::json!({
            "session_id": "codex-cli:abc",
            "text": " ",
            "from": "human:caleb"
        });
        let err =
            parse_message_input(payload.to_string().as_bytes(), MessageMode::Queue).unwrap_err();

        assert!(err.to_string().contains("text must be"));
    }
}
