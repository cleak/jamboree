//! `jam-harness-watcher` - detects harness binary drift (§4.4.6, §4.5.5).
//!
//! The watcher compares installed harness binaries against the per-project
//! lockfile. Drift emits `harness.version-changed`; spawn-time enforcement
//! still lives in `jam-svc-session`, so new Pickers fail loudly even if the
//! periodic watcher has not run yet.

#![deny(missing_docs)]

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

use chrono::{DateTime, Utc};
use clap::Parser;
use jam_events::generated::{Event, HarnessVersionChanged};
use jam_events::EventEnvelope;
use jam_nats::JamNats;
use jam_trace::TraceCtx;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::time::{self, Duration};
use tracing::{error, info};

const SERVICE_NAME: &str = "jam-harness-watcher";
const SERVICE_VERSION: &str = env!("CARGO_PKG_VERSION");
const DEFAULT_INTERVAL_SECS: u64 = 3_600;
const DEFAULT_HARNESS: &str = "codex-cli";
const DEFAULT_CODEX_BIN: &str = "codex";

#[derive(Debug, Parser)]
#[command(
    name = SERVICE_NAME,
    version,
    about = "Detect installed harness binary drift"
)]
struct Cli {
    /// Check cadence in seconds.
    #[arg(long)]
    interval_secs: Option<u64>,

    /// Per-project harness lockfile path.
    #[arg(long)]
    lockfile_path: Option<PathBuf>,

    /// Codex CLI binary path.
    #[arg(long)]
    codex_bin: Option<PathBuf>,

    /// Check once, then exit.
    #[arg(long)]
    once: bool,

    /// Stop after this many check ticks; useful for smoke tests.
    #[arg(long)]
    max_ticks: Option<u64>,
}

