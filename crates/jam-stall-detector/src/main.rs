//! `jam-stall-detector` - cheap deterministic Picker stall detection (§4.4.6).
//!
//! This process watches live Picker lifecycle/output subjects and emits
//! `picker.stalled` when deterministic stall rules trip. It does not interrupt,
//! kill, or otherwise make policy decisions; the Maestro wakes on the event and
//! decides what to do.

#![deny(missing_docs)]

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use clap::Parser;
use futures::StreamExt;
use jam_events::generated::{Event, PickerStalled};
use jam_nats::async_nats;
use jam_nats::JamNats;
use jam_trace::TraceCtx;
use serde_json::Value;
use tokio::time::{self, Duration};
use tracing::{error, info, warn};

const SERVICE_NAME: &str = "jam-stall-detector";
const SERVICE_VERSION: &str = env!("CARGO_PKG_VERSION");
const DEFAULT_TOKEN_IDLE_SECS: u64 = 90;
const DEFAULT_TOOL_LOOP_THRESHOLD: u32 = 3;
const DEFAULT_TICK_SECS: u64 = 5;

#[derive(Debug, Parser)]
#[command(name = SERVICE_NAME, version, about = "Detect stalled Picker sessions")]
struct Cli {
    /// Emit token-idle stalls after this many seconds without output.
    #[arg(long)]
    token_idle_secs: Option<u64>,

    /// Emit tool-loop stalls after the same tool+arguments repeats N times.
    #[arg(long)]
    tool_loop_threshold: Option<u32>,

    /// Idle scan cadence in seconds.
    #[arg(long)]
    tick_secs: Option<u64>,
}

#[derive(Debug, thiserror::Error)]
enum StallError {
    #[error("nats: {0}")]
    Nats(#[from] jam_nats::NatsError),

    #[error("subscribe: {0}")]
    Subscribe(String),

    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone)]
struct Config {
    nats_url: String,
    nats_token: Option<String>,
    token_idle_secs: u64,
    tool_loop_threshold: u32,
    tick_secs: u64,
}

impl Config {
    fn from_env_and_cli(cli: &Cli) -> Self {
        let nats_url = std::env::var("NATS_URL").unwrap_or_else(|_| "nats://127.0.0.1:4222".into());
        let nats_token = std::env::var("NATS_TOKEN").ok();
        let token_idle_secs = cli.token_idle_secs.unwrap_or_else(|| {
            env_parse("JAM_STALL_TOKEN_IDLE_SECS").unwrap_or(DEFAULT_TOKEN_IDLE_SECS)
        });
        let tool_loop_threshold = cli.tool_loop_threshold.unwrap_or_else(|| {
            env_parse("JAM_STALL_TOOL_LOOP_THRESHOLD").unwrap_or(DEFAULT_TOOL_LOOP_THRESHOLD)
        });
        let tick_secs = cli
            .tick_secs
            .unwrap_or_else(|| env_parse("JAM_STALL_TICK_SECS").unwrap_or(DEFAULT_TICK_SECS));
        Self {
            nats_url,
            nats_token,
            token_idle_secs,
            tool_loop_threshold,
            tick_secs,
        }
    }
}

fn env_parse<T>(name: &str) -> Option<T>
where
    T: std::str::FromStr,
{
    std::env::var(name).ok()?.parse().ok()
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    if let Err(err) = run().await {
        error!("jam-stall-detector fatal: {err}");
        return std::process::ExitCode::FAILURE;
    }
    std::process::ExitCode::SUCCESS
}

