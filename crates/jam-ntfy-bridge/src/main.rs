//! `jam-ntfy-bridge` - forwards `notify.human` bus messages to ntfy.
//!
//! The bridge is intentionally narrow: it consumes already-traced notification
//! events, validates the human-facing shape, and invokes `curl` without a shell.

#![deny(missing_docs)]

use std::fs;
use std::path::{Path, PathBuf};

use clap::Parser;
use futures::StreamExt;
use jam_nats::async_nats;
use jam_nats::JamNats;
use jam_secrets::{ExposeSecret, FileBackend, PassBackend, SecretBackend, SecretKey, SecretString};
use jam_trace::TraceCtx;
use rand::distributions::{Alphanumeric, DistString};
use serde::Deserialize;
use serde_json::Value;
use tokio::process::Command;
use tokio::time::Duration;
use tracing::{error, info, warn};

const SERVICE_NAME: &str = "jam-ntfy-bridge";
const SERVICE_VERSION: &str = env!("CARGO_PKG_VERSION");
const DEFAULT_NTFY_SERVER_URL: &str = "https://ntfy.sh";
const DEFAULT_CURL_BIN: &str = "curl";
const DEFAULT_TIMEOUT_SECS: u64 = 15;
const INSTALL_ID_FILE: &str = "install-id";
const MAX_SUMMARY_LEN: usize = 500;
const MAX_BODY_LEN: usize = 8_000;

#[derive(Debug, Parser)]
#[command(name = SERVICE_NAME, version, about = "Forward notify.human to ntfy")]
struct Cli {
    /// ntfy server URL. Defaults to https://ntfy.sh.
    #[arg(long)]
    server_url: Option<String>,

    /// ntfy topic. Defaults to jam-<user-id>-<install-id>.
    #[arg(long)]
    topic: Option<String>,

    /// curl-compatible binary path.
    #[arg(long)]
    curl_bin: Option<PathBuf>,

    /// TOML secrets file fallback. If absent, pass is used.
    #[arg(long)]
    secrets_file: Option<PathBuf>,

    /// Stop after this many notify.human events; useful for smoke tests.
    #[arg(long)]
    max_events: Option<u64>,
}