#[derive(Debug, thiserror::Error)]
enum WatcherError {
    #[error("nats: {0}")]
    Nats(#[from] jam_nats::NatsError),

    #[error("lockfile: {0}")]
    Lockfile(String),

    #[error("harness: {0}")]
    Harness(String),
}

#[derive(Debug, Clone)]
struct Config {
    nats_url: String,
    nats_token: Option<String>,
    lockfile_path: PathBuf,
    codex_bin: PathBuf,
    interval_secs: u64,
}

impl Config {
    fn from_env_and_cli(cli: &Cli) -> Self {
        Self {
            nats_url: std::env::var("NATS_URL").unwrap_or_else(|_| "nats://127.0.0.1:4222".into()),
            nats_token: std::env::var("NATS_TOKEN").ok(),
            lockfile_path: cli
                .lockfile_path
                .clone()
                .or_else(|| std::env::var_os("JAM_HARNESS_LOCKFILE").map(PathBuf::from))
                .unwrap_or_else(default_harness_lockfile_path),
            codex_bin: cli
                .codex_bin
                .clone()
                .or_else(|| std::env::var_os("JAM_CODEX_BIN").map(PathBuf::from))
                .unwrap_or_else(|| PathBuf::from(DEFAULT_CODEX_BIN)),
            interval_secs: cli.interval_secs.unwrap_or_else(|| {
                env_parse("JAM_HARNESS_WATCH_INTERVAL_SECS").unwrap_or(DEFAULT_INTERVAL_SECS)
            }),
        }
    }
}

#[derive(Debug, Deserialize)]
struct HarnessLockfile {
    harnesses: HashMap<String, HarnessPin>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct HarnessPin {
    version: String,
    checksum_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HarnessDrift {
    harness: String,
    expected: String,
    installed: String,
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    if let Err(err) = run().await {
        error!("jam-harness-watcher fatal: {err}");
        return std::process::ExitCode::FAILURE;
    }
    std::process::ExitCode::SUCCESS
}

async fn run() -> Result<(), WatcherError> {
    init_tracing();
    let cli = Cli::parse();
    let config = Config::from_env_and_cli(&cli);

    info!(
        service = %SERVICE_NAME,
        version = %SERVICE_VERSION,
        nats = %config.nats_url,
        lockfile = %config.lockfile_path.display(),
        codex_bin = %config.codex_bin.display(),
        interval_secs = config.interval_secs,
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
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("jam_harness_watcher=info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

fn default_harness_lockfile_path() -> PathBuf {
    jam_tools_core::paths::jam_home()
        .join("config")
        .join("projects")
        .join("blueberry-harnesses.lock")
}

fn env_parse<T>(name: &str) -> Option<T>
where
    T: std::str::FromStr,
{
    std::env::var(name).ok()?.parse().ok()
}

async fn check_once(
    nats: &JamNats,
    config: &Config,
    detected_at: DateTime<Utc>,
) -> Result<(), WatcherError> {
    let Some(drift) = detect_codex_drift(&config.codex_bin, &config.lockfile_path)? else {
        info!(harness = DEFAULT_HARNESS, "harness matches lockfile");
        return Ok(());
    };

    let ctx = TraceCtx::new_root(
        "harness-watcher.version-changed",
        format!(
            "{} drift: expected {}, installed {}",
            drift.harness, drift.expected, drift.installed
        ),
    );
    let payload = HarnessVersionChanged {
        harness: drift.harness,
        expected: drift.expected,
        installed: drift.installed,
        detected_at,
    };
    let envelope = EventEnvelope::new(
        HarnessVersionChanged::EVENT_TYPE,
        HarnessVersionChanged::EVENT_SUBTYPE_VERSION,
        0,
        ctx.trace_id.to_string(),
        SERVICE_NAME,
        payload,
    );
    nats.publish_traced("journal.harness.version-changed", &envelope, &ctx)
        .await?;
    info!("published harness.version-changed");
    Ok(())
}

fn detect_codex_drift(
    codex_bin: &Path,
    lockfile_path: &Path,
) -> Result<Option<HarnessDrift>, WatcherError> {
    let lockfile = read_lockfile(lockfile_path)?;
    let pin = lockfile.harnesses.get(DEFAULT_HARNESS).ok_or_else(|| {
        WatcherError::Lockfile(format!("{DEFAULT_HARNESS} is not pinned in lockfile"))
    })?;
    let installed = InstalledHarness::inspect(codex_bin)?;
    let expected = harness_fingerprint(&pin.version, &pin.checksum_sha256);
    let installed_fingerprint = harness_fingerprint(&installed.version, &installed.checksum_sha256);

    if expected == installed_fingerprint {
        Ok(None)
    } else {
        Ok(Some(HarnessDrift {
            harness: DEFAULT_HARNESS.into(),
            expected,
            installed: installed_fingerprint,
        }))
    }
}

fn read_lockfile(path: &Path) -> Result<HarnessLockfile, WatcherError> {
    let raw = fs::read_to_string(path)
        .map_err(|err| WatcherError::Lockfile(format!("read {}: {err}", path.display())))?;
    toml::from_str(&raw)
        .map_err(|err| WatcherError::Lockfile(format!("parse {}: {err}", path.display())))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct InstalledHarness {
    version: String,
    checksum_sha256: String,
}

impl InstalledHarness {
    fn inspect(codex_bin: &Path) -> Result<Self, WatcherError> {
        let resolved = resolve_binary_path(codex_bin)?;
        Ok(Self {
            version: codex_version(&resolved)?,
            checksum_sha256: sha256_file(&resolved)?,
        })
    }
}

fn codex_version(codex_bin: &Path) -> Result<String, WatcherError> {
    let output = ProcessCommand::new(codex_bin)
        .arg("--version")
        .output()
        .map_err(|err| {
            WatcherError::Harness(format!(
                "failed to run {} --version: {err}",
                codex_bin.display()
            ))
        })?;
    if !output.status.success() {
        return Err(WatcherError::Harness(format!(
            "{} --version failed: {}",
            codex_bin.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    parse_codex_version(&String::from_utf8_lossy(&output.stdout)).ok_or_else(|| {
        WatcherError::Harness(format!(
            "{} --version output was not understood: {}",
            codex_bin.display(),
            String::from_utf8_lossy(&output.stdout).trim()
        ))
    })
}

fn parse_codex_version(output: &str) -> Option<String> {
    let trimmed = output.trim();
    if trimmed.is_empty() {
        return None;
    }
    trimmed
        .split_whitespace()
        .find(|part| part.chars().next().is_some_and(|ch| ch.is_ascii_digit()))
        .map(ToOwned::to_owned)
}

fn sha256_file(path: &Path) -> Result<String, WatcherError> {
    let bytes = fs::read(path)
        .map_err(|err| WatcherError::Harness(format!("read {}: {err}", path.display())))?;
    let digest = Sha256::digest(bytes);
    Ok(hex::encode(digest))
}

fn resolve_binary_path(path: &Path) -> Result<PathBuf, WatcherError> {
    if path.components().count() > 1 || path.is_absolute() {
        return path.canonicalize().map_err(|err| {
            WatcherError::Harness(format!("canonicalize {}: {err}", path.display()))
        });
    }
    let path_var = std::env::var_os("PATH").unwrap_or_default();
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(path);
        if candidate.is_file() {
            return candidate.canonicalize().map_err(|err| {
                WatcherError::Harness(format!("canonicalize {}: {err}", candidate.display()))
            });
        }
    }
    Err(WatcherError::Harness(format!(
        "could not find {} on PATH",
        path.display()
    )))
}

fn harness_fingerprint(version: &str, checksum_sha256: &str) -> String {
    format!("version={version} checksum-sha256={checksum_sha256}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn parses_codex_version_output() {
        assert_eq!(
            parse_codex_version("codex-cli 0.128.0\n"),
            Some("0.128.0".into())
        );
        assert_eq!(parse_codex_version("0.128.0\n"), Some("0.128.0".into()));
        assert_eq!(parse_codex_version(""), None);
    }

    #[cfg(unix)]
    #[test]
    fn matching_lockfile_returns_no_drift() {
        let fixture = HarnessFixture::new("0.128.0");
        fixture.write_lockfile("0.128.0", &fixture.checksum);

        let drift = detect_codex_drift(&fixture.codex_bin, &fixture.lockfile).unwrap();

        assert_eq!(drift, None);
    }

    #[cfg(unix)]
    #[test]
    fn version_mismatch_reports_drift() {
        let fixture = HarnessFixture::new("0.128.0");
        fixture.write_lockfile("0.127.0", &fixture.checksum);

        let drift = detect_codex_drift(&fixture.codex_bin, &fixture.lockfile)
            .unwrap()
            .unwrap();

        assert_eq!(drift.harness, "codex-cli");
        assert!(drift.expected.contains("version=0.127.0"));
        assert!(drift.installed.contains("version=0.128.0"));
    }

    #[cfg(unix)]
    #[test]
    fn checksum_mismatch_reports_drift() {
        let fixture = HarnessFixture::new("0.128.0");
        fixture.write_lockfile("0.128.0", "bad-checksum");

        let drift = detect_codex_drift(&fixture.codex_bin, &fixture.lockfile)
            .unwrap()
            .unwrap();

        assert_eq!(drift.harness, "codex-cli");
        assert!(drift.expected.contains("checksum-sha256=bad-checksum"));
        assert!(drift
            .installed
            .contains(&format!("checksum-sha256={}", fixture.checksum)));
    }

    #[cfg(unix)]
    struct HarnessFixture {
        _tmp: TempDir,
        codex_bin: PathBuf,
        lockfile: PathBuf,
        checksum: String,
    }

    #[cfg(unix)]
    impl HarnessFixture {
        fn new(version: &str) -> Self {
            use std::os::unix::fs::PermissionsExt;

            let tmp = TempDir::new().unwrap();
            let codex_bin = tmp.path().join("codex");
            fs::write(
                &codex_bin,
                format!("#!/bin/sh\nprintf 'codex-cli {version}\\n'\n"),
            )
            .unwrap();
            let mut permissions = fs::metadata(&codex_bin).unwrap().permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&codex_bin, permissions).unwrap();
            let checksum = sha256_file(&codex_bin).unwrap();
            let lockfile = tmp.path().join("blueberry-harnesses.lock");
            Self {
                _tmp: tmp,
                codex_bin,
                lockfile,
                checksum,
            }
        }

        fn write_lockfile(&self, version: &str, checksum: &str) {
            fs::write(
                &self.lockfile,
                format!(
                    r#"[harnesses.codex-cli]
version = "{version}"
checksum-sha256 = "{checksum}"
"#
                ),
            )
            .unwrap();
        }
    }
}