async fn run() -> Result<(), StallError> {
    init_tracing();
    let cli = Cli::parse();
    let config = Config::from_env_and_cli(&cli);
    info!(
        service = %SERVICE_NAME,
        version = %SERVICE_VERSION,
        nats = %config.nats_url,
        token_idle_secs = config.token_idle_secs,
        tool_loop_threshold = config.tool_loop_threshold,
        tick_secs = config.tick_secs,
        "starting",
    );

    let nats = JamNats::connect(&config.nats_url, config.nats_token.clone()).await?;
    info!("connected to NATS");
    let mut lifecycle_sub = nats
        .client()
        .subscribe("picker.*.lifecycle")
        .await
        .map_err(|e| StallError::Subscribe(e.to_string()))?;
    let mut output_sub = nats
        .client()
        .subscribe("picker.*.output")
        .await
        .map_err(|e| StallError::Subscribe(e.to_string()))?;
    info!("subscribed to picker.*.lifecycle and picker.*.output");

    let mut detector = Detector::new(config);
    let mut interval = time::interval(Duration::from_secs(detector.config.tick_secs));
    interval.set_missed_tick_behavior(time::MissedTickBehavior::Delay);
    let shutdown = tokio::signal::ctrl_c();
    tokio::pin!(shutdown);

    loop {
        tokio::select! {
            _ = &mut shutdown => {
                info!("shutdown signal received");
                return Ok(());
            }
            _ = interval.tick() => {
                let stalls = detector.detect_idle(Utc::now());
                publish_stalls(&nats, stalls).await?;
            }
            msg = lifecycle_sub.next() => {
                let Some(message) = msg else {
                    warn!("lifecycle subscription closed");
                    return Ok(());
                };
                if let Err(err) = detector.handle_lifecycle(&message, Utc::now()) {
                    warn!(subject = %message.subject, "lifecycle message ignored: {err}");
                }
            }
            msg = output_sub.next() => {
                let Some(message) = msg else {
                    warn!("output subscription closed");
                    return Ok(());
                };
                match detector.handle_output(&message, Utc::now()) {
                    Ok(stalls) => publish_stalls(&nats, stalls).await?,
                    Err(err) => warn!(subject = %message.subject, "output message ignored: {err}"),
                }
            }
        }
    }
}

async fn publish_stalls(nats: &JamNats, stalls: Vec<Stall>) -> Result<(), StallError> {
    for stall in stalls {
        nats.publish_traced(PickerStalled::EVENT_TYPE, &stall.payload, &stall.trace)
            .await?;
        info!(
            session_id = %stall.payload.session_id,
            task_id = %stall.payload.task_id,
            stall_kind = %stall.payload.stall_kind,
            stall_secs = stall.payload.stall_secs,
            "published picker.stalled",
        );
    }
    Ok(())
}

#[derive(Debug)]
struct Detector {
    config: Config,
    sessions: HashMap<String, SessionState>,
}

impl Detector {
    fn new(config: Config) -> Self {
        Self {
            config,
            sessions: HashMap::new(),
        }
    }

    fn handle_lifecycle(
        &mut self,
        message: &async_nats::Message,
        now: DateTime<Utc>,
    ) -> Result<(), StallError> {
        let trace = trace_from_message(message)?;
        let payload: Value = serde_json::from_slice(&message.payload)?;
        let Some(session_id) = session_id_from_subject(&message.subject, "lifecycle")
            .or_else(|| string_field(&payload, "session_id"))
        else {
            warn!(subject = %message.subject, "lifecycle message missing session_id");
            return Ok(());
        };
        let lifecycle = string_field(&payload, "lifecycle")
            .or_else(|| string_field(&payload, "event"))
            .or_else(|| string_field(&payload, "event_type"))
            .unwrap_or_else(|| "progress".to_owned());
        if matches!(
            lifecycle.as_str(),
            "exited" | "killed" | "picker.exited" | "picker.killed"
        ) {
            self.sessions.remove(&session_id);
            return Ok(());
        }

        let task_id = string_field(&payload, "task_id")
            .or_else(|| {
                self.sessions
                    .get(&session_id)
                    .map(|state| state.task_id.clone())
            })
            .unwrap_or_else(|| "unknown".to_owned());
        let state = self
            .sessions
            .entry(session_id.clone())
            .or_insert_with(|| SessionState::new(session_id, task_id.clone(), trace.clone(), now));
        state.task_id = task_id;
        state.trace = trace;
        if matches!(
            lifecycle.as_str(),
            "spawned" | "first-output" | "picker.spawned" | "picker.first-output" | "progress"
        ) {
            state.last_output_at = timestamp_field(&payload, "ts")
                .or_else(|| timestamp_field(&payload, "spawned_at"))
                .unwrap_or(now);
        }
        Ok(())
    }

