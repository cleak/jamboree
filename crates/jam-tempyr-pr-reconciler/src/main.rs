//! `jam-tempyr-pr-reconciler` - proactive Tempyr drift candidate publisher.
//!
//! The reconciler listens for `pr.merged`, finds Tempyr graph nodes that
//! mention the merge's touched paths, and emits `tempyr.update-candidate` for
//! human or Maestro review. It never edits Tempyr nodes directly.

#![deny(missing_docs)]

use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Component, Path, PathBuf};
use std::str::FromStr;

use chrono::Utc;
use clap::Parser;
use futures::StreamExt;
use jam_events::generated::{Event, TempyrUpdateCandidate};
use jam_events::EventEnvelope;
use jam_nats::async_nats;
use jam_nats::JamNats;
use jam_trace::{TraceCtx, TraceId};
use serde::Deserialize;
use serde_json::Value;
use tokio::time::{timeout, Duration};
use tracing::{error, info, warn};

const SERVICE_NAME: &str = "jam-tempyr-pr-reconciler";
const SERVICE_VERSION: &str = env!("CARGO_PKG_VERSION");
const DEFAULT_CANONICAL_WORKTREE: &str = "/home/caleb/blueberry-jam";
const DEFAULT_GRAPH_RELPATH: &str = "graph";
const DRAIN_TIMEOUT_SECS: u64 = 2;

#[derive(Debug, Parser)]
#[command(
    name = SERVICE_NAME,
    version,
    about = "Publish Tempyr update candidates for merged PR touched paths"
)]
struct Cli {
    /// Tempyr graph directory to scan.
    #[arg(long)]
    graph_dir: Option<PathBuf>,

    /// Replay PR journal files once, then exit.
    #[arg(long)]
    once: bool,

    /// Stop after this many merged PR events; useful for smoke tests.
    #[arg(long)]
    max_events: Option<u64>,
}

