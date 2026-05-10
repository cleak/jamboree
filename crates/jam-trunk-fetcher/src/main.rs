//! `jam-trunk-fetcher` - periodic git fetch and branch staleness publisher.
//!
//! This watcher is intentionally deterministic: it fetches the configured
//! Blueberry trunk, emits journal events when `origin/<trunk>` moves, and
//! recomputes simple behind/ahead counts for worktrees discovered from the
//! append-only journal. It never rebases, merges, or edits Picker worktrees.

#![deny(missing_docs)]

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use clap::Parser;
use futures::StreamExt;
use jam_events::generated::{BranchStalenessUpdated, BranchTrunkMoved, Event};
use jam_events::EventEnvelope;
use jam_nats::async_nats;
use jam_nats::JamNats;
use jam_trace::TraceCtx;
use serde::Deserialize;
use serde_json::Value;
use tokio::process::Command;
use tokio::time::{self, Duration};
use tracing::{error, info, warn};

const SERVICE_NAME: &str = "jam-trunk-fetcher";
const SERVICE_VERSION: &str = env!("CARGO_PKG_VERSION");
const DEFAULT_REPO_PATH: &str = "/home/caleb/blueberry";
const DEFAULT_PROJECT: &str = "blueberry";
const DEFAULT_REMOTE: &str = "origin";
const DEFAULT_TRUNK_REF: &str = "origin/master";
const DEFAULT_INTERVAL_SECS: u64 = 300;

#[derive(Debug, Parser)]
#[command(name = SERVICE_NAME, version, about = "Fetch trunk and publish branch staleness")]
struct Cli {
    /// Local repository path whose remote trunk should be fetched.
    #[arg(long)]
    repo_path: Option<PathBuf>,

    /// Project label for branch.trunk-moved events.
    #[arg(long)]
    project: Option<String>,

    /// Git remote to fetch.
    #[arg(long)]
    remote: Option<String>,

    /// Remote-tracking trunk ref, e.g. origin/master.
    #[arg(long)]
    trunk_ref: Option<String>,

    /// Fetch cadence in seconds.
    #[arg(long)]
    interval_secs: Option<u64>,

    /// Fetch and recompute once, then exit.
    #[arg(long)]
    once: bool,

    /// Stop after this many fetch ticks; useful for smoke tests.
    #[arg(long)]
    max_ticks: Option<u64>,
}

