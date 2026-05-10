//! `jam-clock-watcher` - verifies host NTP synchronization (§4.4.6, §21.3).
//!
//! The watcher is a deterministic reconciler: it checks `timedatectl`, emits
//! `clock.unsynced` when synchronization is false, and leaves policy decisions
//! to the Maestro or notification layer.

#![deny(missing_docs)]

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use clap::Parser;
use jam_events::generated::{ClockUnsynced, Event};
use jam_events::EventEnvelope;
use jam_nats::JamNats;
use jam_trace::TraceCtx;
use tokio::process::Command;
use tokio::time::{self, Duration};
use tracing::{error, info};

const SERVICE_NAME: &str = "jam-clock-watcher";
const SERVICE_VERSION: &str = env!("CARGO_PKG_VERSION");
const DEFAULT_INTERVAL_SECS: u64 = 600;
const DEFAULT_DRIFT_THRESHOLD_MS: u64 = 1_000;

#[derive(Debug, Parser)]
#[command(name = SERVICE_NAME, version, about = "Verify NTP synchronization")]
struct Cli {
    /// Check cadence in seconds.
    #[arg(long)]
    interval_secs: Option<u64>,

    /// Drift threshold in milliseconds; unsynced clocks report threshold + 1.
    #[arg(long)]
    drift_threshold_ms: Option<u64>,

    /// timedatectl binary path.
    #[arg(long)]
    timedatectl_bin: Option<PathBuf>,

    /// Check once, then exit.
    #[arg(long)]
    once: bool,

    /// Stop after this many check ticks; useful for smoke tests.
    #[arg(long)]
    max_ticks: Option<u64>,
}

#[derive(Debug, thiserror::Error)]
enum ClockError {
    #[error("nats: {0}")]
    Nats(#[from] jam_nats::NatsError),

    #[error("timedatectl: {0}")]
    Timedatectl(String),

    #[error("protocol: {0}")]
    Protocol(String),
}

#[derive(Debug, Clone)]
struct Config {
    nats_url: String,
    nats_token: Option<String>,
    timedatectl_bin: PathBuf,
    interval_secs: u64,
    drift_threshold_ms: u64,
}

impl Config {
    fn from_env_and_cli(cli: &Cli) -> Self {
        Self {
            nats_url: std::env::var("NATS_URL").unwrap_or_else(|_| "nats://127.0.0.1:4222".into()),
            nats_token: std::env::var("NATS_TOKEN").ok(),
            timedatectl_bin: cli
                .timedatectl_bin
                .clone()
                .or_else(|| std::env::var_os("JAM_TIMEDATECTL_BIN").map(PathBuf::from))
                .unwrap_or_else(|| PathBuf::from("timedatectl")),
            interval_secs: cli.interval_secs.unwrap_or_else(|| {
                env_parse("JAM_CLOCK_WATCH_INTERVAL_SECS").unwrap_or(DEFAULT_INTERVAL_SECS)
            }),
            drift_threshold_ms: cli.drift_threshold_ms.unwrap_or_else(|| {
                env_parse("JAM_CLOCK_DRIFT_THRESHOLD_MS").unwrap_or(DEFAULT_DRIFT_THRESHOLD_MS)
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
        error!("jam-clock-watcher fatal: {err}");
        return std::process::ExitCode::FAILURE;
    }
    std::process::ExitCode::SUCCESS
}

async fn run() -> Result<(), ClockError> {
    init_tracing();
    let cli = Cli::parse();
    let config = Config::from_env_and_cli(&cli);

    info!(
        service = %SERVICE_NAME,
        version = %SERVICE_VERSION,
        nats = %config.nats_url,
        timedatectl = %config.timedatectl_bin.display(),
        interval_secs = config.interval_secs,
        drift_threshold_ms = config.drift_threshold_ms,
        "starting",
    );

    let nats = JamNats::connect(&config.nats_url, config.nats_token.clone()).await?;
    info!("connected to NATS");

    if cli.once {
        check_once(&nats, &config, Utc::now()).await?;
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
                check_once(&nats, &config, Utc::now()).await?;
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
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("jam_clock_watcher=info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

async fn check_once(
    nats: &JamNats,
    config: &Config,
    checked_at: DateTime<Utc>,
) -> Result<(), ClockError> {
    let output = timedatectl_show(&config.timedatectl_bin).await?;
    let status = parse_timedatectl_sync(&output)?;
    if status.ntp_synchronized && status.system_clock_synchronized.unwrap_or(true) {
        info!("clock synchronized");
        return Ok(());
    }

    let drift_ms = config.drift_threshold_ms.saturating_add(1);
    let ctx = TraceCtx::new_root(
        "clock-watcher.unsynced",
        format!("timedatectl reported unsynchronized clock; drift_ms={drift_ms}"),
    );
    let payload = ClockUnsynced {
        drift_ms,
        ts: checked_at,
    };
    let envelope = EventEnvelope::new(
        ClockUnsynced::EVENT_TYPE,
        ClockUnsynced::EVENT_SUBTYPE_VERSION,
        0,
        ctx.trace_id.to_string(),
        SERVICE_NAME,
        payload,
    );
    nats.publish_traced("journal.clock.unsynced", &envelope, &ctx)
        .await?;
    info!(drift_ms, "published clock.unsynced");
    Ok(())
}

async fn timedatectl_show(timedatectl_bin: &PathBuf) -> Result<String, ClockError> {
    let output = Command::new(timedatectl_bin)
        .args([
            "show",
            "-p",
            "NTPSynchronized",
            "-p",
            "SystemClockSynchronized",
        ])
        .output()
        .await
        .map_err(|err| ClockError::Timedatectl(err.to_string()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        let detail = if stderr.is_empty() { stdout } else { stderr };
        return Err(ClockError::Timedatectl(detail));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ClockSyncStatus {
    ntp_synchronized: bool,
    system_clock_synchronized: Option<bool>,
}

fn parse_timedatectl_sync(output: &str) -> Result<ClockSyncStatus, ClockError> {
    let mut ntp = None;
    let mut system = None;
    for line in output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key {
            "NTPSynchronized" => ntp = Some(parse_yes_no(value)?),
            "SystemClockSynchronized" => system = Some(parse_yes_no(value)?),
            _ => {}
        }
    }
    let Some(ntp_synchronized) = ntp else {
        return Err(ClockError::Protocol(
            "timedatectl output missing NTPSynchronized".into(),
        ));
    };
    Ok(ClockSyncStatus {
        ntp_synchronized,
        system_clock_synchronized: system,
    })
}

fn parse_yes_no(value: &str) -> Result<bool, ClockError> {
    match value {
        "yes" | "true" | "1" => Ok(true),
        "no" | "false" | "0" => Ok(false),
        other => Err(ClockError::Protocol(format!(
            "invalid timedatectl bool value: {other}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_synced_timedatectl_output() {
        let status = parse_timedatectl_sync(
            "NTPSynchronized=yes\nSystemClockSynchronized=yes\nTimeUSec=ignored\n",
        )
        .unwrap();

        assert!(status.ntp_synchronized);
        assert_eq!(status.system_clock_synchronized, Some(true));
    }

    #[test]
    fn parses_unsynced_timedatectl_output() {
        let status =
            parse_timedatectl_sync("NTPSynchronized=no\nSystemClockSynchronized=yes\n").unwrap();

        assert!(!status.ntp_synchronized);
        assert_eq!(status.system_clock_synchronized, Some(true));
    }

    #[test]
    fn rejects_missing_ntp_field() {
        assert!(parse_timedatectl_sync("SystemClockSynchronized=yes\n").is_err());
    }
}