    fn handle_output(
        &mut self,
        message: &async_nats::Message,
        now: DateTime<Utc>,
    ) -> Result<Vec<Stall>, StallError> {
        let trace = trace_from_message(message)?;
        let payload: Value = serde_json::from_slice(&message.payload)?;
        let Some(session_id) = session_id_from_subject(&message.subject, "output")
            .or_else(|| string_field(&payload, "session_id"))
        else {
            warn!(subject = %message.subject, "output message missing session_id");
            return Ok(Vec::new());
        };
        let task_id = string_field(&payload, "task_id")
            .or_else(|| {
                self.sessions
                    .get(&session_id)
                    .map(|state| state.task_id.clone())
            })
            .unwrap_or_else(|| "unknown".to_owned());
        let state = self
            .sessions
            .entry(session_id.clone())
            .or_insert_with(|| SessionState::new(session_id, task_id.clone(), trace.clone(), now));
        state.task_id = task_id;
        state.trace = trace.clone();

        if payload_has_tokens(&payload) {
            state.last_output_at = timestamp_field(&payload, "ts").unwrap_or(now);
        }

        let mut stalls = Vec::new();
        if let Some(signature) = tool_signature(&payload) {
            if state.last_tool.as_ref() == Some(&signature) {
                state.tool_repeat_count = state.tool_repeat_count.saturating_add(1);
            } else {
                state.last_tool = Some(signature);
                state.tool_repeat_count = 1;
            }
            if state.tool_repeat_count >= self.config.tool_loop_threshold
                && state.emitted_kinds.insert("tool-loop".to_owned())
            {
                stalls.push(state.stall("tool-loop", 0, now));
            }
        }
        Ok(stalls)
    }

    fn detect_idle(&mut self, now: DateTime<Utc>) -> Vec<Stall> {
        let mut stalls = Vec::new();
        for state in self.sessions.values_mut() {
            let idle_secs = nonnegative_secs(now - state.last_output_at);
            if idle_secs >= self.config.token_idle_secs
                && state.emitted_kinds.insert("token-idle".to_owned())
            {
                stalls.push(state.stall("token-idle", idle_secs, now));
            }
        }
        stalls
    }
}

#[derive(Debug, Clone)]
struct SessionState {
    session_id: String,
    task_id: String,
    trace: TraceCtx,
    last_output_at: DateTime<Utc>,
    last_tool: Option<ToolSignature>,
    tool_repeat_count: u32,
    emitted_kinds: HashSet<String>,
}

impl SessionState {
    fn new(
        session_id: String,
        task_id: String,
        trace: TraceCtx,
        last_output_at: DateTime<Utc>,
    ) -> Self {
        Self {
            session_id,
            task_id,
            trace,
            last_output_at,
            last_tool: None,
            tool_repeat_count: 0,
            emitted_kinds: HashSet::new(),
        }
    }

    fn stall(&self, stall_kind: &str, stall_secs: u64, detected_at: DateTime<Utc>) -> Stall {
        Stall {
            trace: self.trace.clone(),
            payload: PickerStalled {
                session_id: self.session_id.clone(),
                task_id: self.task_id.clone(),
                stall_kind: stall_kind.to_owned(),
                stall_secs: u32::try_from(stall_secs).unwrap_or(u32::MAX),
                detected_at,
            },
        }
    }
}

#[derive(Debug, Clone)]
struct Stall {
    trace: TraceCtx,
    payload: PickerStalled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ToolSignature {
    name: String,
    arguments: String,
}

fn trace_from_message(message: &async_nats::Message) -> Result<TraceCtx, StallError> {
    message
        .headers
        .as_ref()
        .and_then(jam_nats::extract_trace_from_headers)
        .ok_or_else(|| {
            serde_json::Error::io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "missing or invalid Trace-Id header",
            ))
        })
        .map_err(StallError::Json)
}

fn session_id_from_subject(subject: &str, suffix: &str) -> Option<String> {
    let mut parts = subject.split('.');
    let prefix = parts.next()?;
    let session_id = parts.next()?;
    let actual_suffix = parts.next()?;
    if parts.next().is_some()
        || prefix != "picker"
        || actual_suffix != suffix
        || session_id.is_empty()
    {
        return None;
    }
    Some(session_id.to_owned())
}

fn string_field(payload: &Value, field: &str) -> Option<String> {
    payload
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned)
}

fn timestamp_field(payload: &Value, field: &str) -> Option<DateTime<Utc>> {
    let raw = payload.get(field)?.as_str()?;
    DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

fn payload_has_tokens(payload: &Value) -> bool {
    let has_text = ["text", "content", "delta", "line"]
        .into_iter()
        .filter_map(|field| payload.get(field).and_then(Value::as_str))
        .any(|value| !value.is_empty());
    let has_token_count = ["token_count", "tokens", "output_tokens"]
        .into_iter()
        .filter_map(|field| payload.get(field).and_then(Value::as_u64))
        .any(|value| value > 0);
    has_text || has_token_count
}

fn tool_signature(payload: &Value) -> Option<ToolSignature> {
    let source = payload.get("tool_call").unwrap_or(payload);
    let name = source
        .get("tool_name")
        .or_else(|| source.get("name"))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())?;
    let arguments = source
        .get("arguments")
        .or_else(|| source.get("arguments_json"))
        .or_else(|| source.get("input"))
        .map_or_else(|| "{}".to_owned(), canonical_json);
    Some(ToolSignature {
        name: name.to_owned(),
        arguments,
    })
}

