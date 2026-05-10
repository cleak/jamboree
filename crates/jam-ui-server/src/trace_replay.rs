//! Trace replay over the durable JSONL journal.
//!
//! This backs the UI trace view with the same source of truth as
//! `jam trace replay`: rotated `journal.<group>.jsonl` files under
//! `$JAM_HOME/journal`.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, NaiveDate, Utc};
use jam_trace::TraceId;
use serde::{Deserialize, Serialize};

/// Complete replay for a requested trace.
#[derive(Debug, Clone, Serialize)]
pub struct TraceReplay {
    /// Trace ID requested by the caller.
    pub requested_trace_id: String,
    /// Maximum parent-trace depth used while walking backwards.
    pub max_depth: u32,
    /// Trace chain from requested child trace back toward the root trace.
    pub chain: Vec<String>,
    /// Chronological entries whose trace is in `chain`.
    pub entries: Vec<TraceReplayEntry>,
}

/// One durable journal entry included in a replay.
#[derive(Debug, Clone, Serialize)]
pub struct TraceReplayEntry {
    /// Journal file that contained this entry.
    pub path: PathBuf,
    /// One-based line number inside `path`.
    pub line_number: usize,
    /// Envelope event type, e.g. `picker.spawned`.
    pub event_type: String,
    /// Envelope timestamp.
    pub timestamp: DateTime<Utc>,
    /// Monotonic journal sequence assigned by the writer.
    pub journal_seq: u64,
    /// Entry trace ID.
    pub trace_id: String,
    /// Parent trace ID, when this entry is part of a child trace.
    pub parent_trace_id: Option<String>,
    /// Actor that wrote the journal envelope.
    pub actor: String,
    /// Raw event payload.
    pub payload: serde_json::Value,
}

/// Result set for `find-traces(filter)`.
#[derive(Debug, Clone, Serialize)]
pub struct TraceFindResult {
    /// Original filter string.
    pub filter: String,
    /// Maximum number of matches requested by the caller.
    pub limit: usize,
    /// Matching traces sorted by newest activity first.
    pub matches: Vec<TraceFindMatch>,
}

/// One trace summary returned by `find-traces(filter)`.
#[derive(Debug, Clone, Serialize)]
pub struct TraceFindMatch {
    /// Trace ID that matched.
    pub trace_id: String,
    /// First parent trace ID observed for this trace, when present.
    pub parent_trace_id: Option<String>,
    /// Earliest journal timestamp for this trace.
    pub first_seen: DateTime<Utc>,
    /// Latest journal timestamp for this trace.
    pub last_seen: DateTime<Utc>,
    /// Number of journal entries with this trace ID.
    pub event_count: usize,
    /// Unique event types observed for this trace.
    pub events: Vec<String>,
    /// Unique actors observed for this trace.
    pub actors: Vec<String>,
    /// Unique `task_id` payload values observed for this trace.
    pub task_ids: Vec<String>,
    /// Unique `session_id` payload values observed for this trace.
    pub session_ids: Vec<String>,
    /// Unique `pr_ref` payload values observed for this trace.
    pub pr_refs: Vec<String>,
    /// Unique harness payload values observed for this trace.
    pub harnesses: Vec<String>,
    /// Unique outcome payload values observed for this trace.
    pub outcomes: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct GenericJournalEnvelope {
    event_type: String,
    timestamp: DateTime<Utc>,
    journal_seq: u64,
    trace_id: String,
    parent_trace_id: Option<String>,
    actor: String,
    payload: serde_json::Value,
}

/// Errors that can occur while replaying traces from the journal.
#[derive(Debug, thiserror::Error)]
pub enum TraceReplayError {
    /// The requested trace ID is not a valid ULID trace ID.
    #[error("invalid trace id {trace_id}: {detail}")]
    InvalidTraceId {
        /// Requested trace ID.
        trace_id: String,
        /// Parser detail.
        detail: String,
    },

    /// The journal root directory does not exist.
    #[error("journal root does not exist: {path}")]
    JournalRootMissing {
        /// Missing journal root.
        path: PathBuf,
    },

