use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use chrono::{DateTime, Utc};
use jam_events::EventEnvelope;
use serde::Deserialize;
use serde_json::json;
use tempfile::TempDir;

const ROOT_TRACE: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
const PICKER_TRACE: &str = "01BRZ3NDEKTSV4RRFFQ69G5FAV";
const MERGE_TRACE: &str = "01CRZ3NDEKTSV4RRFFQ69G5FAV";

#[derive(Debug, Deserialize)]
struct JournalEntry {
    event_type: String,
    trace_id: String,
    #[serde(default)]
    parent_trace_id: Option<String>,
}

struct FixtureEvent {
    group: &'static str,
    event_type: &'static str,
    trace_id: &'static str,
    parent_trace_id: Option<&'static str>,
    journal_seq: u64,
    actor: &'static str,
    payload: serde_json::Value,
}

#[test]
fn fake_task_journal_trace_continuity_from_spawn_through_merge() {
    let temp = TempDir::new().unwrap();
    let journal_day = temp.path().join("journal").join("2026-05-06");
    fs::create_dir_all(&journal_day).unwrap();

    for event in fixture_events() {
        append_event(&journal_day, &event);
    }

    let entries = read_entries(&journal_day);
    assert!(entries.iter().any(|entry| entry.event_type == "pr.merged"));
    assert!(entries
        .iter()
        .all(|entry| descends_to_root(entry, ROOT_TRACE, &parent_map(&entries))));

    let output = Command::new(env!("CARGO_BIN_EXE_jam"))
        .env("JAM_HOME", temp.path())
        .args(["trace", "replay", PICKER_TRACE, "--max-depth", "5"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "jam trace replay failed: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("chain: 01BRZ3NDEKTSV4RRFFQ69G5FAV <- 01ARZ3NDEKTSV4RRFFQ69G5FAV"));
    assert!(stdout.contains("event=picker.spawned"));
    assert!(stdout.contains("event=maestro.session-started"));

    let output = Command::new(env!("CARGO_BIN_EXE_jam"))
        .env("JAM_HOME", temp.path())
        .args([
            "trace",
            "find",
            "task_id=trace-smoke AND event=pr.merged",
            "--limit",
            "5",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "jam trace find failed: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("matches: 1"));
    assert!(stdout.contains(MERGE_TRACE));
}

fn fixture_events() -> Vec<FixtureEvent> {
    vec![
        FixtureEvent {
            group: "task",
            event_type: "task.requested",
            trace_id: ROOT_TRACE,
            parent_trace_id: None,
            journal_seq: 1,
            actor: "human:caleb",
            payload: json!({"task_id": "trace-smoke"}),
        },
        FixtureEvent {
            group: "maestro",
            event_type: "maestro.session-started",
            trace_id: ROOT_TRACE,
            parent_trace_id: None,
            journal_seq: 2,
            actor: "mock-llm",
            payload: json!({"session_id": "maestro-trace-smoke", "wake_source": "bus-event"}),
        },
        FixtureEvent {
            group: "worktree",
            event_type: "worktree.created",
            trace_id: PICKER_TRACE,
            parent_trace_id: Some(ROOT_TRACE),
            journal_seq: 3,
            actor: "mock-harness",
            payload: json!({"task_id": "trace-smoke", "worktree_path": "/tmp/trace-smoke"}),
        },
        FixtureEvent {
            group: "picker",
            event_type: "picker.spawned",
            trace_id: PICKER_TRACE,
            parent_trace_id: Some(ROOT_TRACE),
            journal_seq: 4,
            actor: "mock-harness",
            payload: json!({"task_id": "trace-smoke", "session_id": "codex-cli:trace-smoke"}),
        },
        FixtureEvent {
            group: "maestro",
            event_type: "maestro.tool-call",
            trace_id: ROOT_TRACE,
            parent_trace_id: None,
            journal_seq: 5,
            actor: "mock-llm",
            payload: json!({"session_id": "maestro-trace-smoke", "tool_name": "open-pr"}),
        },
        FixtureEvent {
            group: "pr",
            event_type: "pr.opened",
            trace_id: PICKER_TRACE,
            parent_trace_id: Some(ROOT_TRACE),
            journal_seq: 6,
            actor: "mock-github",
            payload: json!({"task_id": "trace-smoke", "pr_ref": "cleak/blueberry#404"}),
        },
        FixtureEvent {
            group: "pr",
            event_type: "pr.merged",
            trace_id: MERGE_TRACE,
            parent_trace_id: Some(ROOT_TRACE),
            journal_seq: 7,
            actor: "mock-github",
            payload: json!({"task_id": "trace-smoke", "pr_ref": "cleak/blueberry#404"}),
        },
    ]
}

fn append_event(journal_day: &Path, event: &FixtureEvent) {
    let mut envelope = EventEnvelope::new(
        event.event_type,
        1,
        event.journal_seq,
        event.trace_id,
        event.actor,
        event.payload.clone(),
    );
    if let Some(parent) = event.parent_trace_id {
        envelope = envelope.with_parent_trace(parent);
    }
    envelope.timestamp = DateTime::parse_from_rfc3339("2026-05-06T05:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let path = journal_day.join(format!("journal.{}.jsonl", event.group));
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .unwrap();
    writeln!(file, "{}", serde_json::to_string(&envelope).unwrap()).unwrap();
}

fn read_entries(journal_day: &Path) -> Vec<JournalEntry> {
    let mut paths: Vec<PathBuf> = fs::read_dir(journal_day)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    paths.sort();
    paths
        .into_iter()
        .flat_map(|path| {
            fs::read_to_string(path)
                .unwrap()
                .lines()
                .map(|line| serde_json::from_str::<JournalEntry>(line).unwrap())
                .collect::<Vec<_>>()
        })
        .collect()
}

fn parent_map(entries: &[JournalEntry]) -> HashMap<&str, Option<&str>> {
    let mut parents = HashMap::new();
    for entry in entries {
        parents
            .entry(entry.trace_id.as_str())
            .or_insert(entry.parent_trace_id.as_deref());
    }
    parents
}

fn descends_to_root(
    entry: &JournalEntry,
    root_trace: &str,
    parents: &HashMap<&str, Option<&str>>,
) -> bool {
    let mut current = entry.trace_id.as_str();
    for _ in 0..=parents.len() {
        if current == root_trace {
            return true;
        }
        let Some(Some(parent)) = parents.get(current) else {
            return false;
        };
        current = parent;
    }
    false
}