fn canonical_json(value: &Value) -> String {
    if let Some(raw) = value.as_str() {
        raw.to_owned()
    } else {
        serde_json::to_string(value).unwrap_or_else(|_| "null".to_owned())
    }
}

fn nonnegative_secs(delta: chrono::TimeDelta) -> u64 {
    u64::try_from(delta.num_seconds()).unwrap_or(0)
}

fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("jam_stall_detector=info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

#[cfg(test)]
mod tests {
    use super::*;
    use jam_trace::TraceId;
    use std::str::FromStr;

    #[test]
    fn detects_token_idle_once() {
        let trace = trace();
        let mut detector = Detector::new(test_config());
        let now = ts("2026-05-06T06:00:00Z");
        detector.sessions.insert(
            "session-1".to_owned(),
            SessionState::new(
                "session-1".to_owned(),
                "task-1".to_owned(),
                trace,
                now - chrono::TimeDelta::seconds(91),
            ),
        );

        let stalls = detector.detect_idle(now);
        let second = detector.detect_idle(now + chrono::TimeDelta::seconds(10));

        assert_eq!(stalls.len(), 1);
        assert_eq!(stalls[0].payload.stall_kind, "token-idle");
        assert_eq!(stalls[0].payload.stall_secs, 91);
        assert!(second.is_empty());
    }

    #[test]
    fn detects_same_tool_loop_once() {
        let mut detector = Detector::new(test_config());
        let now = ts("2026-05-06T06:00:00Z");
        let payload = serde_json::json!({
            "task_id": "task-1",
            "tool_name": "read-file",
            "arguments": {"path": "src/lib.rs"}
        });

        assert!(detector
            .observe_output_value("session-1", &payload, trace(), now)
            .is_empty());
        assert!(detector
            .observe_output_value("session-1", &payload, trace(), now)
            .is_empty());
        let stalls = detector.observe_output_value("session-1", &payload, trace(), now);
        let second = detector.observe_output_value("session-1", &payload, trace(), now);

        assert_eq!(stalls.len(), 1);
        assert_eq!(stalls[0].payload.stall_kind, "tool-loop");
        assert!(second.is_empty());
    }

    impl Detector {
        fn observe_output_value(
            &mut self,
            session_id: &str,
            payload: &Value,
            trace: TraceCtx,
            now: DateTime<Utc>,
        ) -> Vec<Stall> {
            let task_id = string_field(payload, "task_id").unwrap_or_else(|| "unknown".into());
            let state = self
                .sessions
                .entry(session_id.to_owned())
                .or_insert_with(|| {
                    SessionState::new(session_id.to_owned(), task_id.clone(), trace.clone(), now)
                });
            state.task_id = task_id;
            state.trace = trace;
            if payload_has_tokens(payload) {
                state.last_output_at = now;
            }
            let Some(signature) = tool_signature(payload) else {
                return Vec::new();
            };
            if state.last_tool.as_ref() == Some(&signature) {
                state.tool_repeat_count += 1;
            } else {
                state.last_tool = Some(signature);
                state.tool_repeat_count = 1;
            }
            if state.tool_repeat_count >= self.config.tool_loop_threshold
                && state.emitted_kinds.insert("tool-loop".to_owned())
            {
                vec![state.stall("tool-loop", 0, now)]
            } else {
                Vec::new()
            }
        }
    }

    fn test_config() -> Config {
        Config {
            nats_url: "nats://127.0.0.1:4222".into(),
            nats_token: None,
            token_idle_secs: 90,
            tool_loop_threshold: 3,
            tick_secs: 5,
        }
    }

    fn trace() -> TraceCtx {
        TraceCtx {
            trace_id: TraceId::from_str("01ARZ3NDEKTSV4RRFFQ69G5FAV").unwrap(),
            parent_trace_id: None,
            origin_kind: "test",
            origin_summary: String::new(),
        }
    }

    fn ts(raw: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(raw)
            .unwrap()
            .with_timezone(&Utc)
    }
}
