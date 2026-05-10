//! `jam-journal-reconciler` - JSONL/NATS journal to SQLite+FTS5 (§4.4.2).
//!
//! The orchestrator journal is sacred append-only state. This process builds a
//! derived SQLite session store from those events, either by replaying JSONL
//! files (`--rebuild`) or by subscribing to live traced `journal.>` messages.

#![deny(missing_docs)]

use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use clap::Parser;
use futures::StreamExt;
use jam_nats::async_nats;
use jam_nats::JamNats;
#[cfg(test)]
use rusqlite::OptionalExtension;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tracing::{debug, error, info, warn};

const SERVICE_NAME: &str = "jam-journal-reconciler";
const SERVICE_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Parser)]
#[command(name = SERVICE_NAME, version, about = "Replay journal events into session-store.db")]
struct Cli {
    /// Delete and rebuild the session store from JSONL before subscribing.
    #[arg(long)]
    rebuild: bool,

    /// Exit after schema setup/rebuild instead of subscribing to NATS.
    #[arg(long)]
    once: bool,
}

#[derive(Debug, thiserror::Error)]
enum ReconcilerError {
    #[error("nats: {0}")]
    Nats(#[from] jam_nats::NatsError),

    #[error("subscribe: {0}")]
    Subscribe(String),

    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("serde: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("{kind}: {detail}")]
    Protocol {
        kind: &'static str,
        detail: String,
        remediation: &'static str,
        tracked_by: &'static str,
    },
}

impl ReconcilerError {
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
struct Config {
    nats_url: String,
    nats_token: Option<String>,
    journal_root: PathBuf,
    db_path: PathBuf,
}

impl Config {
    fn from_env() -> Self {
        let jam_home = jam_tools_core::paths::jam_home();
        let journal_root = std::env::var_os("JAM_JOURNAL_ROOT")
            .map_or_else(|| jam_home.join("journal"), PathBuf::from);
        let db_path = std::env::var_os("JAM_SESSION_STORE_DB")
            .map_or_else(|| jam_home.join("session-store.db"), PathBuf::from);
        let nats_url = std::env::var("NATS_URL").unwrap_or_else(|_| "nats://127.0.0.1:4222".into());
        let nats_token = std::env::var("NATS_TOKEN").ok();
        Self {
            nats_url,
            nats_token,
            journal_root,
            db_path,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct JournalEnvelope {
    schema_version: u32,
    event_type: String,
    event_subtype_version: u32,
    timestamp: DateTime<Utc>,
    journal_seq: u64,
    trace_id: String,
    #[serde(default)]
    parent_trace_id: Option<String>,
    actor: String,
    payload: serde_json::Value,
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    if let Err(err) = run().await {
        error!("jam-journal-reconciler fatal: {err}");
        return std::process::ExitCode::FAILURE;
    }
    std::process::ExitCode::SUCCESS
}

async fn run() -> Result<(), ReconcilerError> {
    init_tracing();
    let cli = Cli::parse();
    let config = Config::from_env();

    info!(
        service = %SERVICE_NAME,
        version = %SERVICE_VERSION,
        db = %config.db_path.display(),
        journal_root = %config.journal_root.display(),
        nats = %config.nats_url,
        rebuild = cli.rebuild,
        once = cli.once,
        "starting",
    );

    if cli.rebuild && config.db_path.exists() {
        fs::remove_file(&config.db_path)?;
    }
    {
        let conn = open_db(&config.db_path)?;
        ensure_schema(&conn)?;
        if cli.rebuild {
            let count = replay_journal_dir(&conn, &config.journal_root)?;
            info!(count, "replayed journal JSONL");
        }
    }
    if cli.once {
        return Ok(());
    }

    subscribe_live(config).await
}

async fn subscribe_live(config: Config) -> Result<(), ReconcilerError> {
    let nats = JamNats::connect(&config.nats_url, config.nats_token.clone()).await?;
    info!("connected to NATS");
    let mut sub = nats
        .client()
        .subscribe("journal.>")
        .await
        .map_err(|e| ReconcilerError::Subscribe(e.to_string()))?;
    info!(subject = "journal.>", "subscribed");

    let shutdown = tokio::signal::ctrl_c();
    tokio::pin!(shutdown);
    loop {
        tokio::select! {
            _ = &mut shutdown => {
                info!("shutdown signal received");
                return Ok(());
            }
            msg = sub.next() => {
                let Some(message) = msg else {
                    warn!("subscriber stream closed");
                    return Ok(());
                };
                if let Err(err) = handle_message(&config.db_path, &message) {
                    warn!(subject = %message.subject, "ingest failed: {err}");
                }
            }
        }
    }
}

fn handle_message(db_path: &Path, msg: &async_nats::Message) -> Result<(), ReconcilerError> {
    if msg
        .headers
        .as_ref()
        .and_then(jam_nats::extract_trace_from_headers)
        .is_none()
    {
        return Err(ReconcilerError::protocol(
            "missing-trace",
            "live journal message arrived without Trace-Id headers",
            "Use traced publish wrappers for all journal events.",
            "principle-tracing-chains-end-to-end",
        ));
    }
    let envelope: JournalEnvelope = serde_json::from_slice(&msg.payload)?;
    let conn = open_db(db_path)?;
    ensure_schema(&conn)?;
    ingest_event(&conn, &envelope)?;
    Ok(())
}

fn open_db(path: &Path) -> Result<Connection, ReconcilerError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    Ok(conn)
}

fn ensure_schema(conn: &Connection) -> Result<(), ReconcilerError> {
    conn.execute_batch(
        r"
        CREATE TABLE IF NOT EXISTS sessions (
            id TEXT PRIMARY KEY,
            started_at TEXT NOT NULL,
            ended_at TEXT,
            actor TEXT NOT NULL,
            trace_id TEXT NOT NULL,
            metadata_json TEXT
        );

        CREATE TABLE IF NOT EXISTS messages (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id TEXT NOT NULL REFERENCES sessions(id),
            timestamp TEXT NOT NULL,
            role TEXT NOT NULL,
            content TEXT NOT NULL,
            metadata_json TEXT
        );

        CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts
        USING fts5(content, content='messages', content_rowid='id');

        CREATE TABLE IF NOT EXISTS tool_calls (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            message_id INTEGER NOT NULL REFERENCES messages(id),
            tool_name TEXT NOT NULL,
            arguments_json TEXT NOT NULL,
            result_json TEXT,
            duration_ms INTEGER
        );

        CREATE TABLE IF NOT EXISTS ingested_events (
            event_key TEXT PRIMARY KEY,
            event_type TEXT NOT NULL,
            trace_id TEXT NOT NULL,
            ingested_at TEXT NOT NULL
        );
        ",
    )?;
    Ok(())
}

fn replay_journal_dir(conn: &Connection, journal_root: &Path) -> Result<usize, ReconcilerError> {
    if !journal_root.exists() {
        return Ok(0);
    }
    if !journal_root.is_dir() {
        return Err(ReconcilerError::protocol(
            "invalid-journal-root",
            format!(
                "journal root is not a directory: {}",
                journal_root.display()
            ),
            "Set JAM_JOURNAL_ROOT to the orchestrator journal directory.",
            "comp-journal-reconciler",
        ));
    }
    let mut files = journal_files(journal_root)?;
    files.sort();
    let mut count = 0;
    for file in files {
        count += replay_file(conn, &file)?;
    }
    Ok(count)
}

fn journal_files(journal_root: &Path) -> Result<Vec<PathBuf>, ReconcilerError> {
    let mut files = Vec::new();
    for day in fs::read_dir(journal_root)? {
        let day = day?;
        let day_path = day.path();
        if !day_path.is_dir() {
            continue;
        }
        for entry in fs::read_dir(day_path)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "jsonl") {
                files.push(path);
            }
        }
    }
    Ok(files)
}

fn replay_file(conn: &Connection, path: &Path) -> Result<usize, ReconcilerError> {
    let file = File::open(path)?;
    let mut count = 0;
    for (index, line) in BufReader::new(file).lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let envelope: JournalEnvelope = serde_json::from_str(&line).map_err(|err| {
            ReconcilerError::protocol(
                "invalid-jsonl",
                format!(
                    "{}:{} is invalid journal JSON: {err}",
                    path.display(),
                    index + 1
                ),
                "Fix or remove the malformed journal line before rebuilding session-store.",
                "principle-failure-surfaces-immediately",
            )
        })?;
        if ingest_event(conn, &envelope)? {
            count += 1;
        }
    }
    Ok(count)
}

fn ingest_event(conn: &Connection, envelope: &JournalEnvelope) -> Result<bool, ReconcilerError> {
    if envelope.schema_version != 1 {
        return Err(ReconcilerError::protocol(
            "unsupported-envelope-version",
            format!(
                "unsupported envelope schema_version {}",
                envelope.schema_version
            ),
            "Upgrade jam-journal-reconciler before replaying this journal.",
            "comp-journal-reconciler",
        ));
    }
    if envelope.trace_id.trim().is_empty() {
        return Err(ReconcilerError::protocol(
            "missing-trace",
            format!("{} envelope has empty trace_id", envelope.event_type),
            "Fix the event publisher; trace propagation is required.",
            "principle-tracing-chains-end-to-end",
        ));
    }
    let event_key = event_key(envelope)?;
    let inserted = conn.execute(
        "INSERT OR IGNORE INTO ingested_events(event_key,event_type,trace_id,ingested_at)
         VALUES (?1,?2,?3,?4)",
        params![
            event_key,
            envelope.event_type,
            envelope.trace_id,
            Utc::now().to_rfc3339()
        ],
    )?;
    if inserted == 0 {
        debug!(event_type = %envelope.event_type, trace_id = %envelope.trace_id, "event already ingested");
        return Ok(false);
    }

    let session_id = session_id(envelope);
    let metadata_json = serde_json::to_string(envelope)?;
    conn.execute(
        "INSERT OR IGNORE INTO sessions(id,started_at,ended_at,actor,trace_id,metadata_json)
         VALUES (?1,?2,NULL,?3,?4,?5)",
        params![
            session_id,
            envelope.timestamp.to_rfc3339(),
            envelope.actor,
            envelope.trace_id,
            metadata_json
        ],
    )?;
    update_session_end(conn, &session_id, envelope)?;

    let content = event_content(envelope);
    conn.execute(
        "INSERT INTO messages(session_id,timestamp,role,content,metadata_json)
         VALUES (?1,?2,?3,?4,?5)",
        params![
            session_id,
            envelope.timestamp.to_rfc3339(),
            role_for(envelope),
            content,
            serde_json::to_string(&envelope.payload)?
        ],
    )?;
    let rowid = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO messages_fts(rowid, content) VALUES (?1, ?2)",
        params![rowid, content],
    )?;
    maybe_insert_tool_call(conn, rowid, envelope)?;
    Ok(true)
}

fn update_session_end(
    conn: &Connection,
    session_id: &str,
    envelope: &JournalEnvelope,
) -> Result<(), ReconcilerError> {
    if matches!(
        envelope.event_type.as_str(),
        "picker.exited"
            | "picker.killed"
            | "pr.merged"
            | "task.abandoned"
            | "maestro.session-ended"
    ) {
        conn.execute(
            "UPDATE sessions SET ended_at = ?1 WHERE id = ?2",
            params![envelope.timestamp.to_rfc3339(), session_id],
        )?;
    }
    Ok(())
}

fn maybe_insert_tool_call(
    conn: &Connection,
    message_id: i64,
    envelope: &JournalEnvelope,
) -> Result<(), ReconcilerError> {
    if envelope.event_type != "maestro.tool-call" {
        return Ok(());
    }
    let tool_name = envelope
        .payload
        .get("tool_name")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown-tool");
    let arguments_json = envelope
        .payload
        .get("arguments")
        .map_or_else(|| "{}".to_owned(), serde_json::Value::to_string);
    let result_json = envelope
        .payload
        .get("result")
        .map(serde_json::Value::to_string);
    let duration_ms = envelope
        .payload
        .get("duration_ms")
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| i64::try_from(value).ok());
    conn.execute(
        "INSERT INTO tool_calls(message_id,tool_name,arguments_json,result_json,duration_ms)
         VALUES (?1,?2,?3,?4,?5)",
        params![
            message_id,
            tool_name,
            arguments_json,
            result_json,
            duration_ms
        ],
    )?;
    Ok(())
}

