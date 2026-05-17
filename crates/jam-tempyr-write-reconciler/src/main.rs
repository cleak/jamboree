//! `jam-tempyr-write-reconciler` - retries trusted Tempyr write side effects.
//!
//! Orchestrator components persist a `tempyr.write-pending` journal event plus
//! a request file under `JAM_HOME/tempyr-write-requests/`. This reconciler
//! replays pending writes with bounded backoff, emits `tempyr.write-confirmed`
//! on success, and emits `tempyr.write-permanently-failed` plus `notify.human`
//! after retry exhaustion.

#![deny(missing_docs)]

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use chrono::Utc;
use clap::Parser;
use futures::StreamExt;
use jam_events::generated::{
    Event, TempyrWriteConfirmed, TempyrWritePending, TempyrWritePermanentlyFailed,
};
use jam_events::EventEnvelope;
use jam_nats::async_nats;
use jam_nats::JamNats;
use jam_trace::{TraceCtx, TraceId};
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use tokio::time::{sleep, Duration};
use tracing::{error, info, warn};

const SERVICE_NAME: &str = "jam-tempyr-write-reconciler";
const SERVICE_VERSION: &str = env!("CARGO_PKG_VERSION");
const DEFAULT_TEMPYR_BIN: &str = "tempyr";
const DEFAULT_BACKOFF_MS: &[u64] = &[100, 500, 2_000, 10_000, 60_000];

#[derive(Debug, Parser)]
#[command(
    name = SERVICE_NAME,
    version,
    about = "Retry queued Tempyr write side effects"
)]
struct Cli {
    /// Tempyr binary path.
    #[arg(long)]
    tempyr_bin: Option<PathBuf>,

    /// Trusted request directory. Defaults to JAM_HOME/tempyr-write-requests.
    #[arg(long)]
    request_root: Option<PathBuf>,

    /// Comma-separated retry delays in milliseconds.
    #[arg(long)]
    backoff_ms: Option<String>,

    /// Stop after this many pending events; useful for smoke tests.
    #[arg(long)]
    max_events: Option<u64>,
}

#[derive(Debug, thiserror::Error)]
enum WriteRetryError {
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

    #[error("protocol: {0}")]
    Protocol(String),
}

#[derive(Debug, Clone)]
struct Config {
    nats_url: String,
    nats_token: Option<String>,
    tempyr_bin: PathBuf,
    request_root: PathBuf,
    backoff: Vec<Duration>,
}

impl Config {
    fn from_env_and_cli(cli: &Cli) -> Result<Self, WriteRetryError> {
        let jam_home = jam_tools_core::paths::jam_home();
        let request_root = cli
            .request_root
            .clone()
            .or_else(|| std::env::var_os("JAM_TEMPYR_WRITE_REQUESTS").map(PathBuf::from))
            .unwrap_or_else(|| jam_home.join("tempyr-write-requests"));
        let tempyr_bin = cli
            .tempyr_bin
            .clone()
            .or_else(|| std::env::var_os("JAM_TEMPYR_BIN").map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from(DEFAULT_TEMPYR_BIN));
        let backoff = parse_backoff(cli.backoff_ms.as_deref())?;

        Ok(Self {
            nats_url: std::env::var("NATS_URL").unwrap_or_else(|_| "nats://127.0.0.1:4222".into()),
            nats_token: std::env::var("NATS_TOKEN").ok(),
            tempyr_bin,
            request_root,
            backoff,
        })
    }
}

fn parse_backoff(raw: Option<&str>) -> Result<Vec<Duration>, WriteRetryError> {
    let values: Vec<u64> = match raw {
        Some(raw) => raw
            .split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(|part| {
                part.parse::<u64>().map_err(|err| {
                    WriteRetryError::Protocol(format!("invalid backoff {part}: {err}"))
                })
            })
            .collect::<Result<_, _>>()?,
        None => DEFAULT_BACKOFF_MS.to_vec(),
    };
    if values.is_empty() {
        return Err(WriteRetryError::Protocol(
            "backoff list must contain at least one delay".into(),
        ));
    }
    Ok(values.into_iter().map(Duration::from_millis).collect())
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    if let Err(err) = run().await {
        error!("jam-tempyr-write-reconciler fatal: {err}");
        return std::process::ExitCode::FAILURE;
    }
    std::process::ExitCode::SUCCESS
}

