//! `jam-pr-poller` - ETag-cached GitHub PR status poller (§4.4.6, §21.3).
//!
//! The poller discovers active PRs from `pr.opened` journal events, polls the
//! GitHub PR endpoint with `If-None-Match`, and emits journal events only when
//! observed PR/review/CI state changes. GitHub access is currently via the
//! installed `gh` CLI so this slice can be smoke-tested before the shared
//! GitHub App client lands.

#![deny(missing_docs)]

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use chrono::{DateTime, TimeDelta, Utc};
use clap::Parser;
use futures::StreamExt;
use jam_events::generated::{
    Event, PrBranchUpdated, PrCiStatusChanged, PrMerged, PrReviewReceived, PrStatusChanged,
};
use jam_events::EventEnvelope;
use jam_nats::async_nats;
use jam_nats::JamNats;
use jam_secrets::{FileBackend, PassBackend, SecretBackend, SecretError, SecretKey};
use jam_trace::TraceCtx;
use octocrab::models::{AppId, InstallationId};
use octocrab::Octocrab;
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use serde_json::Value;
use tokio::process::Command;
use tokio::time::{self, Duration};
use tracing::{error, info, warn};

const SERVICE_NAME: &str = "jam-pr-poller";
const SERVICE_VERSION: &str = env!("CARGO_PKG_VERSION");
const DEFAULT_REPO: &str = "cleak/blueberry";
const DEFAULT_POLL_INTERVAL_SECS: u64 = 30;
const DEFAULT_INACTIVE_SECS: u64 = 300;
const DEFAULT_INACTIVE_AFTER_SECS: u64 = 1_800;
const DEFAULT_TICK_SECS: u64 = 5;

#[derive(Debug, Parser)]
#[command(name = SERVICE_NAME, version, about = "Poll active GitHub PRs with ETag caching")]
struct Cli {
    /// Active PR poll cadence in seconds.
    #[arg(long)]
    interval_secs: Option<u64>,

    /// Inactive PR poll cadence in seconds.
    #[arg(long)]
    inactive_secs: Option<u64>,

    /// Treat PRs with no activity for this many seconds as inactive.
    #[arg(long)]
    inactive_after_secs: Option<u64>,

    /// Scheduler tick cadence in seconds.
    #[arg(long)]
    tick_secs: Option<u64>,

    /// Poll all replayed PRs once, then exit.
    #[arg(long)]
    once: bool,

    /// Stop after this many scheduler ticks; useful for smoke tests.
    #[arg(long)]
    max_ticks: Option<u64>,
}

#[derive(Debug, thiserror::Error)]
enum PollerError {
    #[error("nats: {0}")]
    Nats(#[from] jam_nats::NatsError),

    #[error("subscribe: {0}")]
    Subscribe(String),

    #[error("github: {0}")]
    GitHub(String),

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
    gh_bin: PathBuf,
    default_repo: String,
    github_app: Option<GitHubAppConfig>,
    interval_secs: u64,
    inactive_secs: u64,
    inactive_after_secs: u64,
    tick_secs: u64,
}

impl Config {
    fn from_env_and_cli(cli: &Cli) -> Result<Self, PollerError> {
        let jam_home = jam_tools_core::paths::jam_home();
        let journal_root = std::env::var_os("JAM_JOURNAL_ROOT")
            .map_or_else(|| jam_home.join("journal"), PathBuf::from);
        let default_repo = std::env::var("JAM_PR_POLLER_REPO")
            .or_else(|_| std::env::var("JAM_GITHUB_REPO"))
            .unwrap_or_else(|_| DEFAULT_REPO.into());

        Ok(Self {
            nats_url: std::env::var("NATS_URL").unwrap_or_else(|_| "nats://127.0.0.1:4222".into()),
            nats_token: std::env::var("NATS_TOKEN").ok(),
            journal_root,
            gh_bin: std::env::var_os("JAM_GH_BIN").map_or_else(|| "gh".into(), PathBuf::from),
            default_repo,
            github_app: GitHubAppConfig::from_env()?,
            interval_secs: cli.interval_secs.unwrap_or_else(|| {
                env_parse("JAM_PR_POLL_INTERVAL_SECS").unwrap_or(DEFAULT_POLL_INTERVAL_SECS)
            }),
            inactive_secs: cli.inactive_secs.unwrap_or_else(|| {
                env_parse("JAM_PR_POLL_INACTIVE_SECS").unwrap_or(DEFAULT_INACTIVE_SECS)
            }),
            inactive_after_secs: cli.inactive_after_secs.unwrap_or_else(|| {
                env_parse("JAM_PR_POLL_INACTIVE_AFTER_SECS").unwrap_or(DEFAULT_INACTIVE_AFTER_SECS)
            }),
            tick_secs: cli
                .tick_secs
                .unwrap_or_else(|| env_parse("JAM_PR_POLL_TICK_SECS").unwrap_or(DEFAULT_TICK_SECS)),
        })
    }
}

#[derive(Debug, Clone)]
struct GitHubAppConfig {
    app_id: u64,
    installation_id: u64,
    private_key_pem: SecretString,
    api_base_uri: Option<String>,
}

impl GitHubAppConfig {
    fn from_env() -> Result<Option<Self>, PollerError> {
        let app_id = first_secret(
            &["JAM_GITHUB_APP_ID", "GITHUB_APP_ID"],
            "jam/pickers/github-app-id",
            "pickers/github-app-id",
        )?;
        let installation_id = first_secret(
            &[
                "JAM_GITHUB_APP_INSTALLATION_ID",
                "GITHUB_APP_INSTALLATION_ID",
            ],
            "jam/pickers/github-app-installation-id",
            "pickers/github-app-installation-id",
        )?;
        let private_key = first_secret(
            &["JAM_GITHUB_APP_PRIVATE_KEY", "GITHUB_APP_PRIVATE_KEY"],
            "jam/pickers/github-app-key",
            "pickers/github-app-key",
        )?
        .or_else(|| read_optional_file_env("JAM_GITHUB_APP_PRIVATE_KEY_FILE"))
        .or_else(|| read_optional_file_env("GITHUB_APP_PRIVATE_KEY_FILE"));
        if app_id.is_none() && installation_id.is_none() && private_key.is_none() {
            return Ok(None);
        }
        let app_id = parse_required_u64("GITHUB_APP_ID", app_id)?;
        let installation_id = parse_required_u64("GITHUB_APP_INSTALLATION_ID", installation_id)?;
        let private_key_pem = private_key.ok_or_else(|| {
            PollerError::Protocol(
                "GitHub App private key is missing while other App config is set".into(),
            )
        })?;
        Ok(Some(Self {
            app_id,
            installation_id,
            private_key_pem: SecretString::from(private_key_pem.replace("\\n", "\n")),
            api_base_uri: first_env(&["JAM_GITHUB_API_BASE_URI", "GITHUB_API_BASE_URI"]),
        }))
    }
}

fn env_parse<T>(name: &str) -> Option<T>
where
    T: std::str::FromStr,
{
    std::env::var(name).ok()?.parse().ok()
}

fn first_env(keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| std::env::var(key).ok())
        .filter(|value| !value.is_empty())
}