fn session_id(envelope: &JournalEnvelope) -> String {
    for field in ["session_id", "picker_handle", "maestro_session_id"] {
        if let Some(value) = envelope
            .payload
            .get(field)
            .and_then(serde_json::Value::as_str)
        {
            return value.to_owned();
        }
    }
    format!("trace:{}", envelope.trace_id)
}

fn role_for(envelope: &JournalEnvelope) -> &'static str {
    if envelope.event_type.starts_with("maestro.") {
        "assistant"
    } else if envelope.actor.starts_with("human:") {
        "user"
    } else {
        "tool"
    }
}

fn event_content(envelope: &JournalEnvelope) -> String {
    format!(
        "{} actor={} trace={} subtype={} seq={} parent={} payload={}",
        envelope.event_type,
        envelope.actor,
        envelope.trace_id,
        envelope.event_subtype_version,
        envelope.journal_seq,
        envelope.parent_trace_id.as_deref().unwrap_or(""),
        envelope.payload
    )
}

fn event_key(envelope: &JournalEnvelope) -> Result<String, ReconcilerError> {
    let bytes = serde_json::to_vec(envelope)?;
    let digest = Sha256::digest(bytes);
    Ok(format!("{digest:x}"))
}

#[cfg(test)]
fn query_fts(conn: &Connection, query: &str) -> Result<Vec<String>, ReconcilerError> {
    let mut stmt = conn.prepare(
        "SELECT messages.content
         FROM messages_fts
         JOIN messages ON messages_fts.rowid = messages.id
         WHERE messages_fts MATCH ?1
         ORDER BY bm25(messages_fts)
         LIMIT 20",
    )?;
    let rows = stmt
        .query_map([query], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

#[cfg(test)]
fn count_messages(conn: &Connection) -> Result<u64, ReconcilerError> {
    let count = conn
        .query_row("SELECT COUNT(*) FROM messages", [], |row| {
            row.get::<_, u64>(0)
        })
        .optional()?
        .unwrap_or(0);
    Ok(count)
}

#[cfg(test)]
fn count_tool_calls(conn: &Connection) -> Result<u64, ReconcilerError> {
    let count = conn
        .query_row("SELECT COUNT(*) FROM tool_calls", [], |row| {
            row.get::<_, u64>(0)
        })
        .optional()?
        .unwrap_or(0);
    Ok(count)
}

fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("jam_journal_reconciler=info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn ingests_event_into_session_store_and_fts() {
        let tmp = TempDir::new().unwrap();
        let db = tmp.path().join("session-store.db");
        let conn = open_db(&db).unwrap();
        ensure_schema(&conn).unwrap();
        let envelope = envelope(
            "picker.spawned",
            serde_json::json!({
                "task_id": "task-1",
                "session_id": "codex-cli:abc",
                "worktree_path": "/tmp/task-1"
            }),
        );

        assert!(ingest_event(&conn, &envelope).unwrap());
        assert!(!ingest_event(&conn, &envelope).unwrap());

        assert_eq!(count_messages(&conn).unwrap(), 1);
        let rows = query_fts(&conn, "worktree_path").unwrap();
        assert_eq!(rows.len(), 1);
        assert!(rows[0].contains("picker.spawned"));
    }

    #[test]
    fn rebuild_replays_jsonl_files() {
        let tmp = TempDir::new().unwrap();
        let journal_day = tmp.path().join("journal").join("2026-05-06");
        fs::create_dir_all(&journal_day).unwrap();
        let line = serde_json::to_string(&envelope(
            "pr.opened",
            serde_json::json!({
                "task_id": "task-1",
                "pr_ref": "cleak/blueberry#42",
                "title": "Test PR"
            }),
        ))
        .unwrap();
        fs::write(journal_day.join("journal.pr.jsonl"), format!("{line}\n")).unwrap();
        let db = tmp.path().join("session-store.db");
        let conn = open_db(&db).unwrap();
        ensure_schema(&conn).unwrap();

        let count = replay_journal_dir(&conn, &tmp.path().join("journal")).unwrap();

        assert_eq!(count, 1);
        assert_eq!(count_messages(&conn).unwrap(), 1);
        assert_eq!(query_fts(&conn, "blueberry").unwrap().len(), 1);
    }

    #[test]
    fn maestro_tool_call_populates_tool_calls_table() {
        let tmp = TempDir::new().unwrap();
        let db = tmp.path().join("session-store.db");
        let conn = open_db(&db).unwrap();
        ensure_schema(&conn).unwrap();
        let envelope = envelope(
            "maestro.tool-call",
            serde_json::json!({
                "session_id": "maestro-session-1",
                "tool_name": "world-snapshot",
                "duration_ms": 123,
                "success": true,
                "ts": "2026-05-06T05:30:00Z"
            }),
        );

        assert!(ingest_event(&conn, &envelope).unwrap());

        assert_eq!(count_messages(&conn).unwrap(), 1);
        assert_eq!(count_tool_calls(&conn).unwrap(), 1);
    }

    fn envelope(event_type: &str, payload: serde_json::Value) -> JournalEnvelope {
        JournalEnvelope {
            schema_version: 1,
            event_type: event_type.into(),
            event_subtype_version: 1,
            timestamp: DateTime::parse_from_rfc3339("2026-05-06T05:30:00Z")
                .unwrap()
                .with_timezone(&Utc),
            journal_seq: 1,
            trace_id: "01HXKJVF7P4N6X5R8SRZWB6JCM".into(),
            parent_trace_id: None,
            actor: "test".into(),
            payload,
        }
    }
}