async fn run() -> Result<(), WriteRetryError> {
    init_tracing();
    let cli = Cli::parse();
    let config = Config::from_env_and_cli(&cli)?;

    info!(
        service = %SERVICE_NAME,
        version = %SERVICE_VERSION,
        nats = %config.nats_url,
        tempyr = %config.tempyr_bin.display(),
        request_root = %config.request_root.display(),
        "starting",
    );

    let nats = JamNats::connect(&config.nats_url, config.nats_token.clone()).await?;
    info!("connected to NATS");

    let mut sub = nats
        .client()
        .subscribe("journal.tempyr.write-pending")
        .await
        .map_err(|err| WriteRetryError::Subscribe(err.to_string()))?;
    info!(subject = "journal.tempyr.write-pending", "subscribed");

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
            msg = sub.next() => {
                let Some(message) = msg else {
                    warn!("write-pending subscription closed");
                    return Ok(());
                };
                let envelope = parse_pending_message(&message)?;
                let write_id = envelope.payload.write_id.clone();
                if seen.insert(write_id) {
                    process_pending(&nats, &config, envelope).await?;
                }
                handled = handled.saturating_add(1);
                if cli.max_events.is_some_and(|max_events| handled >= max_events) {
                    info!(handled, "max events reached");
                    return Ok(());
                }
            }
        }
    }
}

fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("jam_tempyr_write_reconciler=info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

fn parse_pending_message(
    message: &async_nats::Message,
) -> Result<EventEnvelope<TempyrWritePending>, WriteRetryError> {
    let envelope: EventEnvelope<TempyrWritePending> = serde_json::from_slice(&message.payload)?;
    if envelope.event_type != TempyrWritePending::EVENT_TYPE {
        return Err(WriteRetryError::Protocol(format!(
            "expected event_type {}, got {}",
            TempyrWritePending::EVENT_TYPE,
            envelope.event_type
        )));
    }
    Ok(envelope)
}

async fn process_pending(
    nats: &JamNats,
    config: &Config,
    envelope: EventEnvelope<TempyrWritePending>,
) -> Result<(), WriteRetryError> {
    let ctx = trace_ctx_from_envelope(&envelope)?;
    let payload = envelope.payload;

    let request = match load_request(config, &payload) {
        Ok(request) => request,
        Err(err) => {
            publish_permanent_failure(nats, &ctx, &payload, 0, &err.to_string()).await?;
            publish_notify_human(nats, &ctx, &payload, 0, &err.to_string()).await?;
            return Ok(());
        }
    };

    let outcome = execute_with_retry(config, &request).await;
    match outcome {
        WriteOutcome::Confirmed { attempts } => {
            publish_confirmed(nats, &ctx, &payload, attempts).await?;
        }
        WriteOutcome::PermanentlyFailed {
            attempts,
            last_error,
        } => {
            publish_permanent_failure(nats, &ctx, &payload, attempts, &last_error).await?;
            publish_notify_human(nats, &ctx, &payload, attempts, &last_error).await?;
        }
    }
    Ok(())
}

fn trace_ctx_from_envelope<P>(envelope: &EventEnvelope<P>) -> Result<TraceCtx, WriteRetryError> {
    let trace_id = TraceId::from_str(&envelope.trace_id)?;
    let parent_trace_id = envelope
        .parent_trace_id
        .as_deref()
        .map(TraceId::from_str)
        .transpose()?;
    Ok(TraceCtx {
        trace_id,
        parent_trace_id,
        origin_kind: "tempyr.write-pending",
        origin_summary: format!("Tempyr write {}", envelope.event_type),
    })
}

#[derive(Debug, Clone, Deserialize)]
struct TempyrWriteRequest {
    schema_version: u32,
    write_id: String,
    node_id: String,
    operation: String,
    worktree: Option<PathBuf>,
    args: Vec<String>,
}

fn load_request(
    config: &Config,
    pending: &TempyrWritePending,
) -> Result<TempyrWriteRequest, WriteRetryError> {
    let path = trusted_request_path(&config.request_root, &pending.request_path)?;
    let raw = fs::read_to_string(&path)?;
    let request: TempyrWriteRequest = serde_json::from_str(&raw)?;
    validate_request_matches_pending(&request, pending)?;
    validate_tempyr_args(&request.args)?;
    if let Some(worktree) = &request.worktree {
        validate_native_home_path(worktree)?;
    }
    Ok(request)
}

fn trusted_request_path(root: &Path, request_path: &str) -> Result<PathBuf, WriteRetryError> {
    fs::create_dir_all(root)?;
    let root = root.canonicalize()?;
    let candidate = PathBuf::from(request_path);
    let candidate = if candidate.is_absolute() {
        candidate
    } else {
        root.join(candidate)
    };
    let candidate = candidate.canonicalize()?;
    if candidate.starts_with(&root) {
        Ok(candidate)
    } else {
        Err(WriteRetryError::Protocol(format!(
            "request path {} is outside trusted root {}",
            candidate.display(),
            root.display()
        )))
    }
}

fn validate_request_matches_pending(
    request: &TempyrWriteRequest,
    pending: &TempyrWritePending,
) -> Result<(), WriteRetryError> {
    if request.schema_version != 1 {
        return Err(WriteRetryError::Protocol(format!(
            "unsupported request schema_version {}",
            request.schema_version
        )));
    }
    for (label, request_value, pending_value) in [
        ("write_id", &request.write_id, &pending.write_id),
        ("node_id", &request.node_id, &pending.node_id),
        ("operation", &request.operation, &pending.operation),
    ] {
        if request_value != pending_value {
            return Err(WriteRetryError::Protocol(format!(
                "request {label} {request_value:?} does not match pending event {pending_value:?}"
            )));
        }
    }
    Ok(())
}

fn validate_tempyr_args(args: &[String]) -> Result<(), WriteRetryError> {
    if args.is_empty() {
        return Err(WriteRetryError::Protocol("tempyr args are empty".into()));
    }
    if args.iter().any(|arg| arg.is_empty() || arg.contains('\0')) {
        return Err(WriteRetryError::Protocol(
            "tempyr args may not be empty or contain NUL".into(),
        ));
    }
    match args[0].as_str() {
        "add" | "add-edge" | "remove-edge" | "rename" | "status" => Ok(()),
        "journal" => match args.get(1).map(String::as_str) {
            Some("log" | "flush" | "finalize" | "bootstrap") => Ok(()),
            Some(other) => Err(WriteRetryError::Protocol(format!(
                "tempyr journal subcommand {other:?} is not an allowed write"
            ))),
            None => Err(WriteRetryError::Protocol(
                "tempyr journal write requires a subcommand".into(),
            )),
        },
        other => Err(WriteRetryError::Protocol(format!(
            "tempyr command {other:?} is not an allowed write"
        ))),
    }
}

fn validate_native_home_path(path: &Path) -> Result<(), WriteRetryError> {
    let text = path.to_string_lossy();
    if text.starts_with("/cygdrive/") || is_drvfs_mount(&text) {
        return Err(WriteRetryError::Protocol(format!(
            "worktree path {} is on a Windows mount",
            path.display()
        )));
    }
    if !text.starts_with("/home/") {
        return Err(WriteRetryError::Protocol(format!(
            "worktree path {} must be under /home",
            path.display()
        )));
    }
    Ok(())
}

fn is_drvfs_mount(path: &str) -> bool {
    let Some(rest) = path.strip_prefix("/mnt/") else {
        return false;
    };
    let mut chars = rest.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_lowercase() && matches!(chars.next(), None | Some('/'))
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum WriteOutcome {
    Confirmed { attempts: u32 },
    PermanentlyFailed { attempts: u32, last_error: String },
}

async fn execute_with_retry(config: &Config, request: &TempyrWriteRequest) -> WriteOutcome {
    let max_attempts = config.backoff.len().saturating_add(1);
    let mut last_error = String::new();
    for attempt_index in 0..max_attempts {
        let attempts = u32::try_from(attempt_index.saturating_add(1)).unwrap_or(u32::MAX);
        match execute_once(config, request).await {
            Ok(()) => return WriteOutcome::Confirmed { attempts },
            Err(err) => {
                last_error = err;
                if let Some(delay) = config.backoff.get(attempt_index) {
                    sleep(*delay).await;
                }
            }
        }
    }
    WriteOutcome::PermanentlyFailed {
        attempts: u32::try_from(max_attempts).unwrap_or(u32::MAX),
        last_error,
    }
}

async fn execute_once(config: &Config, request: &TempyrWriteRequest) -> Result<(), String> {
    let mut command = Command::new(&config.tempyr_bin);
    command.args(&request.args);
    if let Some(worktree) = &request.worktree {
        command.current_dir(worktree);
    }
    let output = command
        .output()
        .await
        .map_err(|err| format!("run {}: {err}", config.tempyr_bin.display()))?;
    if output.status.success() {
        return Ok(());
    }
    let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if detail.is_empty() {
        Err(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(detail)
    }
}

async fn publish_confirmed(
    nats: &JamNats,
    ctx: &TraceCtx,
    pending: &TempyrWritePending,
    attempts: u32,
) -> Result<(), WriteRetryError> {
    let payload = TempyrWriteConfirmed {
        write_id: pending.write_id.clone(),
        node_id: pending.node_id.clone(),
        operation: pending.operation.clone(),
        attempts,
        ts: Utc::now(),
    };
    publish_journal_event(nats, payload, ctx).await
}

async fn publish_permanent_failure(
    nats: &JamNats,
    ctx: &TraceCtx,
    pending: &TempyrWritePending,
    attempts: u32,
    last_error: &str,
) -> Result<(), WriteRetryError> {
    let payload = TempyrWritePermanentlyFailed {
        write_id: Some(pending.write_id.clone()),
        node_id: pending.node_id.clone(),
        operation: pending.operation.clone(),
        request_path: Some(pending.request_path.clone()),
        last_error: last_error.to_owned(),
        attempts,
        ts: Utc::now(),
    };
    publish_journal_event(nats, payload, ctx).await
}

#[derive(Debug, Serialize)]
struct NotifyHumanPayload<'a> {
    urgency: &'a str,
    summary: &'a str,
    payload: NotifyHumanDetail<'a>,
}

#[derive(Debug, Serialize)]
struct NotifyHumanDetail<'a> {
    write_id: &'a str,
    node_id: &'a str,
    operation: &'a str,
    request_path: &'a str,
    attempts: u32,
    last_error: &'a str,
}

async fn publish_notify_human(
    nats: &JamNats,
    ctx: &TraceCtx,
    pending: &TempyrWritePending,
    attempts: u32,
    last_error: &str,
) -> Result<(), WriteRetryError> {
    let payload = NotifyHumanPayload {
        urgency: "high",
        summary: "Tempyr write retry exhausted",
        payload: NotifyHumanDetail {
            write_id: &pending.write_id,
            node_id: &pending.node_id,
            operation: &pending.operation,
            request_path: &pending.request_path,
            attempts,
            last_error,
        },
    };
    nats.publish_traced("notify.human", &payload, ctx).await?;
    Ok(())
}

async fn publish_journal_event<P>(
    nats: &JamNats,
    payload: P,
    ctx: &TraceCtx,
) -> Result<(), WriteRetryError>
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
    nats.publish_traced(format!("journal.{}", P::EVENT_TYPE), &envelope, ctx)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn parses_backoff_ms() {
        let parsed = parse_backoff(Some("1, 2,3")).unwrap();
        assert_eq!(
            parsed,
            vec![
                Duration::from_millis(1),
                Duration::from_millis(2),
                Duration::from_millis(3)
            ]
        );
        assert!(parse_backoff(Some("")).is_err());
    }

    #[test]
    fn validates_request_path_stays_under_root() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().join("requests");
        fs::create_dir_all(&root).unwrap();
        let request = root.join("one.json");
        fs::write(&request, "{}").unwrap();
        let outside = tmp.path().join("outside.json");
        fs::write(&outside, "{}").unwrap();

        assert_eq!(trusted_request_path(&root, "one.json").unwrap(), request);
        assert!(trusted_request_path(&root, outside.to_str().unwrap()).is_err());
    }

    #[test]
    fn validates_tempyr_args_are_write_shaped() {
        assert!(validate_tempyr_args(&["status".into(), "task-1".into(), "done".into()]).is_ok());
        assert!(validate_tempyr_args(&["journal".into(), "log".into()]).is_ok());
        assert!(validate_tempyr_args(&["search".into(), "task".into()]).is_err());
        assert!(validate_tempyr_args(&["journal".into(), "search".into()]).is_err());
    }

    #[tokio::test]
    async fn retry_succeeds_after_transient_failure() {
        let tmp = TempDir::new().unwrap();
        let tempyr = fake_tempyr(tmp.path(), true);
        let config = Config {
            nats_url: "nats://127.0.0.1:4222".into(),
            nats_token: None,
            tempyr_bin: tempyr,
            request_root: tmp.path().join("requests"),
            // Backoff was 1ms which intermittently flaked on slow CI runners
            // where the shell script's `printf > {state}` for attempt 1
            // hadn't reached the filesystem before attempt 2 ran. 25ms gives
            // plenty of headroom without slowing the local run noticeably.
            backoff: vec![Duration::from_millis(25), Duration::from_millis(25)],
        };
        let request = test_request(tmp.path());

        let outcome = execute_with_retry(&config, &request).await;

        assert_eq!(outcome, WriteOutcome::Confirmed { attempts: 2 });
    }

    #[tokio::test]
    async fn retry_exhaustion_reports_attempts() {
        let tmp = TempDir::new().unwrap();
        let tempyr = fake_tempyr(tmp.path(), false);
        let config = Config {
            nats_url: "nats://127.0.0.1:4222".into(),
            nats_token: None,
            tempyr_bin: tempyr,
            request_root: tmp.path().join("requests"),
            backoff: vec![Duration::from_millis(1), Duration::from_millis(1)],
        };
        let request = test_request(tmp.path());

        let outcome = execute_with_retry(&config, &request).await;

        assert_eq!(
            outcome,
            WriteOutcome::PermanentlyFailed {
                attempts: 3,
                last_error: "fake tempyr failed".into()
            }
        );
    }

    fn test_request(root: &Path) -> TempyrWriteRequest {
        TempyrWriteRequest {
            schema_version: 1,
            write_id: "write-1".into(),
            node_id: "task-1".into(),
            operation: "status".into(),
            worktree: Some(root.to_path_buf()),
            args: vec!["status".into(), "task-1".into(), "done".into()],
        }
    }

    #[cfg(unix)]
    fn fake_tempyr(root: &Path, succeed_second: bool) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;

        let script = root.join("fake-tempyr");
        let state = root.join("attempts");
        let body = if succeed_second {
            format!(
                "#!/bin/sh\nn=$(cat {state} 2>/dev/null || printf 0)\nn=$((n + 1))\nprintf '%s' \"$n\" > {state}\nif [ \"$n\" -lt 2 ]; then echo 'fake tempyr failed' >&2; exit 1; fi\nexit 0\n",
                state = state.display()
            )
        } else {
            "#!/bin/sh\necho 'fake tempyr failed' >&2\nexit 1\n".into()
        };
        fs::write(&script, body).unwrap();
        let mut permissions = fs::metadata(&script).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script, permissions).unwrap();
        script
    }

    #[cfg(not(unix))]
    fn fake_tempyr(_root: &Path, _succeed_second: bool) -> PathBuf {
        PathBuf::from("tempyr")
    }
}