#[derive(Debug, thiserror::Error)]
enum BridgeError {
    #[error("nats: {0}")]
    Nats(#[from] jam_nats::NatsError),

    #[error("subscribe: {0}")]
    Subscribe(String),

    #[error("json: {0}")]
    Json(#[from] serde_json::Error),

    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("secret: {0}")]
    Secret(#[from] jam_secrets::SecretError),

    #[error("config: {0}")]
    Config(String),

    #[error("notify: {0}")]
    Notify(String),
}

#[derive(Debug, Clone)]
struct Config {
    nats_url: String,
    nats_token: Option<String>,
    server_url: String,
    topic: String,
    token: SecretString,
    curl_bin: PathBuf,
    timeout: Duration,
}

impl Config {
    fn from_env_and_cli(cli: &Cli) -> Result<Self, BridgeError> {
        let jam_home = jam_tools_core::paths::jam_home();
        let raw_server_url = cli
            .server_url
            .as_deref()
            .map(str::to_owned)
            .or_else(|| std::env::var("JAM_NTFY_SERVER_URL").ok())
            .unwrap_or_else(|| DEFAULT_NTFY_SERVER_URL.into());
        let server_url = normalize_server_url(&raw_server_url)?;
        let topic = cli
            .topic
            .clone()
            .or_else(|| std::env::var("JAM_NTFY_TOPIC").ok())
            .map_or_else(|| default_topic(&jam_home), Ok)?;
        validate_topic(&topic)?;
        let secrets_file = cli
            .secrets_file
            .clone()
            .or_else(|| std::env::var_os("JAM_SECRETS_FILE").map(PathBuf::from));
        let token = load_ntfy_token(secrets_file.as_deref())?;
        let curl_bin = cli
            .curl_bin
            .clone()
            .or_else(|| std::env::var_os("JAM_CURL_BIN").map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from(DEFAULT_CURL_BIN));
        let timeout = std::env::var("JAM_NTFY_TIMEOUT_SECS")
            .ok()
            .and_then(|raw| raw.parse::<u64>().ok())
            .map_or(
                Duration::from_secs(DEFAULT_TIMEOUT_SECS),
                Duration::from_secs,
            );

        Ok(Self {
            nats_url: std::env::var("NATS_URL").unwrap_or_else(|_| "nats://127.0.0.1:4222".into()),
            nats_token: std::env::var("NATS_TOKEN").ok(),
            server_url,
            topic,
            token,
            curl_bin,
            timeout,
        })
    }

    fn endpoint(&self) -> String {
        format!("{}/{}", self.server_url, self.topic)
    }
}

#[derive(Debug, Clone, Deserialize)]
struct NotifyHumanMessage {
    #[serde(default = "default_urgency")]
    urgency: String,
    summary: String,
    #[serde(default)]
    payload: Option<Value>,
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    if let Err(err) = run().await {
        error!("jam-ntfy-bridge fatal: {err}");
        return std::process::ExitCode::FAILURE;
    }
    std::process::ExitCode::SUCCESS
}

async fn run() -> Result<(), BridgeError> {
    init_tracing();
    let cli = Cli::parse();
    let config = Config::from_env_and_cli(&cli)?;

    info!(
        service = %SERVICE_NAME,
        version = %SERVICE_VERSION,
        nats = %config.nats_url,
        server = %config.server_url,
        topic = %config.topic,
        curl_bin = %config.curl_bin.display(),
        "starting",
    );

    let nats = JamNats::connect(&config.nats_url, config.nats_token.clone()).await?;
    info!("connected to NATS");

    let mut sub = nats
        .client()
        .subscribe("notify.human")
        .await
        .map_err(|err| BridgeError::Subscribe(err.to_string()))?;
    info!(subject = "notify.human", "subscribed");

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
                    warn!("notify.human subscription closed");
                    return Ok(());
                };
                if let Err(err) = handle_notify_message(&config, &message).await {
                    warn!(subject = %message.subject, "notify.human delivery failed: {err}");
                }
                handled = handled.saturating_add(1);
                if cli.max_events.is_some_and(|max_events| handled >= max_events) {
                    info!(handled, "max events reached");
                    return Ok(());
                }
            }
        }
    }
}

fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("jam_ntfy_bridge=info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

async fn handle_notify_message(
    config: &Config,
    message: &async_nats::Message,
) -> Result<(), BridgeError> {
    let Some(ctx) = message
        .headers
        .as_ref()
        .and_then(jam_nats::extract_trace_from_headers)
    else {
        return Err(BridgeError::Notify(
            "notify.human message missing Trace-Id header".into(),
        ));
    };
    let notification: NotifyHumanMessage = serde_json::from_slice(&message.payload)?;
    let notification = validate_notification(notification)?;
    send_ntfy(config, &notification, &ctx).await?;
    info!(
        trace_id = %ctx.trace_id,
        urgency = %notification.urgency,
        "ntfy notification delivered",
    );
    Ok(())
}

async fn send_ntfy(
    config: &Config,
    notification: &NotifyHumanMessage,
    ctx: &TraceCtx,
) -> Result<(), BridgeError> {
    let output = Command::new(&config.curl_bin)
        .args(curl_args(config, notification, ctx)?)
        .output()
        .await
        .map_err(|err| {
            BridgeError::Notify(format!(
                "failed to execute {}: {err}",
                config.curl_bin.display()
            ))
        })?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    Err(BridgeError::Notify(if stderr.is_empty() {
        stdout
    } else {
        stderr
    }))
}

fn curl_args(
    config: &Config,
    notification: &NotifyHumanMessage,
    ctx: &TraceCtx,
) -> Result<Vec<String>, BridgeError> {
    let body = render_body(notification, ctx)?;
    Ok(vec![
        "-fsS".into(),
        "--max-time".into(),
        config.timeout.as_secs().to_string(),
        "-X".into(),
        "POST".into(),
        "-H".into(),
        format!("Authorization: Bearer {}", config.token.expose_secret()),
        "-H".into(),
        format!("Title: {}", title_for(notification)),
        "-H".into(),
        format!("Priority: {}", priority_for(&notification.urgency)),
        "-H".into(),
        format!("Tags: {}", tags_for(&notification.urgency)),
        "--data-raw".into(),
        body,
        config.endpoint(),
    ])
}

fn render_body(notification: &NotifyHumanMessage, ctx: &TraceCtx) -> Result<String, BridgeError> {
    let mut body = format!(
        "{}\n\ntrace_id: {}\norigin: {}",
        notification.summary, ctx.trace_id, ctx.origin_kind
    );
    if let Some(payload) = &notification.payload {
        let rendered = serde_json::to_string_pretty(payload)?;
        body.push_str("\n\npayload:\n");
        body.push_str(&rendered);
    }
    if body.contains('\0') {
        return Err(BridgeError::Notify(
            "notification body may not contain NUL".into(),
        ));
    }
    if body.len() > MAX_BODY_LEN {
        body.truncate(MAX_BODY_LEN);
        body.push_str("\n\n[truncated]");
    }
    Ok(body)
}

fn validate_notification(
    notification: NotifyHumanMessage,
) -> Result<NotifyHumanMessage, BridgeError> {
    let urgency = normalize_urgency(&notification.urgency)?;
    let summary = notification.summary.trim();
    if summary.is_empty() {
        return Err(BridgeError::Notify(
            "notify.human summary must not be empty".into(),
        ));
    }
    if summary.len() > MAX_SUMMARY_LEN {
        return Err(BridgeError::Notify(format!(
            "notify.human summary must be at most {MAX_SUMMARY_LEN} bytes"
        )));
    }
    if summary.contains('\0') {
        return Err(BridgeError::Notify(
            "notify.human summary may not contain NUL".into(),
        ));
    }
    Ok(NotifyHumanMessage {
        urgency,
        summary: summary.to_owned(),
        payload: notification.payload,
    })
}

fn normalize_urgency(raw: &str) -> Result<String, BridgeError> {
    let urgency = raw.trim().to_ascii_lowercase();
    match urgency.as_str() {
        "low" | "medium" | "high" | "critical" => Ok(urgency),
        _ => Err(BridgeError::Notify(format!(
            "notify.human urgency {raw:?} is not one of low, medium, high, critical"
        ))),
    }
}

fn default_urgency() -> String {
    "medium".into()
}

fn title_for(notification: &NotifyHumanMessage) -> String {
    match notification.urgency.as_str() {
        "critical" => "Jamboree critical".into(),
        "high" => "Jamboree high".into(),
        "low" => "Jamboree low".into(),
        _ => "Jamboree".into(),
    }
}

fn priority_for(urgency: &str) -> &'static str {
    match urgency {
        "low" => "2",
        "high" => "4",
        "critical" => "5",
        _ => "3",
    }
}

fn tags_for(urgency: &str) -> &'static str {
    match urgency {
        "critical" | "high" => "jamboree,warning",
        _ => "jamboree",
    }
}

fn normalize_server_url(raw: &str) -> Result<String, BridgeError> {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err(BridgeError::Config("JAM_NTFY_SERVER_URL is empty".into()));
    }
    if trimmed.contains('\0') {
        return Err(BridgeError::Config(
            "JAM_NTFY_SERVER_URL may not contain NUL".into(),
        ));
    }
    if !(trimmed.starts_with("https://") || trimmed.starts_with("http://")) {
        return Err(BridgeError::Config(
            "JAM_NTFY_SERVER_URL must start with http:// or https://".into(),
        ));
    }
    Ok(trimmed.to_owned())
}