    /// Reading a journal directory failed.
    #[error("read {path}: {source}")]
    ReadDir {
        /// Directory being read.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// Opening a journal file failed.
    #[error("open {path}: {source}")]
    Open {
        /// Journal file path.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// Reading a journal line failed.
    #[error("read {path} line {line_number}: {source}")]
    ReadLine {
        /// Journal file path.
        path: PathBuf,
        /// One-based line number.
        line_number: usize,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// Parsing a journal line failed.
    #[error("parse {path} line {line_number}: {source}")]
    ParseLine {
        /// Journal file path.
        path: PathBuf,
        /// One-based line number.
        line_number: usize,
        /// JSON parse error.
        #[source]
        source: serde_json::Error,
    },

    /// No journal entries exist for the requested trace chain.
    #[error("no journal entries found for trace {trace_id} in {journal_root}")]
    NoEntries {
        /// Requested trace ID.
        trace_id: String,
        /// Journal root searched.
        journal_root: PathBuf,
    },
}

/// Errors that can occur while searching traces from the journal.
#[derive(Debug, thiserror::Error)]
pub enum TraceFindError {
    /// The supplied limit was zero.
    #[error("trace find limit must be greater than zero")]
    InvalidLimit {
        /// Rejected limit value.
        limit: usize,
    },

    /// The supplied filter could not be parsed.
    #[error("invalid trace filter {filter:?}: {detail}")]
    InvalidFilter {
        /// Original filter string.
        filter: String,
        /// Parser detail.
        detail: String,
    },

    /// Reading or parsing the journal failed.
    #[error("{source}")]
    Journal {
        /// Underlying trace replay journal error.
        #[source]
        source: TraceReplayError,
    },
}

/// Reconstruct a trace chain from rotated JSONL journal files.
///
/// Per `principle-tracing-chains-end-to-end`, the selected entries include the
/// requested trace and parent traces discovered via `parent_trace_id`, up to
/// `max_depth`.
pub fn trace_replay_from_journal(
    journal_root: &Path,
    trace_id: &str,
    max_depth: u32,
) -> Result<TraceReplay, TraceReplayError> {
    trace_id
        .parse::<TraceId>()
        .map_err(|err| TraceReplayError::InvalidTraceId {
            trace_id: trace_id.to_owned(),
            detail: err.to_string(),
        })?;

    let mut all_entries = read_trace_journal_entries(journal_root)?;
    let chain = trace_parent_chain(&all_entries, trace_id, max_depth);
    let selected: HashSet<&str> = chain.iter().map(String::as_str).collect();
    all_entries.retain(|entry| selected.contains(entry.trace_id.as_str()));
    all_entries.sort_by(|left, right| {
        left.timestamp
            .cmp(&right.timestamp)
            .then(left.journal_seq.cmp(&right.journal_seq))
            .then(left.path.cmp(&right.path))
            .then(left.line_number.cmp(&right.line_number))
    });

    if all_entries.is_empty() {
        return Err(TraceReplayError::NoEntries {
            trace_id: trace_id.to_owned(),
            journal_root: journal_root.to_path_buf(),
        });
    }

    Ok(TraceReplay {
        requested_trace_id: trace_id.into(),
        max_depth,
        chain,
        entries: all_entries,
    })
}

/// Find traces matching a simple `AND` filter over journal envelope fields.
///
/// Supported terms are `key=value` pairs joined by `AND`. Terms match the trace
/// as a whole, so `event=picker.spawned AND task_id=foo` may be satisfied by
/// different journal entries with the same trace ID. `since=last-Nd`,
/// `since=last-Nh`, `since=last-Nm`, RFC 3339 timestamps, and `YYYY-MM-DD`
/// dates are supported.
pub fn find_traces_in_journal(
    journal_root: &Path,
    filter: &str,
    limit: usize,
) -> Result<TraceFindResult, TraceFindError> {
    if limit == 0 {
        return Err(TraceFindError::InvalidLimit { limit });
    }
    let parsed = TraceFilter::parse(filter, Utc::now())?;
    let entries = read_trace_journal_entries(journal_root)
        .map_err(|source| TraceFindError::Journal { source })?;

    let mut traces = HashMap::<String, TraceSummaryBuilder>::new();
    for entry in entries {
        traces
            .entry(entry.trace_id.clone())
            .or_insert_with(|| TraceSummaryBuilder::new(&entry.trace_id))
            .add_entry(&entry);
    }

    let mut matches = traces
        .into_values()
        .filter(|summary| parsed.matches(summary))
        .map(TraceSummaryBuilder::finish)
        .collect::<Vec<_>>();
    matches.sort_by(|left, right| {
        right
            .last_seen
            .cmp(&left.last_seen)
            .then(left.trace_id.cmp(&right.trace_id))
    });
    matches.truncate(limit);

    Ok(TraceFindResult {
        filter: filter.to_owned(),
        limit,
        matches,
    })
}

fn trace_parent_chain(entries: &[TraceReplayEntry], trace_id: &str, max_depth: u32) -> Vec<String> {
    let mut parent_by_trace = HashMap::new();
    for entry in entries {
        if let Some(parent) = &entry.parent_trace_id {
            parent_by_trace
                .entry(entry.trace_id.as_str())
                .or_insert(parent.as_str());
        }
    }

    let mut chain = vec![trace_id.to_owned()];
    let mut current = trace_id;
    for _ in 0..max_depth {
        let Some(parent) = parent_by_trace.get(current) else {
            break;
        };
        if chain.iter().any(|seen| seen == parent) {
            break;
        }
        chain.push((*parent).to_owned());
        current = parent;
    }
    chain
}

fn read_trace_journal_entries(
    journal_root: &Path,
) -> Result<Vec<TraceReplayEntry>, TraceReplayError> {
    let mut entries = Vec::new();
    for path in journal_jsonl_paths(journal_root)? {
        let file = File::open(&path).map_err(|source| TraceReplayError::Open {
            path: path.clone(),
            source,
        })?;
        for (idx, line) in BufReader::new(file).lines().enumerate() {
            let line_number = idx + 1;
            let line = line.map_err(|source| TraceReplayError::ReadLine {
                path: path.clone(),
                line_number,
                source,
            })?;
            let envelope =
                serde_json::from_str::<GenericJournalEnvelope>(&line).map_err(|source| {
                    TraceReplayError::ParseLine {
                        path: path.clone(),
                        line_number,
                        source,
                    }
                })?;
            entries.push(TraceReplayEntry {
                path: path.clone(),
                line_number,
                event_type: envelope.event_type,
                timestamp: envelope.timestamp,
                journal_seq: envelope.journal_seq,
                trace_id: envelope.trace_id,
                parent_trace_id: envelope.parent_trace_id,
                actor: envelope.actor,
                payload: envelope.payload,
            });
        }
    }
    Ok(entries)
}

fn journal_jsonl_paths(root: &Path) -> Result<Vec<PathBuf>, TraceReplayError> {
    if !root.exists() {
        return Err(TraceReplayError::JournalRootMissing {
            path: root.to_path_buf(),
        });
    }

    let mut paths = Vec::new();
    let entries = fs::read_dir(root).map_err(|source| TraceReplayError::ReadDir {
        path: root.to_path_buf(),
        source,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| TraceReplayError::ReadDir {
            path: root.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let files = fs::read_dir(&path).map_err(|source| TraceReplayError::ReadDir {
            path: path.clone(),
            source,
        })?;
        for file in files {
            let file = file.map_err(|source| TraceReplayError::ReadDir {
                path: path.clone(),
                source,
            })?;
            let candidate = file.path();
            if candidate.extension().and_then(|ext| ext.to_str()) == Some("jsonl") {
                paths.push(candidate);
            }
        }
    }
    paths.sort();
    Ok(paths)
}

#[derive(Debug)]
struct TraceFilter {
    terms: Vec<TraceFilterTerm>,
}

impl TraceFilter {
    fn parse(filter: &str, now: DateTime<Utc>) -> Result<Self, TraceFindError> {
        let raw_terms = split_filter_terms(filter)?;
        let mut terms = Vec::with_capacity(raw_terms.len());
        for raw in raw_terms {
            let (key, value) =
                raw.split_once('=')
                    .ok_or_else(|| TraceFindError::InvalidFilter {
                        filter: filter.to_owned(),
                        detail: format!("term {raw:?} must be key=value"),
                    })?;
            let key =
                normalize_filter_key(key.trim()).ok_or_else(|| TraceFindError::InvalidFilter {
                    filter: filter.to_owned(),
                    detail: format!("invalid field name {:?}", key.trim()),
                })?;
            let value = value.trim();
            if value.is_empty() {
                return Err(TraceFindError::InvalidFilter {
                    filter: filter.to_owned(),
                    detail: format!("field {key} has an empty value"),
                });
            }
            if key == "since" {
                terms.push(TraceFilterTerm::Since(parse_since(value, now).map_err(
                    |detail| TraceFindError::InvalidFilter {
                        filter: filter.to_owned(),
                        detail,
                    },
                )?));
            } else {
                terms.push(TraceFilterTerm::Field {
                    key,
                    value: value.to_owned(),
                });
            }
        }
        Ok(Self { terms })
    }

    fn matches(&self, summary: &TraceSummaryBuilder) -> bool {
        self.terms.iter().all(|term| match term {
            TraceFilterTerm::Since(cutoff) => summary.last_seen >= *cutoff,
            TraceFilterTerm::Field { key, value } => summary
                .indexed_values
                .get(key)
                .is_some_and(|values| values.contains(value)),
        })
    }
}

#[derive(Debug)]
enum TraceFilterTerm {
    Field { key: String, value: String },
    Since(DateTime<Utc>),
}

#[derive(Debug)]
struct TraceSummaryBuilder {
    trace_id: String,
    parent_trace_ids: BTreeSet<String>,
    first_seen: DateTime<Utc>,
    last_seen: DateTime<Utc>,
    event_count: usize,
    events: BTreeSet<String>,
    actors: BTreeSet<String>,
    indexed_values: BTreeMap<String, BTreeSet<String>>,
}

impl TraceSummaryBuilder {
    fn new(trace_id: &str) -> Self {
        let epoch = DateTime::<Utc>::UNIX_EPOCH;
        let mut indexed_values = BTreeMap::new();
        indexed_values
            .entry("trace_id".to_owned())
            .or_insert_with(BTreeSet::new)
            .insert(trace_id.to_owned());
        indexed_values
            .entry("trace".to_owned())
            .or_insert_with(BTreeSet::new)
            .insert(trace_id.to_owned());
        Self {
            trace_id: trace_id.to_owned(),
            parent_trace_ids: BTreeSet::new(),
            first_seen: epoch,
            last_seen: epoch,
            event_count: 0,
            events: BTreeSet::new(),
            actors: BTreeSet::new(),
            indexed_values,
        }
    }

    fn add_entry(&mut self, entry: &TraceReplayEntry) {
        if self.event_count == 0 || entry.timestamp < self.first_seen {
            self.first_seen = entry.timestamp;
        }
        if self.event_count == 0 || entry.timestamp > self.last_seen {
            self.last_seen = entry.timestamp;
        }
        self.event_count += 1;
        self.events.insert(entry.event_type.clone());
        self.actors.insert(entry.actor.clone());
        self.add_indexed_value("event", &entry.event_type);
        self.add_indexed_value("event_type", &entry.event_type);
        self.add_indexed_value("actor", &entry.actor);
        if let Some(parent_trace_id) = &entry.parent_trace_id {
            self.parent_trace_ids.insert(parent_trace_id.clone());
            self.add_indexed_value("parent_trace_id", parent_trace_id);
            self.add_indexed_value("parent_trace", parent_trace_id);
        }
        if let Some(payload) = entry.payload.as_object() {
            for (key, value) in payload {
                if let Some(value) = scalar_payload_value(value) {
                    self.add_indexed_value(key, &value);
                    if key == "harness_id" {
                        self.add_indexed_value("harness", &value);
                    }
                }
            }
        }
    }

    fn finish(self) -> TraceFindMatch {
        let parent_trace_id = self.parent_trace_ids.iter().next().cloned();
        let task_ids = self.field_values("task_id");
        let session_ids = self.field_values("session_id");
        let pr_refs = self.field_values("pr_ref");
        let harnesses = self
            .indexed_values
            .get("harness")
            .into_iter()
            .chain(self.indexed_values.get("harness_id"))
            .flat_map(|values| values.iter().cloned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let outcomes = self.field_values("outcome");
        TraceFindMatch {
            trace_id: self.trace_id,
            parent_trace_id,
            first_seen: self.first_seen,
            last_seen: self.last_seen,
            event_count: self.event_count,
            events: self.events.into_iter().collect(),
            actors: self.actors.into_iter().collect(),
            task_ids,
            session_ids,
            pr_refs,
            harnesses,
            outcomes,
        }
    }

    fn add_indexed_value(&mut self, key: &str, value: &str) {
        if let Some(key) = normalize_filter_key(key) {
            self.indexed_values
                .entry(key)
                .or_default()
                .insert(value.to_owned());
        }
    }

    fn field_values(&self, key: &str) -> Vec<String> {
        self.indexed_values
            .get(key)
            .map(|values| values.iter().cloned().collect())
            .unwrap_or_default()
    }
}

fn split_filter_terms(filter: &str) -> Result<Vec<String>, TraceFindError> {
    let mut terms = Vec::new();
    let mut current = Vec::new();
    for token in filter.split_whitespace() {
        if token.eq_ignore_ascii_case("AND") {
            if current.is_empty() {
                return Err(TraceFindError::InvalidFilter {
                    filter: filter.to_owned(),
                    detail: "AND must separate key=value terms".into(),
                });
            }
            terms.push(current.join(" "));
            current.clear();
        } else {
            current.push(token);
        }
    }
    if !current.is_empty() {
        terms.push(current.join(" "));
    }
    if terms.is_empty() {
        return Err(TraceFindError::InvalidFilter {
            filter: filter.to_owned(),
            detail: "filter must contain at least one key=value term".into(),
        });
    }
    Ok(terms)
}

fn normalize_filter_key(key: &str) -> Option<String> {
    if key.is_empty()
        || !key
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-'))
    {
        return None;
    }
    Some(key.replace('-', "_"))
}

fn parse_since(value: &str, now: DateTime<Utc>) -> Result<DateTime<Utc>, String> {
    if let Some(raw) = value.strip_prefix("last-") {
        let (amount, unit) = raw.split_at(raw.len().saturating_sub(1));
        if amount.is_empty() || unit.is_empty() {
            return Err(format!("invalid since value {value:?}"));
        }
        let amount = amount
            .parse::<i64>()
            .map_err(|err| format!("invalid relative since value {value:?}: {err}"))?;
        if amount <= 0 {
            return Err(format!("relative since value {value:?} must be positive"));
        }
        let duration = match unit {
            "d" => Duration::days(amount),
            "h" => Duration::hours(amount),
            "m" => Duration::minutes(amount),
            _ => {
                return Err(format!(
                    "relative since value {value:?} must end with d, h, or m"
                ));
            }
        };
        return Ok(now - duration);
    }
    if let Ok(timestamp) = DateTime::parse_from_rfc3339(value) {
        return Ok(timestamp.with_timezone(&Utc));
    }
    let date = NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|err| format!("invalid since value {value:?}: {err}"))?;
    let Some(timestamp) = date.and_hms_opt(0, 0, 0) else {
        return Err(format!("invalid since date {value:?}"));
    };
    Ok(DateTime::<Utc>::from_naive_utc_and_offset(timestamp, Utc))
}

fn scalar_payload_value(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(value) => Some(value.clone()),
        serde_json::Value::Bool(value) => Some(value.to_string()),
        serde_json::Value::Number(value) => Some(value.to_string()),
        serde_json::Value::Null | serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trace_replay_walks_parent_chain_from_journal() {
        let tmp = tempfile::tempdir().unwrap();
        let journal_root = tmp.path().join("journal");
        let day = journal_root.join("2026-05-06");
        fs::create_dir_all(&day).unwrap();
        let root = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
        let child = "01BRZ3NDEKTSV4RRFFQ69G5FAV";
        let unrelated = "01CRZ3NDEKTSV4RRFFQ69G5FAV";
        fs::write(
            day.join("journal.maestro.jsonl"),
            format!(
                "{}\n{}\n",
                envelope_with_trace(
                    "maestro.session-started",
                    root,
                    None,
                    1,
                    &serde_json::json!({"session_id": "maestro-1"})
                ),
                envelope_with_trace(
                    "maestro.session-started",
                    unrelated,
                    None,
                    1,
                    &serde_json::json!({"session_id": "maestro-other"})
                )
            ),
        )
        .unwrap();
        fs::write(
            day.join("journal.picker.jsonl"),
            format!(
                "{}\n",
                envelope_with_trace(
                    "picker.spawned",
                    child,
                    Some(root),
                    2,
                    &serde_json::json!({
                        "task_id": "task-1",
                        "session_id": "codex-cli:abc",
                        "harness_id": "codex-cli",
                        "outcome": "failed"
                    })
                )
            ),
        )
        .unwrap();

        let replay = trace_replay_from_journal(&journal_root, child, 5).unwrap();

        assert_eq!(replay.chain, vec![child.to_owned(), root.to_owned()]);
        assert_eq!(replay.entries.len(), 2);
        assert_eq!(replay.entries[0].trace_id, root);
        assert_eq!(replay.entries[1].trace_id, child);
        assert!(replay
            .entries
            .iter()
            .all(|entry| entry.trace_id != unrelated));
    }

    #[test]
    fn find_traces_matches_payload_and_since_terms() {
        let tmp = tempfile::tempdir().unwrap();
        let journal_root = tmp.path().join("journal");
        let day = journal_root.join("2026-05-06");
        fs::create_dir_all(&day).unwrap();
        let root = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
        let child = "01BRZ3NDEKTSV4RRFFQ69G5FAV";
        fs::write(
            day.join("journal.picker.jsonl"),
            format!(
                "{}\n{}\n",
                envelope_with_trace(
                    "picker.spawned",
                    child,
                    Some(root),
                    1,
                    &serde_json::json!({
                        "task_id": "task-1",
                        "session_id": "codex-cli:abc",
                        "harness_id": "codex-cli"
                    })
                ),
                envelope_with_trace(
                    "picker.finished",
                    child,
                    Some(root),
                    2,
                    &serde_json::json!({
                        "task_id": "task-1",
                        "outcome": "failed"
                    })
                )
            ),
        )
        .unwrap();

        let result = find_traces_in_journal(
            &journal_root,
            "harness=codex-cli AND outcome=failed AND since=2026-05-05",
            10,
        )
        .unwrap();

        assert_eq!(result.matches.len(), 1);
        assert_eq!(result.matches[0].trace_id, child);
        assert_eq!(result.matches[0].parent_trace_id.as_deref(), Some(root));
        assert_eq!(result.matches[0].task_ids, vec!["task-1"]);
        assert_eq!(result.matches[0].harnesses, vec!["codex-cli"]);
        assert_eq!(result.matches[0].outcomes, vec!["failed"]);
    }

    #[test]
    fn trace_replay_rejects_invalid_trace_ids() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("journal")).unwrap();

        let error =
            trace_replay_from_journal(&tmp.path().join("journal"), "not-a-trace", 5).unwrap_err();

        assert!(matches!(error, TraceReplayError::InvalidTraceId { .. }));
    }

    fn envelope_with_trace(
        event_type: &str,
        trace_id: &str,
        parent_trace_id: Option<&str>,
        journal_seq: u64,
        payload: &serde_json::Value,
    ) -> String {
        serde_json::json!({
            "event_type": event_type,
            "event_subtype_version": 1,
            "timestamp": "2026-05-06T00:00:00Z",
            "journal_seq": journal_seq,
            "trace_id": trace_id,
            "parent_trace_id": parent_trace_id,
            "actor": "test",
            "payload": payload,
        })
        .to_string()
    }
}