fn first_secret(
    env_keys: &[&str],
    file_key: &'static str,
    pass_key: &'static str,
) -> Result<Option<String>, PollerError> {
    if let Some(value) = first_env(env_keys) {
        return Ok(Some(value));
    }
    read_secret_key(file_key, pass_key)
}

fn read_secret_key(
    file_key: &'static str,
    pass_key: &'static str,
) -> Result<Option<String>, PollerError> {
    if let Some(path) = std::env::var_os("JAM_SECRETS_FILE").filter(|value| !value.is_empty()) {
        let backend = FileBackend::new(path);
        match backend.get(&SecretKey::new(file_key)) {
            Ok(secret) => return Ok(Some(secret.expose_secret().trim_end().to_owned())),
            Err(SecretError::NotFound(_)) => {}
            Err(err) => {
                return Err(PollerError::Protocol(format!(
                    "failed reading {file_key} from JAM_SECRETS_FILE: {err}",
                )));
            }
        }
    }
    let backend = PassBackend::new("jam");
    match backend.get(&SecretKey::new(pass_key)) {
        Ok(secret) => Ok(Some(secret.expose_secret().trim_end().to_owned())),
        Err(_) => Ok(None),
    }
}

fn read_optional_file_env(key: &str) -> Option<String> {
    let path = std::env::var_os(key).filter(|value| !value.is_empty())?;
    std::fs::read_to_string(path).ok()
}

fn parse_required_u64(name: &'static str, value: Option<String>) -> Result<u64, PollerError> {
    let value = value.ok_or_else(|| {
        PollerError::Protocol(format!(
            "{name} is required when GitHub App config is partially set",
        ))
    })?;
    value.parse::<u64>().map_err(|err| {
        PollerError::Protocol(format!(
            "{name}={value:?} is not an unsigned integer: {err}"
        ))
    })
}

async fn github_app_installation_token(
    config: &Config,
) -> Result<Option<SecretString>, PollerError> {
    let Some(app) = &config.github_app else {
        return Ok(None);
    };
    app_installation_token(app).await.map(Some)
}

async fn app_installation_token(app: &GitHubAppConfig) -> Result<SecretString, PollerError> {
    let key =
        jsonwebtoken::EncodingKey::from_rsa_pem(app.private_key_pem.expose_secret().as_bytes())
            .map_err(|err| {
                PollerError::Protocol(format!("failed to parse GitHub App private key: {err}"))
            })?;
    let crab = {
        let mut builder = Octocrab::builder().app(AppId(app.app_id), key);
        if let Some(base_uri) = &app.api_base_uri {
            builder = builder.base_uri(base_uri).map_err(|err| {
                PollerError::Protocol(format!("invalid GitHub API base URI {base_uri:?}: {err}"))
            })?;
        }
        builder.build().map_err(|err| {
            PollerError::Protocol(format!("failed to build GitHub App client: {err}"))
        })?
    };
    let (_installation, token) = crab
        .installation_and_token(InstallationId(app.installation_id))
        .await
        .map_err(|err| PollerError::Protocol(format!("GitHub App token exchange failed: {err}")))?;
    Ok(token)
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    if let Err(err) = run().await {
        error!("jam-pr-poller fatal: {err}");
        return std::process::ExitCode::FAILURE;
    }
    std::process::ExitCode::SUCCESS
}