#[derive(Debug, thiserror::Error)]
enum FetcherError {
    #[error("nats: {0}")]
    Nats(#[from] jam_nats::NatsError),

    #[error("subscribe: {0}")]
    Subscribe(String),

    #[error("git: {0}")]
    Git(String),

    #[error("json: {0}")]
    Json(#[from] serde_json::Error),

    #[error("protocol: {0}")]
    Protocol(String),
}

#[derive(Debug, Clone)]
struct Config {
    nats_url: String,
    nats_token: Option<String>,
    journal_root: PathBuf,
    git_bin: PathBuf,
    repo_path: PathBuf,
    project: String,
    remote: String,
    trunk_ref: String,
    interval_secs: u64,
}

impl Config {
    fn from_env_and_cli(cli: &Cli) -> Self {
        let jam_home = jam_tools_core::paths::jam_home();
        let journal_root = std::env::var_os("JAM_JOURNAL_ROOT")
            .map_or_else(|| jam_home.join("journal"), PathBuf::from);
        let repo_path = cli
            .repo_path
            .clone()
            .or_else(|| std::env::var_os("JAM_TRUNK_REPO_PATH").map(PathBuf::from))
            .or_else(|| std::env::var_os("JAM_REPO_PATH").map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from(DEFAULT_REPO_PATH));

        Self {
            nats_url: std::env::var("NATS_URL").unwrap_or_else(|_| "nats://127.0.0.1:4222".into()),
            nats_token: std::env::var("NATS_TOKEN").ok(),
            journal_root,
            git_bin: std::env::var_os("JAM_GIT_BIN").map_or_else(|| "git".into(), PathBuf::from),
            repo_path,
            project: cli
                .project
                .clone()
                .or_else(|| std::env::var("JAM_PROJECT").ok())
                .unwrap_or_else(|| DEFAULT_PROJECT.into()),
            remote: cli
                .remote
                .clone()
                .or_else(|| std::env::var("JAM_TRUNK_REMOTE").ok())
                .unwrap_or_else(|| DEFAULT_REMOTE.into()),
            trunk_ref: cli
                .trunk_ref
                .clone()
                .or_else(|| std::env::var("JAM_TRUNK_REF").ok())
                .unwrap_or_else(|| DEFAULT_TRUNK_REF.into()),
            interval_secs: cli.interval_secs.unwrap_or_else(|| {
                env_parse("JAM_TRUNK_FETCH_INTERVAL_SECS").unwrap_or(DEFAULT_INTERVAL_SECS)
            }),
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
        error!("jam-trunk-fetcher fatal: {err}");
        return std::process::ExitCode::FAILURE;
    }
    std::process::ExitCode::SUCCESS
}

async fn run() -> Result<(), FetcherError> {
    init_tracing();
    let cli = Cli::parse();
    let config = Config::from_env_and_cli(&cli);

    info!(
        service = %SERVICE_NAME,
        version = %SERVICE_VERSION,
        nats = %config.nats_url,
        journal_root = %config.journal_root.display(),
        repo_path = %config.repo_path.display(),
        remote = %config.remote,
        trunk_ref = %config.trunk_ref,
        interval_secs = config.interval_secs,
        "starting",
    );

    let nats = JamNats::connect(&config.nats_url, config.nats_token.clone()).await?;
    info!("connected to NATS");

    let mut fetcher = TrunkFetcher::new(config.clone());
    fetcher.load_journal();

    if cli.once {
        fetcher.run_cycle(&nats, Utc::now()).await?;
        return Ok(());
    }

    let mut sub = nats
        .client()
        .subscribe("journal.worktree.created")
        .await
        .map_err(|err| FetcherError::Subscribe(err.to_string()))?;
    info!(subject = "journal.worktree.created", "subscribed");

    let mut interval = time::interval(Duration::from_secs(config.interval_secs));
    interval.set_missed_tick_behavior(time::MissedTickBehavior::Delay);
    let shutdown = tokio::signal::ctrl_c();
    tokio::pin!(shutdown);
    let mut ticks = 0_u64;

    loop {
        tokio::select! {
            _ = &mut shutdown => {
                info!("shutdown signal received");
                return Ok(());
            }
            _ = interval.tick() => {
                ticks = ticks.saturating_add(1);
                fetcher.run_cycle(&nats, Utc::now()).await?;
                if cli.max_ticks.is_some_and(|max_ticks| ticks >= max_ticks) {
                    info!(ticks, "max ticks reached");
                    return Ok(());
                }
            }
            msg = sub.next() => {
                let Some(message) = msg else {
                    warn!("worktree.created subscription closed");
                    return Ok(());
                };
                if let Err(err) = fetcher.handle_worktree_created(&message) {
                    warn!(subject = %message.subject, "ignored worktree.created event: {err}");
                }
            }
        }
    }
}

fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("jam_trunk_fetcher=info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

#[derive(Debug)]
struct TrunkFetcher {
    config: Config,
    worktrees: HashMap<String, ActiveWorktree>,
}

impl TrunkFetcher {
    fn new(config: Config) -> Self {
        Self {
            config,
            worktrees: HashMap::new(),
        }
    }

    fn load_journal(&mut self) {
        let files = journal_files(&self.config.journal_root);
        if files.is_empty() {
            info!(
                journal_root = %self.config.journal_root.display(),
                "no worktree/branch journal files found at startup",
            );
            return;
        }

        for path in files {
            self.load_journal_file(&path);
        }
        info!(
            active_worktrees = self.worktrees.len(),
            "journal replay complete"
        );
    }

    fn load_journal_file(&mut self, path: &Path) {
        let Ok(file) = File::open(path) else {
            warn!(path = %path.display(), "failed to open journal file");
            return;
        };

        for line in BufReader::new(file).lines().map_while(Result::ok) {
            let Ok(envelope) = serde_json::from_str::<JournalEnvelope>(&line) else {
                continue;
            };
            self.apply_journal_envelope(&envelope);
        }
    }

    fn apply_journal_envelope(&mut self, envelope: &JournalEnvelope) {
        match envelope.event_type.as_str() {
            "worktree.created" => {
                if let Some(worktree) = ActiveWorktree::from_created(&envelope.payload) {
                    self.worktrees.insert(worktree.task_id.clone(), worktree);
                }
            }
            "branch.staleness-updated" => {
                let Some(task_id) = value_string(&envelope.payload, "task_id") else {
                    return;
                };
                let Some(counts) = StalenessCounts::from_payload(&envelope.payload) else {
                    return;
                };
                if let Some(worktree) = self.worktrees.get_mut(&task_id) {
                    worktree.last_counts = Some(counts);
                }
            }
            _ => {}
        }
    }

    fn handle_worktree_created(
        &mut self,
        message: &async_nats::Message,
    ) -> Result<(), FetcherError> {
        if message
            .headers
            .as_ref()
            .and_then(jam_nats::extract_trace_from_headers)
            .is_none()
        {
            return Err(FetcherError::Protocol(
                "journal.worktree.created arrived without Trace-Id headers".into(),
            ));
        }

        let envelope = serde_json::from_slice::<JournalEnvelope>(&message.payload)?;
        if envelope.event_type != "worktree.created" {
            return Err(FetcherError::Protocol(format!(
                "expected worktree.created envelope, got {}",
                envelope.event_type
            )));
        }
        let Some(worktree) = ActiveWorktree::from_created(&envelope.payload) else {
            return Err(FetcherError::Protocol(
                "worktree.created payload did not contain task_id and worktree_path".into(),
            ));
        };
        info!(
            task_id = %worktree.task_id,
            worktree_path = %worktree.path.display(),
            "tracking worktree",
        );
        self.worktrees.insert(worktree.task_id.clone(), worktree);
        Ok(())
    }

    async fn run_cycle(&mut self, nats: &JamNats, now: DateTime<Utc>) -> Result<(), FetcherError> {
        let ctx = TraceCtx::new_root(
            "trunk-fetcher.tick",
            format!("fetch {} {}", self.config.remote, self.config.trunk_ref),
        );
        let before = rev_parse_optional(
            &self.config.git_bin,
            &self.config.repo_path,
            &self.config.trunk_ref,
        )
        .await?;
        git_fetch(
            &self.config.git_bin,
            &self.config.repo_path,
            &self.config.remote,
        )
        .await?;
        let after = rev_parse_required(
            &self.config.git_bin,
            &self.config.repo_path,
            &self.config.trunk_ref,
        )
        .await?;

        if let Some(old_sha) = before.as_deref().filter(|old_sha| *old_sha != after) {
            let payload = BranchTrunkMoved {
                project: self.config.project.clone(),
                old_sha: old_sha.into(),
                new_sha: after.clone(),
                fetched_at: now,
            };
            publish_journal_event(nats, payload, &ctx).await?;
        }

        let tasks: Vec<String> = self.worktrees.keys().cloned().collect();
        for task_id in tasks {
            let Some(mut worktree) = self.worktrees.remove(&task_id) else {
                continue;
            };
            match staleness_counts(&self.config.git_bin, &worktree.path, &self.config.trunk_ref)
                .await
            {
                Ok(counts) => {
                    if worktree.last_counts != Some(counts) {
                        let payload = BranchStalenessUpdated {
                            task_id: worktree.task_id.clone(),
                            commits_behind: counts.commits_behind,
                            commits_ahead: counts.commits_ahead,
                            ts: now,
                        };
                        publish_journal_event(nats, payload, &ctx).await?;
                        worktree.last_counts = Some(counts);
                    }
                }
                Err(err) => warn!(
                    task_id = %worktree.task_id,
                    worktree_path = %worktree.path.display(),
                    "staleness recompute failed: {err}",
                ),
            }
            self.worktrees.insert(worktree.task_id.clone(), worktree);
        }

        info!(
            trunk_sha = %after,
            active_worktrees = self.worktrees.len(),
            "trunk fetch cycle complete",
        );
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct ActiveWorktree {
    task_id: String,
    path: PathBuf,
    last_counts: Option<StalenessCounts>,
}

impl ActiveWorktree {
    fn from_created(payload: &Value) -> Option<Self> {
        Some(Self {
            task_id: value_string(payload, "task_id")?,
            path: PathBuf::from(value_string(payload, "worktree_path")?),
            last_counts: None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct StalenessCounts {
    commits_behind: u32,
    commits_ahead: u32,
}

impl StalenessCounts {
    fn from_payload(payload: &Value) -> Option<Self> {
        Some(Self {
            commits_behind: value_u32(payload, "commits_behind")?,
            commits_ahead: value_u32(payload, "commits_ahead")?,
        })
    }
}

#[derive(Debug, Deserialize)]
struct JournalEnvelope {
    event_type: String,
    payload: Value,
}

fn journal_files(journal_root: &Path) -> Vec<PathBuf> {
    let Ok(days) = fs::read_dir(journal_root) else {
        return Vec::new();
    };
    let mut day_paths: Vec<PathBuf> = days
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    day_paths.sort();

    let mut files = Vec::new();
    for day_path in day_paths {
        for name in ["journal.worktree.jsonl", "journal.branch.jsonl"] {
            let path = day_path.join(name);
            if path.is_file() {
                files.push(path);
            }
        }
    }
    files
}

fn value_string(payload: &Value, field: &str) -> Option<String> {
    payload.get(field)?.as_str().map(ToOwned::to_owned)
}

fn value_u32(payload: &Value, field: &str) -> Option<u32> {
    payload
        .get(field)?
        .as_u64()
        .and_then(|value| value.try_into().ok())
}

async fn git_fetch(git_bin: &Path, repo_path: &Path, remote: &str) -> Result<(), FetcherError> {
    run_git(
        git_bin,
        repo_path,
        &["fetch".into(), remote.into(), "--prune".into()],
    )
    .await
    .map(|_| ())
}

async fn rev_parse_optional(
    git_bin: &Path,
    repo_path: &Path,
    trunk_ref: &str,
) -> Result<Option<String>, FetcherError> {
    let refspec = format!("{trunk_ref}^{{commit}}");
    let output = Command::new(git_bin)
        .arg("-C")
        .arg(repo_path)
        .arg("rev-parse")
        .arg("--verify")
        .arg(&refspec)
        .output()
        .await
        .map_err(|err| FetcherError::Git(format!("rev-parse {refspec}: {err}")))?;
    if output.status.success() {
        return Ok(Some(
            String::from_utf8_lossy(&output.stdout).trim().to_owned(),
        ));
    }
    Ok(None)
}

async fn rev_parse_required(
    git_bin: &Path,
    repo_path: &Path,
    trunk_ref: &str,
) -> Result<String, FetcherError> {
    let refspec = format!("{trunk_ref}^{{commit}}");
    run_git(
        git_bin,
        repo_path,
        &["rev-parse".into(), "--verify".into(), refspec],
    )
    .await
    .map(|sha| sha.trim().to_owned())
}

async fn staleness_counts(
    git_bin: &Path,
    worktree_path: &Path,
    trunk_ref: &str,
) -> Result<StalenessCounts, FetcherError> {
    let range = format!("{trunk_ref}...HEAD");
    let output = run_git(
        git_bin,
        worktree_path,
        &[
            "rev-list".into(),
            "--left-right".into(),
            "--count".into(),
            range,
        ],
    )
    .await?;
    parse_rev_list_counts(&output).map_err(FetcherError::Git)
}

async fn run_git(git_bin: &Path, cwd: &Path, args: &[String]) -> Result<String, FetcherError> {
    let output = Command::new(git_bin)
        .arg("-C")
        .arg(cwd)
        .args(args)
        .output()
        .await
        .map_err(|err| FetcherError::Git(format!("{}: {err}", args.join(" "))))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        let detail = if stderr.is_empty() { stdout } else { stderr };
        return Err(FetcherError::Git(format!("{}: {detail}", args.join(" "))));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn parse_rev_list_counts(raw: &str) -> Result<StalenessCounts, String> {
    let mut parts = raw.split_whitespace();
    let commits_behind = parts
        .next()
        .ok_or_else(|| format!("rev-list count missing behind field: {raw:?}"))?
        .parse()
        .map_err(|err| format!("behind count is not u32: {err}"))?;
    let commits_ahead = parts
        .next()
        .ok_or_else(|| format!("rev-list count missing ahead field: {raw:?}"))?
        .parse()
        .map_err(|err| format!("ahead count is not u32: {err}"))?;
    Ok(StalenessCounts {
        commits_behind,
        commits_ahead,
    })
}

async fn publish_journal_event<P>(
    nats: &JamNats,
    payload: P,
    ctx: &TraceCtx,
) -> Result<(), FetcherError>
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
    nats.publish_traced(format!("journal.{}", P::EVENT_TYPE), &envelope, ctx)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn parses_rev_list_counts() {
        assert_eq!(
            parse_rev_list_counts("12\t3\n").unwrap(),
            StalenessCounts {
                commits_behind: 12,
                commits_ahead: 3,
            }
        );
        assert!(parse_rev_list_counts("not-counts").is_err());
    }

    #[test]
    fn extracts_worktree_from_created_payload() {
        let payload = serde_json::json!({
            "task_id": "task-1",
            "worktree_path": "/tmp/worktree"
        });
        let worktree = ActiveWorktree::from_created(&payload).unwrap();

        assert_eq!(worktree.task_id, "task-1");
        assert_eq!(worktree.path, PathBuf::from("/tmp/worktree"));
    }

    #[test]
    fn replays_worktree_and_staleness_from_journal() {
        let tmp = TempDir::new().unwrap();
        let day = tmp.path().join("2026-05-06");
        std::fs::create_dir_all(&day).unwrap();
        std::fs::write(
            day.join("journal.worktree.jsonl"),
            serde_json::json!({
                "event_type": "worktree.created",
                "payload": {
                    "task_id": "task-1",
                    "worktree_path": "/tmp/worktree"
                }
            })
            .to_string()
                + "\n",
        )
        .unwrap();
        std::fs::write(
            day.join("journal.branch.jsonl"),
            serde_json::json!({
                "event_type": "branch.staleness-updated",
                "payload": {
                    "task_id": "task-1",
                    "commits_behind": 2,
                    "commits_ahead": 1
                }
            })
            .to_string()
                + "\n",
        )
        .unwrap();
        let mut fetcher = TrunkFetcher::new(test_config(tmp.path()));

        fetcher.load_journal();

        let worktree = fetcher.worktrees.get("task-1").unwrap();
        assert_eq!(
            worktree.last_counts,
            Some(StalenessCounts {
                commits_behind: 2,
                commits_ahead: 1,
            })
        );
    }

    fn test_config(journal_root: &Path) -> Config {
        Config {
            nats_url: "nats://127.0.0.1:4222".into(),
            nats_token: None,
            journal_root: journal_root.to_path_buf(),
            git_bin: PathBuf::from("git"),
            repo_path: PathBuf::from("/tmp/repo"),
            project: "blueberry".into(),
            remote: "origin".into(),
            trunk_ref: "origin/master".into(),
            interval_secs: 300,
        }
    }
}