fn default_topic(jam_home: &Path) -> Result<String, BridgeError> {
    let user_id = std::env::var("JAM_NTFY_USER_ID")
        .or_else(|_| std::env::var("JAM_MANAGER_USER"))
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_else(|_| "maestro".into());
    default_topic_for(jam_home, &user_id)
}

fn default_topic_for(jam_home: &Path, user_id: &str) -> Result<String, BridgeError> {
    let user_id = sanitize_topic_part(user_id);
    let install_id = load_or_create_install_id(jam_home)?;
    let topic = format!("jam-{user_id}-{install_id}");
    validate_topic(&topic)?;
    Ok(topic)
}

fn load_or_create_install_id(jam_home: &Path) -> Result<String, BridgeError> {
    let path = jam_home.join(INSTALL_ID_FILE);
    if path.exists() {
        let value = fs::read_to_string(&path)?;
        let value = value.trim();
        if value.is_empty() {
            return Err(BridgeError::Config(format!("{} is empty", path.display())));
        }
        return Ok(sanitize_topic_part(value));
    }
    fs::create_dir_all(jam_home)?;
    let install_id = Alphanumeric.sample_string(&mut rand::thread_rng(), 16);
    let install_id = install_id.to_ascii_lowercase();
    fs::write(&path, format!("{install_id}\n"))?;
    Ok(install_id)
}

fn sanitize_topic_part(raw: &str) -> String {
    let mut out = String::new();
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if matches!(ch, '-' | '_') {
            out.push(ch);
        }
    }
    if out.is_empty() {
        "unknown".into()
    } else {
        out
    }
}

fn validate_topic(topic: &str) -> Result<(), BridgeError> {
    if topic.is_empty() || topic.len() > 128 {
        return Err(BridgeError::Config(
            "JAM_NTFY_TOPIC must be 1..=128 bytes".into(),
        ));
    }
    if !topic
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(BridgeError::Config(
            "JAM_NTFY_TOPIC may contain only ASCII letters, digits, '-' and '_'".into(),
        ));
    }
    Ok(())
}