#[derive(Debug, thiserror::Error)]
enum ReconcilerError {
    #[error("nats: {0}")]
    Nats(#[from] jam_nats::NatsError),

    #[error("subscribe: {0}")]
    Subscribe(String),

    #[error("json: {0}")]
    Json(#[from] serde_json::Error),

    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("yaml: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("trace: {0}")]
    Trace(#[from] jam_trace::TraceIdParseError),

    #[error("protocol: {0}")]
    Protocol(String),
}

#[derive(Debug, Clone)]
struct Config {
    nats_url: String,
    nats_token: Option<String>,
    journal_root: PathBuf,
    graph_dir: PathBuf,
}

impl Config {
    fn from_env_and_cli(cli: &Cli) -> Self {
        let jam_home = jam_tools_core::paths::jam_home();
        let journal_root = std::env::var_os("JAM_JOURNAL_ROOT")
            .map_or_else(|| jam_home.join("journal"), PathBuf::from);
        let graph_dir = cli.graph_dir.clone().unwrap_or_else(default_graph_dir);

        Self {
            nats_url: std::env::var("NATS_URL").unwrap_or_else(|_| "nats://127.0.0.1:4222".into()),
            nats_token: std::env::var("NATS_TOKEN").ok(),
            journal_root,
            graph_dir,
        }
    }
}

fn default_graph_dir() -> PathBuf {
    if let Some(graph_dir) = std::env::var_os("JAM_GRAPH_DIR") {
        return PathBuf::from(graph_dir);
    }

    let canonical_worktree = std::env::var_os("JAM_CANONICAL_TEMPYR_WORKTREE")
        .or_else(|| std::env::var_os("JAM_TEMPYR_WORKTREE"))
        .map_or_else(|| PathBuf::from(DEFAULT_CANONICAL_WORKTREE), PathBuf::from);
    let graph_relpath = std::env::var_os("JAM_GRAPH_RELPATH")
        .map_or_else(|| PathBuf::from(DEFAULT_GRAPH_RELPATH), PathBuf::from);
    canonical_worktree.join(graph_relpath)
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    if let Err(err) = run().await {
        error!("jam-tempyr-pr-reconciler fatal: {err}");
        return std::process::ExitCode::FAILURE;
    }
    std::process::ExitCode::SUCCESS
}

async fn run() -> Result<(), ReconcilerError> {
    init_tracing();
    let cli = Cli::parse();
    let config = Config::from_env_and_cli(&cli);

    info!(
        service = %SERVICE_NAME,
        version = %SERVICE_VERSION,
        nats = %config.nats_url,
        journal_root = %config.journal_root.display(),
        graph_dir = %config.graph_dir.display(),
        "starting",
    );

    let nats = JamNats::connect(&config.nats_url, config.nats_token.clone()).await?;
    info!("connected to NATS");

    let reconciler = Reconciler::new(config);
    if cli.once {
        reconciler.replay_journal(&nats).await?;
        return Ok(());
    }

    let mut sub = nats
        .client()
        .subscribe("journal.pr.merged")
        .await
        .map_err(|err| ReconcilerError::Subscribe(err.to_string()))?;
    info!(subject = "journal.pr.merged", "subscribed");

    let shutdown = tokio::signal::ctrl_c();
    tokio::pin!(shutdown);
    let mut handled = 0_u64;

    loop {
        tokio::select! {
            _ = &mut shutdown => {
                info!("shutdown signal received");
                return Ok(());
            }
            msg = sub.next() => {
                let Some(message) = msg else {
                    warn!("pr.merged subscription closed");
                    return Ok(());
                };
                reconciler.handle_message(&nats, &message).await?;
                handled = handled.saturating_add(1);
                if cli.max_events.is_some_and(|max_events| handled >= max_events) {
                    info!(handled, "max events reached");
                    drain_bridge(&nats).await;
                    return Ok(());
                }
            }
        }
    }
}

fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("jam_tempyr_pr_reconciler=info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

#[derive(Debug, Clone)]
struct Reconciler {
    config: Config,
}

impl Reconciler {
    fn new(config: Config) -> Self {
        Self { config }
    }

    async fn replay_journal(&self, nats: &JamNats) -> Result<(), ReconcilerError> {
        let files = pr_journal_files(&self.config.journal_root);
        if files.is_empty() {
            info!(
                journal_root = %self.config.journal_root.display(),
                "no PR journal files found at startup",
            );
            return Ok(());
        }

        let mut merged_events = 0_u64;
        for path in files {
            let file = File::open(&path)?;
            for line in BufReader::new(file).lines().map_while(Result::ok) {
                let Ok(envelope) = serde_json::from_str::<JournalEnvelope>(&line) else {
                    continue;
                };
                if envelope.event_type == "pr.merged" {
                    merged_events = merged_events.saturating_add(1);
                    let ctx = envelope.trace_ctx()?;
                    self.reconcile_merge(nats, &envelope, &ctx).await?;
                }
            }
        }
        info!(merged_events, "journal replay complete");
        Ok(())
    }

    async fn handle_message(
        &self,
        nats: &JamNats,
        message: &async_nats::Message,
    ) -> Result<(), ReconcilerError> {
        let ctx = message
            .headers
            .as_ref()
            .and_then(jam_nats::extract_trace_from_headers)
            .ok_or_else(|| {
                ReconcilerError::Protocol(
                    "journal.pr.merged arrived without Trace-Id headers".into(),
                )
            })?;
        let envelope = serde_json::from_slice::<JournalEnvelope>(&message.payload)?;
        if envelope.event_type != "pr.merged" {
            return Err(ReconcilerError::Protocol(format!(
                "expected pr.merged envelope, got {}",
                envelope.event_type
            )));
        }
        self.reconcile_merge(nats, &envelope, &ctx).await
    }

    async fn reconcile_merge(
        &self,
        nats: &JamNats,
        envelope: &JournalEnvelope,
        ctx: &TraceCtx,
    ) -> Result<(), ReconcilerError> {
        let merge = MergeEvent::from_envelope(envelope)?;
        let references = find_referencing_nodes(&self.config.graph_dir, &merge.touched_paths)?;
        if references.is_empty() {
            info!(
                pr_ref = %merge.pr_ref,
                touched_paths = merge.touched_paths.len(),
                "no Tempyr nodes referenced touched paths",
            );
            return Ok(());
        }

        for reference in references {
            let reason = format!(
                "{} merged {} touching {}",
                merge.pr_ref,
                short_sha(&merge.merged_sha),
                reference.paths.join(", ")
            );
            let payload = TempyrUpdateCandidate {
                node_id: reference.node_id.clone(),
                source: "auto".into(),
                reason,
                ts: Utc::now(),
            };
            publish_journal_event(nats, payload, ctx).await?;
            info!(
                pr_ref = %merge.pr_ref,
                node_id = %reference.node_id,
                paths = %reference.paths.join(","),
                "published tempyr.update-candidate",
            );
        }
        Ok(())
    }
}

async fn drain_bridge(nats: &JamNats) {
    if timeout(
        Duration::from_secs(DRAIN_TIMEOUT_SECS),
        nats.client().flush(),
    )
    .await
    .is_err()
    {
        warn!("timed out while flushing NATS before exit");
    }
}

async fn publish_journal_event<P>(
    nats: &JamNats,
    payload: P,
    ctx: &TraceCtx,
) -> Result<(), ReconcilerError>
where
    P: Event,
{
    let mut envelope = EventEnvelope::new(
        P::EVENT_TYPE,
        P::EVENT_SUBTYPE_VERSION,
        0,
        ctx.trace_id.to_string(),
        SERVICE_NAME,
        payload,
    );
    if let Some(parent) = ctx.parent_trace_id {
        envelope = envelope.with_parent_trace(parent.to_string());
    }
    nats.publish_traced(format!("journal.{}", P::EVENT_TYPE), &envelope, ctx)
        .await?;
    Ok(())
}

#[derive(Debug, Deserialize)]
struct JournalEnvelope {
    event_type: String,
    trace_id: String,
    #[serde(default)]
    parent_trace_id: Option<String>,
    payload: Value,
}

impl JournalEnvelope {
    fn trace_ctx(&self) -> Result<TraceCtx, ReconcilerError> {
        let trace_id = TraceId::from_str(&self.trace_id)?;
        let parent_trace_id = self
            .parent_trace_id
            .as_deref()
            .map(TraceId::from_str)
            .transpose()?;
        Ok(TraceCtx {
            trace_id,
            parent_trace_id,
            origin_kind: "journal.replay",
            origin_summary: String::new(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MergeEvent {
    pr_ref: String,
    merged_sha: String,
    touched_paths: Vec<String>,
}

impl MergeEvent {
    fn from_envelope(envelope: &JournalEnvelope) -> Result<Self, ReconcilerError> {
        let pr_ref = value_string(&envelope.payload, "pr_ref")
            .ok_or_else(|| ReconcilerError::Protocol("pr.merged payload missing pr_ref".into()))?;
        let merged_sha = value_string(&envelope.payload, "merged_sha").ok_or_else(|| {
            ReconcilerError::Protocol("pr.merged payload missing merged_sha".into())
        })?;
        let raw_paths = value_string(&envelope.payload, "touched_paths").ok_or_else(|| {
            ReconcilerError::Protocol("pr.merged payload missing touched_paths".into())
        })?;
        let touched_paths = parse_touched_paths(&raw_paths)?;
        if touched_paths.is_empty() {
            return Err(ReconcilerError::Protocol(
                "pr.merged touched_paths did not contain any usable relative paths".into(),
            ));
        }

        Ok(Self {
            pr_ref,
            merged_sha,
            touched_paths,
        })
    }
}

fn value_string(payload: &Value, field: &str) -> Option<String> {
    payload.get(field)?.as_str().map(ToOwned::to_owned)
}

fn parse_touched_paths(raw: &str) -> Result<Vec<String>, ReconcilerError> {
    let parsed = if raw.trim_start().starts_with('[') {
        serde_json::from_str::<Vec<String>>(raw)?
    } else {
        raw.split([',', '\n'])
            .map(str::trim)
            .filter(|path| !path.is_empty())
            .map(ToOwned::to_owned)
            .collect()
    };

    let mut unique = BTreeSet::new();
    for path in parsed {
        let Some(normalized) = normalize_touched_path(&path) else {
            continue;
        };
        unique.insert(normalized);
    }
    Ok(unique.into_iter().collect())
}

fn normalize_touched_path(path: &str) -> Option<String> {
    let trimmed = path.trim().trim_matches('`').trim_start_matches("./");
    let path = Path::new(trimmed);
    if path.is_absolute() {
        return None;
    }

    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    if parts.is_empty() {
        return None;
    }
    Some(parts.join("/"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NodeReference {
    node_id: String,
    paths: Vec<String>,
}

fn find_referencing_nodes(
    graph_dir: &Path,
    touched_paths: &[String],
) -> Result<Vec<NodeReference>, ReconcilerError> {
    let mut matches = Vec::new();
    for path in markdown_files(graph_dir)? {
        let content = fs::read_to_string(&path)?;
        let referenced: Vec<String> = touched_paths
            .iter()
            .filter(|touched_path| content.contains(touched_path.as_str()))
            .cloned()
            .collect();
        if referenced.is_empty() {
            continue;
        }

        let node_id = node_id_from_markdown(&content)
            .or_else(|| {
                path.file_stem()
                    .map(|stem| stem.to_string_lossy().into_owned())
            })
            .ok_or_else(|| {
                ReconcilerError::Protocol(format!("{} has no node id", path.display()))
            })?;
        matches.push(NodeReference {
            node_id,
            paths: referenced,
        });
    }
    matches.sort_by(|left, right| left.node_id.cmp(&right.node_id));
    Ok(matches)
}

fn markdown_files(root: &Path) -> Result<Vec<PathBuf>, ReconcilerError> {
    let mut files = Vec::new();
    collect_markdown_files(root, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_markdown_files(path: &Path, files: &mut Vec<PathBuf>) -> Result<(), ReconcilerError> {
    if !path.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_markdown_files(&path, files)?;
        } else if path.extension().is_some_and(|extension| extension == "md") {
            files.push(path);
        }
    }
    Ok(())
}

fn node_id_from_markdown(content: &str) -> Option<String> {
    let mut lines = content.lines();
    if lines.next()? != "---" {
        return None;
    }
    let mut frontmatter = String::new();
    for line in lines {
        if line == "---" {
            break;
        }
        frontmatter.push_str(line);
        frontmatter.push('\n');
    }
    let value: serde_yaml::Value = serde_yaml::from_str(&frontmatter).ok()?;
    value.get("id")?.as_str().map(ToOwned::to_owned)
}

fn pr_journal_files(journal_root: &Path) -> Vec<PathBuf> {
    let Ok(days) = fs::read_dir(journal_root) else {
        return Vec::new();
    };
    let mut files = Vec::new();
    for day in days.flatten() {
        let day_path = day.path();
        if !day_path.is_dir() {
            continue;
        }
        let path = day_path.join("journal.pr.jsonl");
        if path.is_file() {
            files.push(path);
        }
    }
    files.sort();
    files
}

fn short_sha(sha: &str) -> &str {
    sha.get(..12).unwrap_or(sha)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn parses_json_touched_paths() {
        assert_eq!(
            parse_touched_paths(r#"["./crates/foo/src/lib.rs","docs/runbook.md"]"#).unwrap(),
            vec!["crates/foo/src/lib.rs", "docs/runbook.md"],
        );
    }

    #[test]
    fn parses_delimited_touched_paths_and_filters_unsafe_paths() {
        assert_eq!(
            parse_touched_paths("crates/a.rs,\n../secret\n/mnt/c/nope\n`docs/x.md`").unwrap(),
            vec!["crates/a.rs", "docs/x.md"],
        );
    }

    #[test]
    fn finds_nodes_referencing_touched_paths() {
        let temp = TempDir::new().unwrap();
        let graph = temp.path().join("graph");
        fs::create_dir_all(graph.join("components")).unwrap();
        fs::create_dir_all(graph.join("features")).unwrap();
        fs::write(
            graph.join("components").join("comp-a.md"),
            "---\nid: comp-a\ntype: component\n---\nUses `crates/foo/src/lib.rs`.\n",
        )
        .unwrap();
        fs::write(
            graph.join("features").join("feat-b.md"),
            "---\nid: feat-b\ntype: feature\n---\nNo reference here.\n",
        )
        .unwrap();

        let references =
            find_referencing_nodes(&graph, &[String::from("crates/foo/src/lib.rs")]).unwrap();

        assert_eq!(
            references,
            vec![NodeReference {
                node_id: "comp-a".into(),
                paths: vec!["crates/foo/src/lib.rs".into()],
            }],
        );
    }

    #[test]
    fn derives_node_id_from_file_stem_without_frontmatter() {
        let temp = TempDir::new().unwrap();
        let graph = temp.path().join("graph");
        fs::create_dir_all(&graph).unwrap();
        fs::write(graph.join("loose.md"), "Mentions docs/x.md\n").unwrap();

        let references = find_referencing_nodes(&graph, &[String::from("docs/x.md")]).unwrap();

        assert_eq!(references[0].node_id, "loose");
    }
}
