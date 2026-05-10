//! `jam-nats-bridge` — subscribes to `journal.>` and writes the messages to
//! disk via [`jam_journal::JournalWriter`].
//!
//! Per `task-journal-writer-with-secret-redaction` (§12 Phase 0; §24.9 step 4
//! NATS half) and `comp-orchestrator-jsonl-journal`. This daemon is the
//! NATS-side complement to [`jam_journal`]: NATS publishes are durable in the
//! JetStream `journal` stream; this bridge consumes the stream and lands the
//! lines on disk for human inspection (`tail -f`) and the offline FTS5
//! reconciler.
//!
//! ## Configuration (env)
//!
//! | Variable | Default | Purpose |
//! |---|---|---|
//! | `JAM_HOME` | resolved per security-setup §7.1 | Root for journal output (`<JAM_HOME>/journal/`). |
//! | `NATS_URL` | `nats://127.0.0.1:4222` | Single-node JetStream URL. |
//! | `NATS_TOKEN` | unset | Auth token (production: source from `pass`). |
//! | `RUST_LOG` | `jam_nats_bridge=info` | Tracing filter. |
//!
//! ## Stream + consumer
//!
//! The bridge uses a durable pull consumer on the `journal` stream so it
//! resumes from its last-acknowledged offset on restart. Per spec §4.4.1.

#![deny(missing_docs)]

use chrono::{DateTime, Utc};
use futures::StreamExt;
use jam_journal::{JournalWriter, WriterConfig};
use jam_nats::JamNats;
use tracing::{error, info, warn};

