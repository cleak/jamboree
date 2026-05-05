//! The journal writer itself — append, rotate, redact.

use chrono::{DateTime, NaiveDate, Utc};
use jam_events::{Event, EventEnvelope};
use jam_trace::TraceId;
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use crate::redact::Redactor;

/// Failure modes for journal operations.
#[derive(Debug, thiserror::Error)]
pub enum JournalError {
    /// Failed to create or append to a journal file.
    #[error("io error on {path}: {source}")]
    Io {
        /// Path being acted on when the error occurred.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// Failed to serialize the event envelope.
    #[error("serde error: {0}")]
    Serde(#[from] serde_json::Error),
}

impl JournalError {
    fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}

/// Configuration for [`JournalWriter`].
#[derive(Debug, Clone)]
pub struct WriterConfig {
    /// Root directory under which date-bucketed journal files live.
    /// Conventionally `/home/maestro/.jam/journal/`.
    pub root_dir: PathBuf,

    /// Service or actor name written into every envelope's `actor` field.
    /// E.g. `"jam-svc-session"`, `"jam-cli"`, `"human:caleb"`.
    pub actor: String,

    /// Initial value for the monotonic `journal_seq` counter. Set this to the
    /// last sequence number observed at startup so a restarted writer
    /// resumes without collisions.
    pub starting_seq: u64,

    /// Whether to fsync after every write. Defaults to false; set true in
    /// production to survive power loss.
    pub fsync_each_write: bool,
}

impl WriterConfig {
    /// Construct config with defaults: empty actor (must be set), seq=0, no fsync.
    #[must_use]
    pub fn new(root_dir: impl Into<PathBuf>) -> Self {
        Self {
            root_dir: root_dir.into(),
            actor: String::new(),
            starting_seq: 0,
            fsync_each_write: false,
        }
    }

    /// Set the actor name for the envelope's `actor` field.
    #[must_use]
    pub fn with_actor(mut self, actor: impl Into<String>) -> Self {
        self.actor = actor.into();
        self
    }

    /// Set the starting sequence value (e.g. when resuming after restart).
    #[must_use]
    pub const fn with_starting_seq(mut self, seq: u64) -> Self {
        self.starting_seq = seq;
        self
    }

    /// Enable fsync after every write. Costs latency; gains durability.
    #[must_use]
    pub const fn with_fsync(mut self, on: bool) -> Self {
        self.fsync_each_write = on;
        self
    }
}

/// The journal writer — the only component that opens journal files for
/// writing. Process-singleton; cross-process writes to the same file are
/// not supported.
pub struct JournalWriter {
    config: WriterConfig,
    redactor: Redactor,
    sequence: AtomicU64,
    files: Mutex<HashMap<FileKey, File>>,
}

#[derive(Hash, Eq, PartialEq, Clone)]
struct FileKey {
    date: NaiveDate,
    group: String,
}

impl JournalWriter {
    /// Construct with the given config and the default redactor.
    pub fn new(config: WriterConfig) -> Result<Self, JournalError> {
        Self::with_redactor(config, Redactor::with_default_patterns())
    }

    /// Construct with a custom redactor (e.g. for tests or to add project-specific patterns).
    pub fn with_redactor(config: WriterConfig, redactor: Redactor) -> Result<Self, JournalError> {
        std::fs::create_dir_all(&config.root_dir)
            .map_err(|e| JournalError::io(&config.root_dir, e))?;
        let starting_seq = config.starting_seq;
        Ok(Self {
            config,
            redactor,
            sequence: AtomicU64::new(starting_seq),
            files: Mutex::new(HashMap::new()),
        })
    }

    /// Write a typed event payload to the journal.
    ///
    /// Returns the assigned `journal_seq`. Caller can use this for
    /// downstream correlation (e.g. when a reconciler needs to know which
    /// envelope it just produced).
    ///
    /// Per spec §4.4.2: timestamp is `Utc::now()` at write time; routing
    /// to per-group files happens by first segment of `EVENT_TYPE`
    /// (e.g. `picker.spawned` -> `journal.picker.jsonl`).
    pub fn write<P: Event>(
        &self,
        payload: P,
        trace_id: TraceId,
        parent_trace_id: Option<TraceId>,
    ) -> Result<u64, JournalError> {
        let seq = self.sequence.fetch_add(1, Ordering::SeqCst);
        let now = Utc::now();
        let actor = self.config.actor.clone();

        let envelope = EventEnvelope::new(
            P::EVENT_TYPE,
            P::EVENT_SUBTYPE_VERSION,
            seq,
            trace_id.to_string(),
            actor,
            payload,
        );
        let envelope = match parent_trace_id {
            Some(parent) => envelope.with_parent_trace(parent.to_string()),
            None => envelope,
        };

        let serialized = serde_json::to_string(&envelope)?;
        let redacted = self.redactor.redact(&serialized);

        self.append_line(&redacted, P::EVENT_TYPE, now.date_naive())?;
        Ok(seq)
    }

    /// Lower-level write that takes an already-rendered envelope JSON line.
    ///
    /// Used by the future `jam-journal-writer` daemon when it forwards events
    /// from NATS subscriptions: it already has the JSON, doesn't need to
    /// re-serialize. The redactor still runs.
    pub fn write_raw_line(
        &self,
        line: &str,
        event_type: &str,
        timestamp: DateTime<Utc>,
    ) -> Result<(), JournalError> {
        let redacted = self.redactor.redact(line);
        self.append_line(&redacted, event_type, timestamp.date_naive())
    }

    fn append_line(
        &self,
        line: &str,
        event_type: &str,
        date: NaiveDate,
    ) -> Result<(), JournalError> {
        let group = group_from_event_type(event_type);
        let key = FileKey {
            date,
            group: group.to_string(),
        };

        let path = self.path_for(&key);
        let mut files = self.files.lock().expect("poisoned");

        if !files.contains_key(&key) {
            let dir = path
                .parent()
                .expect("journal path always has a parent")
                .to_path_buf();
            std::fs::create_dir_all(&dir).map_err(|e| JournalError::io(&dir, e))?;
            let f = OpenOptions::new()
                .append(true)
                .create(true)
                .open(&path)
                .map_err(|e| JournalError::io(&path, e))?;
            files.insert(key.clone(), f);
        }
        let file = files.get_mut(&key).expect("just inserted");

        // Single write_all + newline. The OS guarantees atomic appends for
        // writes up to PIPE_BUF (typically 4096); journal lines are well
        // under that.
        file.write_all(line.as_bytes())
            .map_err(|e| JournalError::io(&path, e))?;
        file.write_all(b"\n")
            .map_err(|e| JournalError::io(&path, e))?;

        if self.config.fsync_each_write {
            file.sync_data().map_err(|e| JournalError::io(&path, e))?;
        }
        Ok(())
    }

    fn path_for(&self, key: &FileKey) -> PathBuf {
        let date_str = key.date.format("%Y-%m-%d").to_string();
        let mut p = self.config.root_dir.clone();
        p.push(date_str);
        p.push(format!("journal.{}.jsonl", key.group));
        p
    }

    /// Returns the current sequence value (next-write will use this).
    pub fn current_seq(&self) -> u64 {
        self.sequence.load(Ordering::SeqCst)
    }
}

/// Map an event type to its journal file group.
///
/// The group is the first dot-separated segment of the event type:
/// `picker.spawned` -> `picker`, `pr.ci.status-changed` -> `pr`,
/// `setup.completed` -> `setup`. Empty event types fall back to `misc`.
fn group_from_event_type(event_type: &str) -> &str {
    event_type
        .split('.')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or("misc")
}

#[cfg(test)]
mod tests {
    use super::*;
    use jam_events::generated::{PickerSpawned, TaskRequested};
    use std::io::Read;

    fn read_file(path: &std::path::Path) -> String {
        let mut s = String::new();
        File::open(path).unwrap().read_to_string(&mut s).unwrap();
        s
    }

    #[test]
    fn group_extraction_handles_known_event_types() {
        assert_eq!(group_from_event_type("picker.spawned"), "picker");
        assert_eq!(group_from_event_type("task.requested"), "task");
        assert_eq!(group_from_event_type("pr.ci.status-changed"), "pr");
        assert_eq!(group_from_event_type("setup.completed"), "setup");
        assert_eq!(
            group_from_event_type("maestro.budget.daily-exceeded"),
            "maestro"
        );
    }

    #[test]
    fn group_extraction_falls_back_for_empty() {
        assert_eq!(group_from_event_type(""), "misc");
        assert_eq!(group_from_event_type(".no-prefix"), "misc");
    }

    #[test]
    fn writes_event_to_per_group_file() {
        let tmp = tempfile::tempdir().unwrap();
        let writer =
            JournalWriter::new(WriterConfig::new(tmp.path()).with_actor("jam-svc-test")).unwrap();

        let payload = TaskRequested {
            task_id: "t-1".into(),
            description: "x".into(),
            project: "blueberry".into(),
            task_class: "light-edit".into(),
            priority: "normal".into(),
            requested_by: "human:caleb".into(),
        };

        let seq = writer.write(payload, TraceId::new(), None).unwrap();
        assert_eq!(seq, 0);

        let date = Utc::now().date_naive();
        let path = tmp
            .path()
            .join(date.format("%Y-%m-%d").to_string())
            .join("journal.task.jsonl");
        let contents = read_file(&path);
        assert!(contents.contains(r#""event_type":"task.requested""#));
        assert!(contents.contains(r#""actor":"jam-svc-test""#));
        assert!(contents.contains(r#""project":"blueberry""#));
        assert!(contents.ends_with('\n'));
    }

    #[test]
    fn assigns_monotonic_sequences() {
        let tmp = tempfile::tempdir().unwrap();
        let writer = JournalWriter::new(WriterConfig::new(tmp.path()).with_actor("test")).unwrap();

        let make = |id: &str| TaskRequested {
            task_id: id.into(),
            description: String::new(),
            project: "blueberry".into(),
            task_class: "light-edit".into(),
            priority: "normal".into(),
            requested_by: "human:caleb".into(),
        };

        let s0 = writer.write(make("a"), TraceId::new(), None).unwrap();
        let s1 = writer.write(make("b"), TraceId::new(), None).unwrap();
        let s2 = writer.write(make("c"), TraceId::new(), None).unwrap();
        assert_eq!(s0, 0);
        assert_eq!(s1, 1);
        assert_eq!(s2, 2);
    }

    #[test]
    fn resumes_from_starting_seq() {
        let tmp = tempfile::tempdir().unwrap();
        let writer = JournalWriter::new(
            WriterConfig::new(tmp.path())
                .with_actor("test")
                .with_starting_seq(48291),
        )
        .unwrap();

        let payload = TaskRequested {
            task_id: "t".into(),
            description: String::new(),
            project: "blueberry".into(),
            task_class: "light-edit".into(),
            priority: "normal".into(),
            requested_by: "human:caleb".into(),
        };
        let seq = writer.write(payload, TraceId::new(), None).unwrap();
        assert_eq!(seq, 48291);
    }

    #[test]
    fn routes_picker_event_to_picker_group() {
        let tmp = tempfile::tempdir().unwrap();
        let writer = JournalWriter::new(WriterConfig::new(tmp.path()).with_actor("test")).unwrap();

        let payload = PickerSpawned {
            task_id: "t".into(),
            harness: "codex-cli".into(),
            session_id: "s".into(),
            worktree_path: "/home/picker/workers/t".into(),
            spawned_at: Utc::now(),
            picker_pid: Some(1234),
            picker_trace_id: TraceId::new(),
            maestro_trace_id: TraceId::new(),
            sandbox_backend: "local".into(),
            sandbox_profile: "default".into(),
            task_class: "compile-heavy-rust".into(),
        };

        writer.write(payload, TraceId::new(), None).unwrap();

        let date = Utc::now().date_naive();
        let picker_path = tmp
            .path()
            .join(date.format("%Y-%m-%d").to_string())
            .join("journal.picker.jsonl");
        let task_path = tmp
            .path()
            .join(date.format("%Y-%m-%d").to_string())
            .join("journal.task.jsonl");
        assert!(picker_path.exists());
        assert!(!task_path.exists());
    }

    #[test]
    fn redacts_secrets_at_write_time() {
        let tmp = tempfile::tempdir().unwrap();
        let writer = JournalWriter::new(WriterConfig::new(tmp.path()).with_actor("test")).unwrap();

        // A malformed payload that includes a secret in a string field —
        // services should never do this, but if they do, it gets redacted.
        let payload = TaskRequested {
            task_id: "t".into(),
            description: "leaked: ghp_AbCdEfGhIjKlMnOpQrStUvWxYz0123456789".into(),
            project: "blueberry".into(),
            task_class: "light-edit".into(),
            priority: "normal".into(),
            requested_by: "human:caleb".into(),
        };
        writer.write(payload, TraceId::new(), None).unwrap();

        let date = Utc::now().date_naive();
        let path = tmp
            .path()
            .join(date.format("%Y-%m-%d").to_string())
            .join("journal.task.jsonl");
        let contents = read_file(&path);
        assert!(!contents.contains("ghp_AbCdEf"));
        assert!(contents.contains("<redacted-secret>"));
    }

    #[test]
    fn child_trace_serializes_parent() {
        let tmp = tempfile::tempdir().unwrap();
        let writer = JournalWriter::new(WriterConfig::new(tmp.path()).with_actor("test")).unwrap();

        let payload = TaskRequested {
            task_id: "t".into(),
            description: String::new(),
            project: "blueberry".into(),
            task_class: "light-edit".into(),
            priority: "normal".into(),
            requested_by: "human:caleb".into(),
        };
        let parent = TraceId::new();
        writer.write(payload, TraceId::new(), Some(parent)).unwrap();

        let date = Utc::now().date_naive();
        let path = tmp
            .path()
            .join(date.format("%Y-%m-%d").to_string())
            .join("journal.task.jsonl");
        let contents = read_file(&path);
        assert!(contents.contains(r#""parent_trace_id":""#));
        assert!(contents.contains(&parent.to_string()));
    }

    #[test]
    fn root_trace_omits_parent_field() {
        let tmp = tempfile::tempdir().unwrap();
        let writer = JournalWriter::new(WriterConfig::new(tmp.path()).with_actor("test")).unwrap();

        let payload = TaskRequested {
            task_id: "t".into(),
            description: String::new(),
            project: "blueberry".into(),
            task_class: "light-edit".into(),
            priority: "normal".into(),
            requested_by: "human:caleb".into(),
        };
        writer.write(payload, TraceId::new(), None).unwrap();

        let date = Utc::now().date_naive();
        let path = tmp
            .path()
            .join(date.format("%Y-%m-%d").to_string())
            .join("journal.task.jsonl");
        let contents = read_file(&path);
        assert!(!contents.contains("parent_trace_id"));
    }

    #[test]
    fn write_raw_line_redacts_too() {
        let tmp = tempfile::tempdir().unwrap();
        let writer = JournalWriter::new(WriterConfig::new(tmp.path()).with_actor("test")).unwrap();
        let line = r#"{"event_type":"task.requested","payload":{"token":"ghp_AbCdEfGhIjKlMnOpQrStUvWxYz0123456789"}}"#;
        writer
            .write_raw_line(line, "task.requested", Utc::now())
            .unwrap();

        let date = Utc::now().date_naive();
        let path = tmp
            .path()
            .join(date.format("%Y-%m-%d").to_string())
            .join("journal.task.jsonl");
        let contents = read_file(&path);
        assert!(!contents.contains("ghp_AbCd"));
    }

    #[test]
    fn concurrent_writes_serialize_correctly() {
        let tmp = tempfile::tempdir().unwrap();
        let writer = std::sync::Arc::new(
            JournalWriter::new(WriterConfig::new(tmp.path()).with_actor("test")).unwrap(),
        );

        let mut handles = vec![];
        for i in 0..10 {
            let writer = writer.clone();
            handles.push(std::thread::spawn(move || {
                let payload = TaskRequested {
                    task_id: format!("t-{i}"),
                    description: String::new(),
                    project: "blueberry".into(),
                    task_class: "light-edit".into(),
                    priority: "normal".into(),
                    requested_by: "human:caleb".into(),
                };
                writer.write(payload, TraceId::new(), None).unwrap()
            }));
        }
        let mut seqs: Vec<u64> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        seqs.sort_unstable();
        assert_eq!(seqs, (0..10).collect::<Vec<_>>());

        // All ten lines landed in the same file.
        let date = Utc::now().date_naive();
        let path = tmp
            .path()
            .join(date.format("%Y-%m-%d").to_string())
            .join("journal.task.jsonl");
        let contents = read_file(&path);
        let line_count = contents.lines().count();
        assert_eq!(line_count, 10);
    }
}
