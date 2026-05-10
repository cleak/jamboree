//! `jam-skill-suspicion` - detects stale or harmful skills from Tempyr dead ends.
//!
//! This reconciler is intentionally cheap and deterministic. It queries the
//! Tempyr journal for recent `dead_end` entries, counts `skill:<scope>` tags,
//! and emits `evolve.skill-under-suspicion` when a skill crosses the
//! 3-in-7-days threshold from §22.6. It does not mutate skills or quarantine
//! anything; the Maestro decides what to do on the next wake.

#![deny(missing_docs)]

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use clap::Parser;
use jam_events::generated::{Event, EvolveSkillUnderSuspicion};
use jam_events::EventEnvelope;
use jam_nats::JamNats;
use jam_trace::TraceCtx;
use serde_json::Value;
use tokio::process::Command;
use tokio::time::{self, Duration};
use tracing::{error, info};

const SERVICE_NAME: &str = "jam-skill-suspicion";
const SERVICE_VERSION: &str = env!("CARGO_PKG_VERSION");
const DEFAULT_INTERVAL_SECS: u64 = 3_600;
const DEFAULT_SINCE_DAYS: u32 = 7;
const DEFAULT_THRESHOLD: u32 = 3;
const DEFAULT_LIMIT: u32 = 200;
const DEFAULT_TEMPYR_BIN: &str = "tempyr";

#[derive(Debug, Parser)]
#[command(
    name = SERVICE_NAME,
    version,
    about = "Emit skill-under-suspicion from Tempyr dead_end accumulation"
)]
struct Cli {
    /// Check cadence in seconds.
    #[arg(long)]
    interval_secs: Option<u64>,

    /// Count dead_end journal entries newer than this many days.
    #[arg(long)]
    since_days: Option<u32>,

    /// Number of dead_end entries required before emitting suspicion.
    #[arg(long)]
    threshold: Option<u32>,

    /// Maximum Tempyr search results to inspect.
    #[arg(long)]
    limit: Option<u32>,

    /// Tempyr binary path.
    #[arg(long)]
    tempyr_bin: Option<PathBuf>,

    /// Check once, then exit.
    #[arg(long)]
    once: bool,

    /// Stop after this many check ticks; useful for smoke tests.
    #[arg(long)]
    max_ticks: Option<u64>,
}

#[derive(Debug, thiserror::Error)]
enum SkillSuspicionError {
    #[error("nats: {0}")]
    Nats(#[from] jam_nats::NatsError),

    #[error("tempyr: {0}")]
    Tempyr(String),

    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone)]
struct Config {
    nats_url: String,
    nats_token: Option<String>,
    tempyr_bin: PathBuf,
    interval_secs: u64,
    since_days: u32,
    threshold: u32,
    limit: u32,
}

impl Config {
    fn from_env_and_cli(cli: &Cli) -> Self {
        Self {
            nats_url: std::env::var("NATS_URL").unwrap_or_else(|_| "nats://127.0.0.1:4222".into()),
            nats_token: std::env::var("NATS_TOKEN").ok(),
            tempyr_bin: cli
                .tempyr_bin
                .clone()
                .or_else(|| std::env::var_os("JAM_TEMPYR_BIN").map(PathBuf::from))
                .unwrap_or_else(|| PathBuf::from(DEFAULT_TEMPYR_BIN)),
            interval_secs: cli.interval_secs.unwrap_or_else(|| {
                env_parse("JAM_SKILL_SUSPICION_INTERVAL_SECS").unwrap_or(DEFAULT_INTERVAL_SECS)
            }),
            since_days: cli.since_days.unwrap_or_else(|| {
                env_parse("JAM_SKILL_SUSPICION_SINCE_DAYS").unwrap_or(DEFAULT_SINCE_DAYS)
            }),
            threshold: cli.threshold.unwrap_or_else(|| {
                env_parse("JAM_SKILL_SUSPICION_THRESHOLD").unwrap_or(DEFAULT_THRESHOLD)
            }),
            limit: cli
                .limit
                .unwrap_or_else(|| env_parse("JAM_SKILL_SUSPICION_LIMIT").unwrap_or(DEFAULT_LIMIT)),
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
        error!("jam-skill-suspicion fatal: {err}");
        return std::process::ExitCode::FAILURE;
    }
    std::process::ExitCode::SUCCESS
}

async fn run() -> Result<(), SkillSuspicionError> {
    init_tracing();
    let cli = Cli::parse();
    let config = Config::from_env_and_cli(&cli);

    info!(
        service = %SERVICE_NAME,
        version = %SERVICE_VERSION,
        nats = %config.nats_url,
        tempyr = %config.tempyr_bin.display(),
        interval_secs = config.interval_secs,
        since_days = config.since_days,
        threshold = config.threshold,
        limit = config.limit,
        "starting",
    );

    let nats = JamNats::connect(&config.nats_url, config.nats_token.clone()).await?;
    info!("connected to NATS");

    let mut emitted = HashSet::new();
    if cli.once {
        check_once(&nats, &config, &mut emitted, Utc::now()).await?;
        return Ok(());
    }

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
                check_once(&nats, &config, &mut emitted, Utc::now()).await?;
                if cli.max_ticks.is_some_and(|max_ticks| ticks >= max_ticks) {
                    info!(ticks, "max ticks reached");
                    return Ok(());
                }
            }
        }
    }
}

fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("jam_skill_suspicion=info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

async fn check_once(
    nats: &JamNats,
    config: &Config,
    emitted: &mut HashSet<String>,
    detected_at: DateTime<Utc>,
) -> Result<(), SkillSuspicionError> {
    let raw = tempyr_dead_ends(config).await?;
    let suspicions = suspicions_from_search_json(
        &raw,
        SuspicionConfig {
            threshold: config.threshold,
        },
    )?;
    for suspicion in suspicions {
        let fingerprint = suspicion.fingerprint();
        if !emitted.insert(fingerprint) {
            continue;
        }
        publish_suspicion(nats, suspicion, config.since_days, detected_at).await?;
    }
    Ok(())
}

async fn tempyr_dead_ends(config: &Config) -> Result<String, SkillSuspicionError> {
    let output = Command::new(&config.tempyr_bin)
        .args([
            "journal",
            "search",
            "--json",
            "--kind",
            "dead_end",
            "--since-days",
            &config.since_days.to_string(),
            "--limit",
            &config.limit.to_string(),
            "*",
        ])
        .output()
        .await
        .map_err(|err| SkillSuspicionError::Tempyr(err.to_string()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        let detail = if stderr.is_empty() { stdout } else { stderr };
        return Err(SkillSuspicionError::Tempyr(detail));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

async fn publish_suspicion(
    nats: &JamNats,
    suspicion: SkillSuspicion,
    since_days: u32,
    detected_at: DateTime<Utc>,
) -> Result<(), SkillSuspicionError> {
    let skill_scope = suspicion.skill_scope.clone();
    let dead_end_count = u32::try_from(suspicion.entry_ids.len()).unwrap_or(u32::MAX);
    let implicating_traces = suspicion.implicating_traces_json();
    let ctx = TraceCtx::new_root(
        "skill-suspicion.reconciled",
        format!("{skill_scope} has {dead_end_count} dead_end entries in {since_days}d"),
    );
    let payload = EvolveSkillUnderSuspicion {
        skill_scope: skill_scope.clone(),
        dead_end_count,
        since_days,
        implicating_traces,
        ts: detected_at,
    };
    let envelope = EventEnvelope::new(
        EvolveSkillUnderSuspicion::EVENT_TYPE,
        EvolveSkillUnderSuspicion::EVENT_SUBTYPE_VERSION,
        0,
        ctx.trace_id.to_string(),
        SERVICE_NAME,
        payload,
    );
    nats.publish_traced(
        format!("journal.{}", EvolveSkillUnderSuspicion::EVENT_TYPE),
        &envelope,
        &ctx,
    )
    .await?;
    info!(
        skill_scope = %skill_scope,
        dead_end_count,
        "published evolve.skill-under-suspicion",
    );
    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct SuspicionConfig {
    threshold: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SkillSuspicion {
    skill_scope: String,
    entry_ids: Vec<String>,
    trace_ids: Vec<String>,
}

impl SkillSuspicion {
    fn fingerprint(&self) -> String {
        format!("{}:{}", self.skill_scope, self.entry_ids.join(","))
    }

    fn implicating_traces_json(&self) -> Option<String> {
        if self.trace_ids.is_empty() {
            return None;
        }
        serde_json::to_string(&self.trace_ids).ok()
    }
}

fn suspicions_from_search_json(
    raw: &str,
    config: SuspicionConfig,
) -> Result<Vec<SkillSuspicion>, SkillSuspicionError> {
    let parsed: Value = serde_json::from_str(raw)?;
    let hits = parsed
        .get("hits")
        .and_then(Value::as_array)
        .ok_or_else(|| SkillSuspicionError::Tempyr("journal search JSON missing hits[]".into()))?;
    let mut by_skill: HashMap<String, SkillAccumulator> = HashMap::new();
    for hit in hits {
        let entry_id = hit_id(hit);
        let trace_id = hit_trace_id(hit);
        for tag in hit_tags(hit) {
            let Some(skill_scope) = tag.strip_prefix("skill:") else {
                continue;
            };
            let accumulator = by_skill.entry(skill_scope.to_owned()).or_default();
            accumulator.entry_ids.insert(entry_id.clone());
            if let Some(trace) = &trace_id {
                accumulator.trace_ids.insert(trace.clone());
            }
        }
    }

    let mut suspicions: Vec<_> = by_skill
        .into_iter()
        .filter_map(|(skill_scope, accumulator)| {
            let mut entry_ids: Vec<_> = accumulator.entry_ids.into_iter().collect();
            entry_ids.sort();
            if entry_ids.len() < config.threshold as usize {
                return None;
            }
            let mut trace_ids: Vec<_> = accumulator.trace_ids.into_iter().collect();
            trace_ids.sort();
            Some(SkillSuspicion {
                skill_scope,
                entry_ids,
                trace_ids,
            })
        })
        .collect();
    suspicions.sort_by(|left, right| left.skill_scope.cmp(&right.skill_scope));
    Ok(suspicions)
}

#[derive(Debug, Default)]
struct SkillAccumulator {
    entry_ids: HashSet<String>,
    trace_ids: HashSet<String>,
}

fn hit_id(hit: &Value) -> String {
    string_field(hit, "id")
        .or_else(|| string_field(hit, "entry_id"))
        .unwrap_or_else(|| serde_json::to_string(hit).unwrap_or_else(|_| "unknown-entry".into()))
}

fn hit_trace_id(hit: &Value) -> Option<String> {
    string_field(hit, "trace_id").or_else(|| {
        hit_tags(hit).into_iter().find_map(|tag| {
            tag.strip_prefix("trace:")
                .map(std::borrow::ToOwned::to_owned)
        })
    })
}

fn hit_tags(hit: &Value) -> Vec<String> {
    let Some(tags) = hit.get("tags") else {
        return Vec::new();
    };
    if let Some(raw) = tags.as_str() {
        return raw
            .split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(std::borrow::ToOwned::to_owned)
            .collect();
    }
    tags.as_array().map_or_else(Vec::new, |items| {
        items
            .iter()
            .filter_map(Value::as_str)
            .map(std::borrow::ToOwned::to_owned)
            .collect()
    })
}

fn string_field(value: &Value, key: &str) -> Option<String> {
    value.get(key)?.as_str().map(std::borrow::ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn detects_skill_over_threshold() {
        let raw = json!({
            "count": 4,
            "hits": [
                {"id": "a", "trace_id": "trace-a", "tags": ["skill:blueberry/hot-paths"]},
                {"id": "b", "trace_id": "trace-b", "tags": ["skill:blueberry/hot-paths", "other"]},
                {"id": "c", "trace_id": "trace-c", "tags": ["skill:blueberry/hot-paths"]},
                {"id": "d", "trace_id": "trace-d", "tags": ["skill:blueberry/ok-skill"]}
            ],
            "query": "*"
        })
        .to_string();

        let suspicions =
            suspicions_from_search_json(&raw, SuspicionConfig { threshold: 3 }).unwrap();

        assert_eq!(suspicions.len(), 1);
        assert_eq!(suspicions[0].skill_scope, "blueberry/hot-paths");
        assert_eq!(suspicions[0].entry_ids, ["a", "b", "c"]);
        assert_eq!(suspicions[0].trace_ids, ["trace-a", "trace-b", "trace-c"]);
        assert_eq!(
            suspicions[0].implicating_traces_json(),
            Some(r#"["trace-a","trace-b","trace-c"]"#.into()),
        );
    }

    #[test]
    fn accepts_comma_separated_tags_and_trace_tag_fallback() {
        let raw = json!({
            "hits": [
                {"id": "a", "tags": "skill:blueberry/coderabbit, trace:01HXKJ00000000000000000000"},
                {"id": "b", "tags": "skill:blueberry/coderabbit"},
                {"id": "c", "tags": "skill:blueberry/coderabbit"}
            ]
        })
        .to_string();

        let suspicions =
            suspicions_from_search_json(&raw, SuspicionConfig { threshold: 3 }).unwrap();

        assert_eq!(suspicions[0].skill_scope, "blueberry/coderabbit");
        assert_eq!(suspicions[0].trace_ids, ["01HXKJ00000000000000000000"]);
    }

    #[test]
    fn requires_hits_array() {
        let err = suspicions_from_search_json("{}", SuspicionConfig { threshold: 3 }).unwrap_err();

        assert!(err.to_string().contains("missing hits"));
    }
}