async fn run() -> Result<(), PollerError> {
    init_tracing();
    let cli = Cli::parse();
    let config = Config::from_env_and_cli(&cli)?;

    info!(
        service = %SERVICE_NAME,
        version = %SERVICE_VERSION,
        nats = %config.nats_url,
        journal_root = %config.journal_root.display(),
        repo = %config.default_repo,
        interval_secs = config.interval_secs,
        inactive_secs = config.inactive_secs,
        inactive_after_secs = config.inactive_after_secs,
        tick_secs = config.tick_secs,
        "starting",
    );

    let nats = JamNats::connect(&config.nats_url, config.nats_token.clone()).await?;
    info!("connected to NATS");

    let mut poller = Poller::new(config.clone());
    poller.load_journal();

    if cli.once {
        poller.poll_all(&nats, Utc::now()).await?;
        return Ok(());
    }

    let mut sub = nats
        .client()
        .subscribe("journal.pr.opened")
        .await
        .map_err(|err| PollerError::Subscribe(err.to_string()))?;
    info!(subject = "journal.pr.opened", "subscribed");

    let mut interval = time::interval(Duration::from_secs(config.tick_secs));
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
                poller.poll_due(&nats, Utc::now()).await?;
                if cli.max_ticks.is_some_and(|max_ticks| ticks >= max_ticks) {
                    info!(ticks, "max ticks reached");
                    return Ok(());
                }
            }
            msg = sub.next() => {
                let Some(message) = msg else {
                    warn!("pr.opened subscription closed");
                    return Ok(());
                };
                if let Err(err) = poller.handle_pr_opened(&message) {
                    warn!(subject = %message.subject, "ignored pr.opened event: {err}");
                }
            }
        }
    }
}

fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("jam_pr_poller=info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

#[derive(Debug)]
struct Poller {
    config: Config,
    records: HashMap<String, ActivePr>,
}

impl Poller {
    fn new(config: Config) -> Self {
        Self {
            config,
            records: HashMap::new(),
        }
    }

    fn load_journal(&mut self) {
        let files = journal_files(&self.config.journal_root);
        if files.is_empty() {
            info!(
                journal_root = %self.config.journal_root.display(),
                "no PR journal files found at startup",
            );
            return;
        }

        for path in files {
            self.load_journal_file(&path);
        }
        info!(active_prs = self.records.len(), "journal replay complete");
    }

    fn load_journal_file(&mut self, path: &Path) {
        let Ok(file) = File::open(path) else {
            warn!(path = %path.display(), "failed to open PR journal file");
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
            "pr.opened" => {
                if let Some(record) =
                    ActivePr::from_opened(envelope, self.config.default_repo.as_str())
                {
                    self.records.insert(record.pr_ref.clone(), record);
                }
            }
            "pr.status-changed" => {
                let Some(pr_ref) = value_string(&envelope.payload, "pr_ref") else {
                    return;
                };
                let Some(record) = self.records.get_mut(&pr_ref) else {
                    return;
                };
                let state =
                    value_string(&envelope.payload, "state").unwrap_or_else(|| "unknown".into());
                let draft = envelope
                    .payload
                    .get("draft")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                record.last_status = Some(PrStateSnapshot { state, draft });
            }
            "pr.ci.status-changed" => {
                let Some(pr_ref) = value_string(&envelope.payload, "pr_ref") else {
                    return;
                };
                let Some(status) = value_string(&envelope.payload, "ci_status") else {
                    return;
                };
                if let Some(record) = self.records.get_mut(&pr_ref) {
                    record.last_ci_status = Some(status);
                }
            }
            "pr.merged" => {
                if let Some(pr_ref) = value_string(&envelope.payload, "pr_ref") {
                    self.records.remove(&pr_ref);
                }
            }
            _ => {}
        }
    }

    fn handle_pr_opened(&mut self, message: &async_nats::Message) -> Result<(), PollerError> {
        if message
            .headers
            .as_ref()
            .and_then(jam_nats::extract_trace_from_headers)
            .is_none()
        {
            return Err(PollerError::Protocol(
                "journal.pr.opened arrived without Trace-Id headers".into(),
            ));
        }

        let envelope = serde_json::from_slice::<JournalEnvelope>(&message.payload)?;
        if envelope.event_type != "pr.opened" {
            return Err(PollerError::Protocol(format!(
                "expected pr.opened envelope, got {}",
                envelope.event_type
            )));
        }
        let Some(record) = ActivePr::from_opened(&envelope, self.config.default_repo.as_str())
        else {
            return Err(PollerError::Protocol(
                "pr.opened payload did not contain a parseable pr_ref".into(),
            ));
        };
        info!(
            pr_ref = %record.pr_ref,
            task_id = %record.task_id,
            repo = %record.repo,
            number = record.number,
            "tracking PR",
        );
        self.records.insert(record.pr_ref.clone(), record);
        Ok(())
    }

    async fn poll_due(&mut self, nats: &JamNats, now: DateTime<Utc>) -> Result<(), PollerError> {
        let due: Vec<String> = self
            .records
            .iter()
            .filter(|(_, record)| record.is_due(now))
            .map(|(pr_ref, _)| pr_ref.clone())
            .collect();
        self.poll_records(nats, now, due).await
    }

    async fn poll_all(&mut self, nats: &JamNats, now: DateTime<Utc>) -> Result<(), PollerError> {
        let records = self.records.keys().cloned().collect();
        self.poll_records(nats, now, records).await
    }

    async fn poll_records(
        &mut self,
        nats: &JamNats,
        now: DateTime<Utc>,
        records: Vec<String>,
    ) -> Result<(), PollerError> {
        for pr_ref in records {
            let Some(mut record) = self.records.remove(&pr_ref) else {
                continue;
            };
            if let Err(err) = self.poll_record(nats, &mut record, now).await {
                warn!(pr_ref = %record.pr_ref, "poll failed: {err}");
            }
            record.schedule_next(now, &self.config);
            if record.is_terminal() {
                info!(pr_ref = %record.pr_ref, "stopped polling terminal PR");
            } else {
                self.records.insert(record.pr_ref.clone(), record);
            }
        }
        Ok(())
    }

    async fn poll_record(
        &self,
        nats: &JamNats,
        record: &mut ActivePr,
        now: DateTime<Utc>,
    ) -> Result<(), PollerError> {
        let response = gh_api_pull(
            &self.config,
            &record.repo,
            record.number,
            record.etag.as_deref(),
        )
        .await?;
        record.polls_total = record.polls_total.saturating_add(1);
        if let Some(etag) = response.etag {
            record.etag = Some(etag);
        }

        match response.status_code {
            304 => {
                record.not_modified_total = record.not_modified_total.saturating_add(1);
                self.poll_ci_if_possible(nats, record, now).await?;
                Self::log_poll(record, "not-modified");
                Ok(())
            }
            200 => {
                let body = response.body.ok_or_else(|| {
                    PollerError::GitHub(format!(
                        "pulls/{}#{} returned 200 without JSON body",
                        record.repo, record.number
                    ))
                })?;
                let snapshot = PullSnapshot::from_value(body)?;
                self.observe_pull_snapshot(nats, record, &snapshot, now)
                    .await?;
                self.poll_ci_if_possible(nats, record, now).await?;
                Self::log_poll(record, "ok");
                Ok(())
            }
            code => Err(PollerError::GitHub(format!(
                "pulls/{}#{} returned HTTP {code}",
                record.repo, record.number
            ))),
        }
    }

    async fn observe_pull_snapshot(
        &self,
        nats: &JamNats,
        record: &mut ActivePr,
        snapshot: &PullSnapshot,
        now: DateTime<Utc>,
    ) -> Result<(), PollerError> {
        if snapshot.updated_at > record.last_activity_at {
            record.last_activity_at = snapshot.updated_at;
        }
        record.head_sha = Some(snapshot.head_sha.clone());
        record.last_updated_at = Some(snapshot.updated_at);

        let state = PrStateSnapshot {
            state: snapshot.state.clone(),
            draft: snapshot.draft,
        };
        if record.last_status.as_ref() != Some(&state) {
            let ctx = TraceCtx::new_root(
                "pr-poller.status-changed",
                format!("{} {} draft={}", record.pr_ref, state.state, state.draft),
            );
            let payload = PrStatusChanged {
                pr_ref: record.pr_ref.clone(),
                task_id: record.task_id.clone(),
                state: state.state.clone(),
                draft: state.draft,
                changed_at: now,
            };
            publish_journal_event(nats, payload, &ctx).await?;
            record.last_status = Some(state);
            record.last_activity_at = now;
        }

        if snapshot.merged {
            if let (Some(merged_sha), Some(merged_at)) =
                (snapshot.merge_commit_sha.as_ref(), snapshot.merged_at)
            {
                let touched_paths = self.touched_paths(record).await?;
                let ctx = TraceCtx::new_root(
                    "pr-poller.merged",
                    format!("{} merged {}", record.pr_ref, merged_sha),
                );
                let payload = PrMerged {
                    pr_ref: record.pr_ref.clone(),
                    task_id: record.task_id.clone(),
                    merged_sha: merged_sha.clone(),
                    merged_by: snapshot
                        .merged_by
                        .as_deref()
                        .unwrap_or("unknown")
                        .to_owned(),
                    merged_at,
                    touched_paths: touched_paths
                        .map(|paths| serde_json::to_string(&paths).unwrap_or_else(|_| "[]".into())),
                };
                publish_journal_event(nats, payload, &ctx).await?;
                record.last_activity_at = now;
            }
        }

        let review_count = snapshot.review_artifact_count();
        if let Some(previous) = record.last_review_count {
            if review_count > previous {
                let artifact_count = review_count.saturating_sub(previous);
                let ctx = TraceCtx::new_root(
                    "pr-poller.review-received",
                    format!("{} +{artifact_count} review artifact(s)", record.pr_ref),
                );
                let payload = PrReviewReceived {
                    pr_ref: record.pr_ref.clone(),
                    task_id: record.task_id.clone(),
                    reviewer: "github".into(),
                    artifact_count,
                    received_at: now,
                };
                publish_journal_event(nats, payload, &ctx).await?;
                record.last_activity_at = now;
            }
        }
        record.last_review_count = Some(review_count);

        // Auto-rebase BEHIND PRs once per head_sha. GitHub's auto-merge
        // doesn't fire while mergeable_state=behind; we have to call
        // `PUT /pulls/{n}/update-branch` to kick a merge of base into
        // head. Once that completes (asynchronously on GitHub's side)
        // the next poll either sees a new head_sha (we'll re-arm if we
        // somehow fall behind again) or mergeable_state=clean and
        // auto-merge fires.
        //
        // Dedupe on head_sha so we don't pile multiple update-branch
        // calls on the same revision while GitHub is busy.
        if snapshot.is_behind()
            && !snapshot.merged
            && snapshot.state == "open"
            && !snapshot.draft
            && record.last_update_branch_head.as_deref() != Some(&snapshot.head_sha)
        {
            match update_branch(&self.config, &record.repo, record.number).await {
                Ok(()) => {
                    record.last_update_branch_head = Some(snapshot.head_sha.clone());
                    let ctx = TraceCtx::new_root(
                        "pr-poller.branch-updated",
                        format!(
                            "{} update-branch requested for {}",
                            record.pr_ref, snapshot.head_sha
                        ),
                    );
                    let payload = PrBranchUpdated {
                        pr_ref: record.pr_ref.clone(),
                        task_id: record.task_id.clone(),
                        head_sha: snapshot.head_sha.clone(),
                        requested_at: now,
                    };
                    publish_journal_event(nats, payload, &ctx).await?;
                    record.last_activity_at = now;
                }
                Err(err) => {
                    warn!(
                        pr_ref = %record.pr_ref,
                        head_sha = %snapshot.head_sha,
                        error = %err,
                        "update-branch failed; will retry on next poll",
                    );
                }
            }
        }

        Ok(())
    }

    async fn touched_paths(&self, record: &ActivePr) -> Result<Option<Vec<String>>, PollerError> {
        let files: Vec<PullFile> = gh_api_json(
            &self.config,
            &format!("repos/{}/pulls/{}/files", record.repo, record.number),
        )
        .await?;
        if files.is_empty() {
            return Ok(None);
        }
        Ok(Some(files.into_iter().map(|file| file.filename).collect()))
    }

    async fn poll_ci_if_possible(
        &self,
        nats: &JamNats,
        record: &mut ActivePr,
        now: DateTime<Utc>,
    ) -> Result<(), PollerError> {
        let Some(head_sha) = record.head_sha.as_deref() else {
            return Ok(());
        };
        if record.ci_status_unavailable {
            return Ok(());
        }
        let status = match ci_status_for_commit(&self.config, &record.repo, head_sha).await {
            Ok(status) => status,
            Err(err) if is_github_permission_denied(&err) => {
                warn!(
                    pr_ref = %record.pr_ref,
                    "GitHub App cannot read PR checks/statuses; reporting CI as unknown"
                );
                record.ci_status_unavailable = true;
                CiObservation::Unknown
            }
            Err(err) => return Err(err),
        };
        if record.last_ci_status.as_deref() == Some(status.as_str()) {
            return Ok(());
        }

        let ctx = TraceCtx::new_root(
            "pr-poller.ci-status-changed",
            format!("{} CI {}", record.pr_ref, status.as_str()),
        );
        let payload = PrCiStatusChanged {
            pr_ref: record.pr_ref.clone(),
            task_id: record.task_id.clone(),
            ci_status: status.as_str().into(),
            changed_at: now,
        };
        publish_journal_event(nats, payload, &ctx).await?;
        record.last_ci_status = Some(status.as_str().into());
        record.last_activity_at = now;
        Ok(())
    }

    fn log_poll(record: &ActivePr, outcome: &str) {
        info!(
            pr_ref = %record.pr_ref,
            outcome,
            polls_total = record.polls_total,
            not_modified_total = record.not_modified_total,
            etag_304_rate = %record.etag_304_rate(),
            "polled PR",
        );
    }
}

#[derive(Debug, Clone)]
struct ActivePr {
    pr_ref: String,
    task_id: String,
    repo: String,
    number: u64,
    etag: Option<String>,
    head_sha: Option<String>,
    last_status: Option<PrStateSnapshot>,
    last_ci_status: Option<String>,
    ci_status_unavailable: bool,
    last_review_count: Option<u32>,
    last_updated_at: Option<DateTime<Utc>>,
    last_activity_at: DateTime<Utc>,
    next_poll_at: DateTime<Utc>,
    polls_total: u64,
    not_modified_total: u64,
    /// `head_sha` we last triggered `pulls/{n}/update-branch` for. Dedupes
    /// the rebase nudge so we don't fire it repeatedly while GitHub is
    /// rebasing (the call returns 202 the first time, then mergeable_state
    /// stays "behind" for a few seconds until the merge ref refreshes).
    last_update_branch_head: Option<String>,
}

impl ActivePr {
    fn from_opened(envelope: &JournalEnvelope, default_repo: &str) -> Option<Self> {
        let task_id = value_string(&envelope.payload, "task_id")?;
        let pr_ref = value_string(&envelope.payload, "pr_ref")?;
        let (repo, number) = parse_pr_ref(&pr_ref, default_repo)?;
        let opened_at =
            value_datetime(&envelope.payload, "opened_at").unwrap_or(envelope.timestamp);
        Some(Self {
            pr_ref,
            task_id,
            repo,
            number,
            etag: None,
            head_sha: None,
            last_status: None,
            last_ci_status: None,
            ci_status_unavailable: false,
            last_review_count: None,
            last_updated_at: None,
            last_activity_at: opened_at,
            next_poll_at: Utc::now(),
            polls_total: 0,
            not_modified_total: 0,
            last_update_branch_head: None,
        })
    }

    fn is_due(&self, now: DateTime<Utc>) -> bool {
        now >= self.next_poll_at
    }

    fn schedule_next(&mut self, now: DateTime<Utc>, config: &Config) {
        let inactive_cutoff = now - seconds_delta(config.inactive_after_secs);
        let cadence = if self.last_activity_at < inactive_cutoff {
            config.inactive_secs
        } else {
            config.interval_secs
        };
        self.next_poll_at = now + seconds_delta(cadence);
    }

    fn is_terminal(&self) -> bool {
        self.last_status
            .as_ref()
            .is_some_and(|status| status.state == "closed")
    }

    fn etag_304_rate(&self) -> String {
        if self.polls_total == 0 {
            return "0.000".into();
        }
        let per_mille = self
            .not_modified_total
            .saturating_mul(1_000)
            .saturating_div(self.polls_total);
        format!("{}.{:03}", per_mille / 1_000, per_mille % 1_000)
    }
}

fn seconds_delta(seconds: u64) -> TimeDelta {
    let seconds = i64::try_from(seconds).unwrap_or(i64::MAX);
    TimeDelta::seconds(seconds)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PrStateSnapshot {
    state: String,
    draft: bool,
}

#[derive(Debug, Deserialize)]
struct JournalEnvelope {
    event_type: String,
    timestamp: DateTime<Utc>,
    payload: Value,
}

fn journal_files(journal_root: &Path) -> Vec<PathBuf> {
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

fn parse_pr_ref(pr_ref: &str, default_repo: &str) -> Option<(String, u64)> {
    let (repo, number) = pr_ref.rsplit_once('#')?;
    let repo = if repo.is_empty() { default_repo } else { repo };
    if !repo.contains('/') {
        return None;
    }
    let number = number.parse().ok()?;
    Some((repo.to_owned(), number))
}

fn value_string(payload: &Value, field: &str) -> Option<String> {
    payload.get(field)?.as_str().map(ToOwned::to_owned)
}

fn value_datetime(payload: &Value, field: &str) -> Option<DateTime<Utc>> {
    let raw = payload.get(field)?.as_str()?;
    DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

#[derive(Debug)]
struct GhApiResponse {
    status_code: u16,
    etag: Option<String>,
    body: Option<Value>,
}

async fn gh_api_pull(
    config: &Config,
    repo: &str,
    number: u64,
    etag: Option<&str>,
) -> Result<GhApiResponse, PollerError> {
    let endpoint = format!("repos/{repo}/pulls/{number}");
    let mut command = Command::new(&config.gh_bin);
    command
        .arg("api")
        .arg("-i")
        .arg(&endpoint)
        .arg("--method")
        .arg("GET")
        .arg("-H")
        .arg("Accept: application/vnd.github+json");
    if let Some(etag) = etag {
        command.arg("-H").arg(format!("If-None-Match: {etag}"));
    }
    if let Some(token) = github_app_installation_token(config).await? {
        command.env("GH_TOKEN", token.expose_secret());
    }
    let output = command
        .output()
        .await
        .map_err(|err| PollerError::GitHub(format!("{endpoint}: {err}")))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let response = parse_gh_api_response(&stdout)?;
    if !output.status.success() && response.status_code != 304 {
        return Err(PollerError::GitHub(command_failure(&endpoint, &output)));
    }
    Ok(response)
}

/// Ask GitHub to update the PR's head branch by merging the latest base into
/// it. Idempotent on GitHub's side; returns 202 on success, 422 if the PR is
/// already up to date or not mergeable. We treat anything in [200,300) as
/// success and surface other codes for the caller to log + retry next poll.
async fn update_branch(config: &Config, repo: &str, number: u64) -> Result<(), PollerError> {
    let endpoint = format!("repos/{repo}/pulls/{number}/update-branch");
    let mut command = Command::new(&config.gh_bin);
    command
        .arg("api")
        .arg(&endpoint)
        .arg("--method")
        .arg("PUT")
        .arg("-H")
        .arg("Accept: application/vnd.github+json");
    // update-branch requires `Pull requests: write`. The GitHub App
    // installation token already has that scope in our setup, and the
    // poller never holds a user token. If the App auth isn't configured,
    // fall through unauthenticated — the gh CLI's $GH_TOKEN from the
    // env will pick up whatever's there.
    if let Some(token) = github_app_installation_token(config).await? {
        command.env("GH_TOKEN", token.expose_secret());
    }
    let output = command
        .output()
        .await
        .map_err(|err| PollerError::GitHub(format!("{endpoint}: {err}")))?;
    if !output.status.success() {
        return Err(PollerError::GitHub(command_failure(&endpoint, &output)));
    }
    Ok(())
}

async fn gh_api_json<T>(config: &Config, endpoint: &str) -> Result<T, PollerError>
where
    T: for<'de> Deserialize<'de>,
{
    let mut command = Command::new(&config.gh_bin);
    command
        .arg("api")
        .arg(endpoint)
        .arg("--method")
        .arg("GET")
        .arg("-H")
        .arg("Accept: application/vnd.github+json");
    if let Some(token) = github_app_installation_token(config).await? {
        command.env("GH_TOKEN", token.expose_secret());
    }
    let output = command
        .output()
        .await
        .map_err(|err| PollerError::GitHub(format!("{endpoint}: {err}")))?;
    if !output.status.success() {
        return Err(PollerError::GitHub(command_failure(endpoint, &output)));
    }
    serde_json::from_slice(&output.stdout).map_err(PollerError::from)
}

fn command_failure(endpoint: &str, output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if stderr.is_empty() {
        format!("{endpoint}: {stdout}")
    } else {
        format!("{endpoint}: {stderr}")
    }
}

fn parse_gh_api_response(raw: &str) -> Result<GhApiResponse, PollerError> {
    let normalized = raw.replace("\r\n", "\n");
    let (headers, body) = normalized
        .rsplit_once("\n\n")
        .ok_or_else(|| PollerError::GitHub("gh api -i response had no header/body split".into()))?;
    let status_code = headers
        .lines()
        .filter_map(parse_status_line)
        .next_back()
        .ok_or_else(|| PollerError::GitHub("gh api -i response had no HTTP status".into()))?;
    let etag = headers.lines().filter_map(parse_etag_header).next_back();
    let body = if body.trim().is_empty() {
        None
    } else {
        Some(serde_json::from_str(body.trim())?)
    };
    Ok(GhApiResponse {
        status_code,
        etag,
        body,
    })
}

fn parse_status_line(line: &str) -> Option<u16> {
    if !line.starts_with("HTTP/") {
        return None;
    }
    line.split_whitespace().nth(1)?.parse().ok()
}

fn parse_etag_header(line: &str) -> Option<String> {
    let (name, value) = line.split_once(':')?;
    if name.eq_ignore_ascii_case("etag") {
        Some(value.trim().to_owned())
    } else {
        None
    }
}

#[derive(Debug)]
struct PullSnapshot {
    state: String,
    draft: bool,
    merged: bool,
    merged_at: Option<DateTime<Utc>>,
    merged_by: Option<String>,
    merge_commit_sha: Option<String>,
    updated_at: DateTime<Utc>,
    head_sha: String,
    comments: u32,
    review_comments: u32,
    /// GitHub's `mergeable_state` ("clean", "behind", "blocked", "dirty",
    /// "draft", "unknown", "unstable"). We act on "behind" — the PR's head
    /// branch is out of date with its base — by calling update-branch so
    /// auto-merge can fire once the rebase + CI complete.
    mergeable_state: Option<String>,
}

impl PullSnapshot {
    fn from_value(value: Value) -> Result<Self, PollerError> {
        let response: PullApiResponse = serde_json::from_value(value)?;
        Ok(Self {
            state: response.state.to_ascii_lowercase(),
            draft: response.draft,
            merged: response.merged,
            merged_at: response.merged_at,
            merged_by: response.merged_by.map(|user| user.login),
            merge_commit_sha: response.merge_commit_sha,
            updated_at: response.updated_at,
            head_sha: response.head.sha,
            comments: response.comments,
            review_comments: response.review_comments,
            mergeable_state: response.mergeable_state.map(|s| s.to_ascii_lowercase()),
        })
    }

    fn review_artifact_count(&self) -> u32 {
        self.comments.saturating_add(self.review_comments)
    }

    /// True when GitHub reports the PR's head branch is behind its base.
    /// auto-merge can't fire in this state; calling update-branch puts the
    /// PR back into a mergeable state once the rebase + CI complete.
    fn is_behind(&self) -> bool {
        self.mergeable_state.as_deref() == Some("behind")
    }
}

#[derive(Debug, Deserialize)]
struct PullApiResponse {
    state: String,
    draft: bool,
    #[serde(default)]
    merged: bool,
    merged_at: Option<DateTime<Utc>>,
    merged_by: Option<GhUser>,
    merge_commit_sha: Option<String>,
    updated_at: DateTime<Utc>,
    head: PullHead,
    #[serde(default)]
    comments: u32,
    #[serde(default)]
    review_comments: u32,
    #[serde(default)]
    mergeable_state: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PullHead {
    sha: String,
}

#[derive(Debug, Deserialize)]
struct GhUser {
    login: String,
}

#[derive(Debug, Deserialize)]
struct PullFile {
    filename: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CiObservation {
    Unknown,
    Success,
    Running,
    Cancelled,
    Failure,
}

impl CiObservation {
    fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Success => "success",
            Self::Running => "running",
            Self::Cancelled => "cancelled",
            Self::Failure => "failure",
        }
    }

    fn severity(self) -> u8 {
        match self {
            Self::Unknown => 0,
            Self::Success => 1,
            Self::Running => 2,
            Self::Cancelled => 3,
            Self::Failure => 4,
        }
    }

    fn max(self, other: Self) -> Self {
        if other.severity() > self.severity() {
            other
        } else {
            self
        }
    }
}

async fn ci_status_for_commit(
    config: &Config,
    repo: &str,
    sha: &str,
) -> Result<CiObservation, PollerError> {
    let combined: CombinedStatusResponse =
        gh_api_json(config, &format!("repos/{repo}/commits/{sha}/status")).await?;
    let check_runs: CheckRunsResponse =
        gh_api_json(config, &format!("repos/{repo}/commits/{sha}/check-runs")).await?;
    Ok(combine_ci([
        status_from_combined(&combined),
        status_from_check_runs(&check_runs),
    ]))
}

fn is_github_permission_denied(err: &PollerError) -> bool {
    matches!(
        err,
        PollerError::GitHub(detail)
            if detail.contains("Resource not accessible by integration")
                || detail.contains("HTTP 403")
    )
}

fn combine_ci(observations: impl IntoIterator<Item = CiObservation>) -> CiObservation {
    observations
        .into_iter()
        .fold(CiObservation::Unknown, CiObservation::max)
}

#[derive(Debug, Deserialize)]
struct CombinedStatusResponse {
    state: String,
    total_count: u32,
}

fn status_from_combined(response: &CombinedStatusResponse) -> CiObservation {
    if response.total_count == 0 {
        return CiObservation::Unknown;
    }
    match response.state.as_str() {
        "success" => CiObservation::Success,
        "pending" => CiObservation::Running,
        "failure" | "error" => CiObservation::Failure,
        _ => CiObservation::Unknown,
    }
}

#[derive(Debug, Deserialize)]
struct CheckRunsResponse {
    #[serde(default)]
    check_runs: Vec<CheckRun>,
}

#[derive(Debug, Deserialize)]
struct CheckRun {
    status: String,
    conclusion: Option<String>,
}

fn status_from_check_runs(response: &CheckRunsResponse) -> CiObservation {
    if response.check_runs.is_empty() {
        return CiObservation::Unknown;
    }
    combine_ci(response.check_runs.iter().map(status_from_check_run))
}

fn status_from_check_run(run: &CheckRun) -> CiObservation {
    if run.status != "completed" {
        return CiObservation::Running;
    }
    match run.conclusion.as_deref() {
        Some("success" | "neutral" | "skipped") => CiObservation::Success,
        Some("cancelled") => CiObservation::Cancelled,
        Some("failure" | "timed_out" | "action_required" | "startup_failure" | "stale") => {
            CiObservation::Failure
        }
        _ => CiObservation::Unknown,
    }
}

async fn publish_journal_event<P>(
    nats: &JamNats,
    payload: P,
    ctx: &TraceCtx,
) -> Result<(), PollerError>
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
    fn pull_snapshot_is_behind_when_mergeable_state_behind() {
        let value: serde_json::Value = serde_json::from_str(
            r#"{
                "state": "open", "draft": false, "merged": false,
                "updated_at": "2026-05-17T00:00:00Z",
                "head": {"sha": "abc"},
                "mergeable_state": "behind"
            }"#,
        )
        .unwrap();
        let snap = PullSnapshot::from_value(value).unwrap();
        assert!(snap.is_behind());
        assert_eq!(snap.head_sha, "abc");
    }

    #[test]
    fn pull_snapshot_is_not_behind_for_clean_state() {
        for state in ["clean", "blocked", "dirty", "unknown", "unstable", "draft"] {
            let value: serde_json::Value = serde_json::from_str(&format!(
                r#"{{
                    "state": "open", "draft": false, "merged": false,
                    "updated_at": "2026-05-17T00:00:00Z",
                    "head": {{"sha": "abc"}},
                    "mergeable_state": "{state}"
                }}"#
            ))
            .unwrap();
            let snap = PullSnapshot::from_value(value).unwrap();
            assert!(!snap.is_behind(), "state={state}");
        }
    }

    #[test]
    fn pull_snapshot_handles_missing_mergeable_state() {
        // Older GitHub Enterprise versions or certain edge cases omit the
        // field entirely; default to "not behind" so we don't fire
        // update-branch with an unknown view of the PR.
        let value: serde_json::Value = serde_json::from_str(
            r#"{
                "state": "open", "draft": false, "merged": false,
                "updated_at": "2026-05-17T00:00:00Z",
                "head": {"sha": "abc"}
            }"#,
        )
        .unwrap();
        let snap = PullSnapshot::from_value(value).unwrap();
        assert!(!snap.is_behind());
    }

    #[test]
    fn parses_gh_api_200_response() {
        let raw = concat!(
            "HTTP/2.0 200 OK\n",
            "Date: Wed, 06 May 2026 06:50:10 GMT\n",
            "Etag: W/\"abc123\"\n",
            "\n",
            "{\"state\":\"open\"}\n",
        );
        let parsed = parse_gh_api_response(raw).unwrap();

        assert_eq!(parsed.status_code, 200);
        assert_eq!(parsed.etag.as_deref(), Some("W/\"abc123\""));
        assert_eq!(parsed.body.unwrap()["state"], "open");
    }

    #[test]
    fn parses_gh_api_304_response() {
        let raw = concat!(
            "HTTP/2.0 304 Not Modified\n",
            "Date: Wed, 06 May 2026 06:50:10 GMT\n",
            "Etag: \"abc123\"\n",
            "\n",
        );
        let parsed = parse_gh_api_response(raw).unwrap();

        assert_eq!(parsed.status_code, 304);
        assert_eq!(parsed.etag.as_deref(), Some("\"abc123\""));
        assert!(parsed.body.is_none());
    }

    #[test]
    fn parses_pr_ref() {
        assert_eq!(
            parse_pr_ref("cleak/blueberry#383", "fallback/repo"),
            Some(("cleak/blueberry".into(), 383))
        );
        assert_eq!(
            parse_pr_ref("#42", "fallback/repo"),
            Some(("fallback/repo".into(), 42))
        );
        assert!(parse_pr_ref("not-a-ref", "fallback/repo").is_none());
    }

    #[test]
    fn schedules_inactive_prs_at_slow_cadence() {
        let envelope = JournalEnvelope {
            event_type: "pr.opened".into(),
            timestamp: ts("2026-05-06T06:00:00Z"),
            payload: serde_json::json!({
                "task_id": "task-1",
                "pr_ref": "cleak/blueberry#383",
                "opened_at": "2026-05-06T06:00:00Z"
            }),
        };
        let mut record = ActivePr::from_opened(&envelope, "cleak/blueberry").unwrap();
        let config = test_config(30, 300, 1_800);
        let now = ts("2026-05-06T07:00:00Z");

        record.schedule_next(now, &config);

        assert_eq!(record.next_poll_at, now + TimeDelta::seconds(300));
    }

    #[test]
    fn combines_ci_status_by_severity() {
        assert_eq!(
            combine_ci([
                CiObservation::Success,
                CiObservation::Running,
                CiObservation::Unknown,
            ]),
            CiObservation::Running
        );
        assert_eq!(
            combine_ci([CiObservation::Cancelled, CiObservation::Failure]),
            CiObservation::Failure
        );
        let running = CheckRun {
            status: "in_progress".into(),
            conclusion: None,
        };
        assert_eq!(status_from_check_run(&running), CiObservation::Running);
    }

    #[test]
    fn replays_pr_opened_from_journal() {
        let tmp = TempDir::new().unwrap();
        let day = tmp.path().join("2026-05-06");
        std::fs::create_dir_all(&day).unwrap();
        std::fs::write(
            day.join("journal.pr.jsonl"),
            serde_json::json!({
                "event_type": "pr.opened",
                "timestamp": "2026-05-06T06:00:00Z",
                "payload": {
                    "task_id": "task-1",
                    "pr_ref": "cleak/blueberry#383",
                    "opened_at": "2026-05-06T06:00:00Z"
                }
            })
            .to_string()
                + "\n",
        )
        .unwrap();
        let mut poller = Poller::new(test_config_with_journal(tmp.path().to_path_buf()));

        poller.load_journal();

        assert!(poller.records.contains_key("cleak/blueberry#383"));
    }

    fn test_config(interval_secs: u64, inactive_secs: u64, inactive_after_secs: u64) -> Config {
        Config {
            nats_url: "nats://127.0.0.1:4222".into(),
            nats_token: None,
            journal_root: PathBuf::from("/tmp/missing-jam-pr-poller-test"),
            gh_bin: PathBuf::from("gh"),
            default_repo: "cleak/blueberry".into(),
            github_app: None,
            interval_secs,
            inactive_secs,
            inactive_after_secs,
            tick_secs: 1,
        }
    }

    fn test_config_with_journal(journal_root: PathBuf) -> Config {
        Config {
            journal_root,
            ..test_config(30, 300, 1_800)
        }
    }

    fn ts(raw: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(raw)
            .unwrap()
            .with_timezone(&Utc)
    }
}