#[derive(Debug, thiserror::Error)]
enum BridgeError {
    #[error("nats: {0}")]
    Nats(#[from] jam_nats::NatsError),

    #[error("journal: {0}")]
    Journal(#[from] jam_journal::JournalError),

    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("jetstream consumer: {0}")]
    Consumer(String),
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    if let Err(err) = run().await {
        error!("jam-nats-bridge fatal: {err}");
        return std::process::ExitCode::FAILURE;
    }
    std::process::ExitCode::SUCCESS
}

async fn run() -> Result<(), BridgeError> {
    init_tracing();

    let jam_home = jam_tools_core::paths::jam_home();
    let nats_url = std::env::var("NATS_URL").unwrap_or_else(|_| "nats://127.0.0.1:4222".into());
    let nats_token = std::env::var("NATS_TOKEN").ok();

    let journal_root = jam_home.join("journal");
    info!(
        jam_home = %jam_home.display(),
        journal_root = %journal_root.display(),
        nats_url = %nats_url,
        "bridge starting",
    );

    let journal = JournalWriter::new(
        WriterConfig::new(journal_root)
            .with_actor("jam-nats-bridge")
            .with_fsync(false),
    )?;

    let nats = JamNats::connect(&nats_url, nats_token).await?;
    info!("connected to NATS");

    // Idempotently ensure substrate JetStream resources exist (substrate
    // startup ordering means we may race process-compose's nats start).
    jam_nats::ensure_streams(nats.jetstream(), &jam_nats::default_streams()).await?;
    jam_nats::ensure_kv_buckets(nats.jetstream(), &jam_nats::default_kv_buckets()).await?;
    info!("substrate streams and KV buckets ensured");

    let stream = nats
        .jetstream()
        .get_stream("journal")
        .await
        .map_err(|e| BridgeError::Consumer(format!("get_stream: {e}")))?;

    let consumer = stream
        .get_or_create_consumer(
            "jam-nats-bridge",
            async_nats::jetstream::consumer::pull::Config {
                durable_name: Some("jam-nats-bridge".into()),
                description: Some("Forwards journal.* messages to JSONL on disk.".into()),
                filter_subject: "journal.>".into(),
                ..Default::default()
            },
        )
        .await
        .map_err(|e| BridgeError::Consumer(format!("get_or_create_consumer: {e}")))?;

    let mut messages = consumer
        .messages()
        .await
        .map_err(|e| BridgeError::Consumer(format!("consumer.messages: {e}")))?;

    info!("subscribed to journal.>; entering main loop");

    let shutdown = tokio::signal::ctrl_c();
    tokio::pin!(shutdown);

    loop {
        tokio::select! {
            _ = &mut shutdown => {
                info!("shutdown signal received");
                return Ok(());
            }
            msg = messages.next() => match msg {
                Some(Ok(message)) => {
                    if let Err(err) = handle_message(&message, &journal) {
                        error!(subject = %message.subject, "handle_message: {err}");
                    }
                    if let Err(err) = message.ack().await {
                        warn!(subject = %message.subject, "ack failed: {err}");
                    }
                }
                Some(Err(err)) => {
                    warn!("consumer error (will retry): {err}");
                }
                None => {
                    warn!("consumer stream closed");
                    return Ok(());
                }
            }
        }
    }
}

fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("jam_nats_bridge=info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

fn handle_message(
    message: &async_nats::jetstream::Message,
    journal: &JournalWriter,
) -> Result<(), BridgeError> {
    route_message(message.subject.as_str(), &message.payload, journal)
}

/// Pure routing logic — subject + payload + journal writer in, durable
/// JSONL line out. Extracted from the NATS dispatch loop so it can be
/// unit-tested without spinning up a server.
fn route_message(
    subject: &str,
    payload: &[u8],
    journal: &JournalWriter,
) -> Result<(), BridgeError> {
    let event_type = event_type_from_subject(subject);
    let body = std::str::from_utf8(payload)
        .map_err(|e| BridgeError::Consumer(format!("non-utf8 payload: {e}")))?;
    let timestamp = parse_envelope_timestamp(body).unwrap_or_else(Utc::now);

    journal.write_raw_line(body, event_type, timestamp)?;
    Ok(())
}

/// Strip the `journal.` prefix to derive the event type used for routing
/// to per-group JSONL files. Falls back to `"misc"` for malformed subjects
/// (the journal writer also treats this as the catch-all bucket).
fn event_type_from_subject(subject: &str) -> &str {
    subject.strip_prefix("journal.").unwrap_or("misc")
}

/// Best-effort parse of the envelope's `timestamp` field as RFC 3339.
///
/// Returns None if the body isn't valid JSON, the field is missing, or it
/// doesn't parse as RFC 3339. Caller falls back to `Utc::now()` — the
/// envelope's own timestamp is preserved in the JSON regardless; this is
/// only used to route the line to the right `<YYYY-MM-DD>/` directory.
fn parse_envelope_timestamp(body: &str) -> Option<DateTime<Utc>> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let raw = value.get("timestamp")?.as_str()?;
    DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_event_type_from_subject() {
        assert_eq!(
            event_type_from_subject("journal.picker.spawned"),
            "picker.spawned"
        );
        assert_eq!(
            event_type_from_subject("journal.pr.ci.status-changed"),
            "pr.ci.status-changed"
        );
        assert_eq!(
            event_type_from_subject("journal.task.requested"),
            "task.requested"
        );
    }

    #[test]
    fn malformed_subject_falls_back_to_misc() {
        assert_eq!(event_type_from_subject("not-a-journal-subject"), "misc");
        assert_eq!(event_type_from_subject(""), "misc");
    }

    #[test]
    fn parses_envelope_timestamp() {
        let body = r#"{"timestamp":"2026-05-04T12:34:56.789Z","event_type":"x","payload":{}}"#;
        let ts = parse_envelope_timestamp(body).unwrap();
        assert_eq!(
            ts.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            "2026-05-04T12:34:56.789Z"
        );
    }

    #[test]
    fn returns_none_for_malformed_payload() {
        assert!(parse_envelope_timestamp("not json").is_none());
        assert!(parse_envelope_timestamp(r#"{"no_ts":1}"#).is_none());
        assert!(parse_envelope_timestamp(r#"{"timestamp":"not-rfc3339"}"#).is_none());
    }

    #[test]
    fn route_message_lands_journal_line() {
        let tmp = tempfile::tempdir().unwrap();
        let journal =
            JournalWriter::new(WriterConfig::new(tmp.path()).with_actor("jam-nats-bridge"))
                .unwrap();

        let body = br#"{"schema_version":1,"event_type":"task.requested","event_subtype_version":1,"timestamp":"2026-05-04T12:00:00Z","journal_seq":42,"trace_id":"01HXKJ","actor":"jam-cli","payload":{"task_id":"t-1","description":"x","project":"blueberry","task_class":"light-edit","priority":"normal","requested_by":"human:caleb"}}"#;

        route_message("journal.task.requested", body, &journal).unwrap();

        let path = tmp.path().join("2026-05-04").join("journal.task.jsonl");
        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(contents.contains(r#""event_type":"task.requested""#));
        assert!(contents.contains(r#""trace_id":"01HXKJ""#));
        assert_eq!(contents.lines().count(), 1);
    }

    #[test]
    fn route_message_routes_to_correct_date_directory() {
        // Verify the timestamp from the envelope drives directory routing,
        // not Utc::now() (which would put the line in today's directory).
        let tmp = tempfile::tempdir().unwrap();
        let journal =
            JournalWriter::new(WriterConfig::new(tmp.path()).with_actor("jam-nats-bridge"))
                .unwrap();

        let body = br#"{"timestamp":"2025-12-31T23:59:59Z","event_type":"setup.completed","payload":{"checks_passed":24,"checks_total":24,"ts":"2025-12-31T23:59:59Z"}}"#;
        route_message("journal.setup.completed", body, &journal).unwrap();

        // 2025-12-31 directory should exist; today's should not.
        let old = tmp.path().join("2025-12-31").join("journal.setup.jsonl");
        assert!(old.exists(), "envelope timestamp drives date routing");
    }

    #[test]
    fn route_message_redacts_secrets() {
        let tmp = tempfile::tempdir().unwrap();
        let journal =
            JournalWriter::new(WriterConfig::new(tmp.path()).with_actor("jam-nats-bridge"))
                .unwrap();

        // A raw-line forward that includes a secret in the payload — the
        // bridge passes it to write_raw_line which runs the redactor.
        let body = br#"{"timestamp":"2026-05-04T12:00:00Z","event_type":"task.requested","payload":{"description":"leaked: ghp_AbCdEfGhIjKlMnOpQrStUvWxYz0123456789"}}"#;
        route_message("journal.task.requested", body, &journal).unwrap();

        let path = tmp.path().join("2026-05-04").join("journal.task.jsonl");
        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(!contents.contains("ghp_AbCd"), "secret should be redacted");
        assert!(contents.contains("<redacted-secret>"));
    }
}