fn load_ntfy_token(secrets_file: Option<&Path>) -> Result<SecretString, BridgeError> {
    if let Ok(raw) = std::env::var("JAM_NTFY_TOKEN") {
        if raw.trim().is_empty() {
            return Err(BridgeError::Config("JAM_NTFY_TOKEN is empty".into()));
        }
        return Ok(SecretString::from(raw));
    }
    if let Some(path) = secrets_file {
        let backend = FileBackend::new(path);
        return backend
            .get(&SecretKey::new("jam/notify/ntfy-token"))
            .map_err(BridgeError::from);
    }
    let backend = PassBackend::new("jam");
    backend
        .get(&SecretKey::new("notify/ntfy-token"))
        .map_err(BridgeError::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use jam_trace::TraceCtx;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::TempDir;

    #[test]
    fn default_topic_is_stable_and_sanitized() {
        let tmp = TempDir::new().unwrap();

        let topic = default_topic_for(tmp.path(), "Caleb Leak").unwrap();
        let topic_again = default_topic_for(tmp.path(), "Caleb Leak").unwrap();

        assert_eq!(topic, topic_again);
        assert!(topic.starts_with("jam-calebleak-"));
        assert!(tmp.path().join(INSTALL_ID_FILE).exists());
    }

    #[test]
    fn rejects_unsafe_topic_values() {
        assert!(validate_topic("jam-caleb-good_1").is_ok());
        assert!(validate_topic("jam/caleb").is_err());
        assert!(validate_topic("").is_err());
    }

    #[test]
    fn validates_notification_shape() {
        let notification = validate_notification(NotifyHumanMessage {
            urgency: "CRITICAL".into(),
            summary: "Check budget cap".into(),
            payload: Some(serde_json::json!({"session_id": "s1"})),
        })
        .unwrap();

        assert_eq!(notification.urgency, "critical");
        assert_eq!(priority_for(&notification.urgency), "5");
    }

    #[test]
    fn curl_args_use_ntfy_headers() {
        let config = test_config(PathBuf::from("curl"));
        let ctx = TraceCtx::new_root("test.notify", "notify test");
        let notification = NotifyHumanMessage {
            urgency: "high".into(),
            summary: "Investigate failed write".into(),
            payload: None,
        };

        let args = curl_args(&config, &notification, &ctx).unwrap();

        assert!(args.contains(&"Authorization: Bearer test-token".into()));
        assert!(args.contains(&"Priority: 4".into()));
        assert_eq!(args.last().unwrap(), "http://127.0.0.1:9/jam-caleb-test");
    }

    #[tokio::test]
    async fn send_ntfy_invokes_curl_without_shell() {
        let tmp = TempDir::new().unwrap();
        let curl = fake_curl(tmp.path(), true);
        let config = test_config(curl);
        let ctx = TraceCtx::new_root("test.notify", "notify test");
        let notification = NotifyHumanMessage {
            urgency: "critical".into(),
            summary: "Manager action required".into(),
            payload: Some(serde_json::json!({"trace": "abc"})),
        };

        send_ntfy(&config, &notification, &ctx).await.unwrap();

        let recorded = fs::read_to_string(tmp.path().join("curl-args")).unwrap();
        assert!(recorded.contains("Authorization: Bearer test-token"));
        assert!(recorded.contains("Priority: 5"));
        assert!(recorded.contains("Manager action required"));
        assert!(recorded.contains("http://127.0.0.1:9/jam-caleb-test"));
    }

    #[tokio::test]
    async fn send_ntfy_reports_curl_failure() {
        let tmp = TempDir::new().unwrap();
        let curl = fake_curl(tmp.path(), false);
        let config = test_config(curl);
        let ctx = TraceCtx::new_root("test.notify", "notify test");
        let notification = NotifyHumanMessage {
            urgency: "medium".into(),
            summary: "Test failure".into(),
            payload: None,
        };

        let err = send_ntfy(&config, &notification, &ctx).await.unwrap_err();

        assert!(err.to_string().contains("fake curl failed"));
    }

    fn test_config(curl_bin: PathBuf) -> Config {
        Config {
            nats_url: "nats://127.0.0.1:4222".into(),
            nats_token: None,
            server_url: "http://127.0.0.1:9".into(),
            topic: "jam-caleb-test".into(),
            token: SecretString::from("test-token".to_string()),
            curl_bin,
            timeout: Duration::from_secs(1),
        }
    }

    #[cfg(unix)]
    fn fake_curl(root: &Path, success: bool) -> PathBuf {
        let script = root.join("fake-curl");
        let args = root.join("curl-args");
        let body = if success {
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" > {}\nexit 0\n",
                args.display()
            )
        } else {
            "#!/bin/sh\necho 'fake curl failed' >&2\nexit 1\n".into()
        };
        write_executable_for_test(&script, &body);
        script
    }
}

#[cfg(test)]
#[cfg(unix)]
fn write_executable_for_test(path: &std::path::Path, body: &str) {
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;
    // Explicit File::create + write_all + sync_all + drop guarantees
    // the kernel has fully released the write fd before the caller
    // exec's the script. Plain `fs::write` followed by
    // `fs::set_permissions` was racing with Linux's ETXTBSY check on
    // CI runners under load — the test would panic with
    // "Text file busy (os error 26)" because the busy-text flag hadn't
    // cleared by the time `Command::new(...).output()` tried to exec.
    let mut file = std::fs::File::create(path).expect("create test script");
    file.write_all(body.as_bytes()).expect("write test script");
    file.sync_all().expect("fsync test script");
    drop(file);
    let mut perms = std::fs::metadata(path)
        .expect("stat test script")
        .permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms).expect("chmod test script");
}
