//! `jam-svc-search` - search router (§4.8).
//!
//! Web search starts with Brave. Extraction/crawl can use direct HTTP for static
//! pages or Firecrawl v2 when explicitly configured, including `render_js`.

#![deny(missing_docs)]

use std::collections::{HashSet, VecDeque};
use std::fmt::Write as _;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bytes::Bytes;
use chrono::{DateTime, Utc};
use futures::StreamExt;
use jam_events::generated::{Event, SearchWebSearch};
use jam_events::EventEnvelope;
use jam_nats::async_nats;
use jam_nats::JamNats;
use jam_secrets::{ExposeSecret, FileBackend, PassBackend, SecretBackend, SecretError, SecretKey};
use jam_trace::TraceCtx;
use reqwest::Url;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::process::Command;
use tracing::{debug, error, info, warn};

const SERVICE_NAME: &str = "jam-svc-search";
const SERVICE_VERSION: &str = env!("CARGO_PKG_VERSION");
const SUBJECT_PREFIX: &str = "tool.search";
const SUBJECT_PREFIX_ENV: &str = "JAM_SEARCH_SUBJECT_PREFIX";
const DEFAULT_CURL_BIN: &str = "curl";
const DEFAULT_BRAVE_ENDPOINT: &str = "https://api.search.brave.com/res/v1/web/search";
const DEFAULT_FIRECRAWL_ENDPOINT: &str = "https://api.firecrawl.dev/v2";
const DEFAULT_LINKUP_ENDPOINT: &str = "https://api.linkup.so/v1/search";
const DEFAULT_LINKUP_DEPTH: &str = "standard";
const DEFAULT_TIMEOUT_SECS: u64 = 20;
const DEFAULT_RESULT_COUNT: u32 = 5;
const COOLDOWN_SECS: i64 = 3_600;
const MAX_EXTRACT_URLS: usize = 10;
const MAX_CRAWL_DEPTH: u32 = 2;
const DEFAULT_CRAWL_MAX_PAGES: u32 = 5;
const MAX_CRAWL_PAGES: u32 = 25;
const MAX_EXTRACTED_TEXT_CHARS: usize = 60_000;

const BRAVE_ENV_KEYS: &[&str] = &["JAM_BRAVE_API_KEY", "BRAVE_API_KEY"];
const FIRECRAWL_ENV_KEYS: &[&str] = &["JAM_FIRECRAWL_API_KEY", "FIRECRAWL_API_KEY"];
const LINKUP_ENV_KEYS: &[&str] = &["JAM_LINKUP_API_KEY", "LINKUP_API_KEY"];
const BRAVE_FILE_SECRET_KEYS: &[&str] = &["jam/search/brave"];
const FIRECRAWL_FILE_SECRET_KEYS: &[&str] = &["jam/search/firecrawl"];
const LINKUP_FILE_SECRET_KEYS: &[&str] = &["jam/search/linkup"];
const BRAVE_PASS_SECRET_KEYS: &[&str] = &["search/brave"];
const FIRECRAWL_PASS_SECRET_KEYS: &[&str] = &["search/firecrawl"];
const LINKUP_PASS_SECRET_KEYS: &[&str] = &["search/linkup"];

#[derive(Debug, thiserror::Error)]
enum ServiceError {
    #[error("nats: {0}")]
    Nats(#[from] jam_nats::NatsError),

    #[error("subscribe: {0}")]
    Subscribe(String),

    #[error("reply: {0}")]
    Reply(String),
}

#[derive(Debug, thiserror::Error)]
enum SearchError {
    #[error("{kind}: {detail}")]
    Protocol {
        kind: &'static str,
        detail: String,
        remediation: &'static str,
        tracked_by: &'static str,
    },
}

impl SearchError {
    fn protocol(
        kind: &'static str,
        detail: impl Into<String>,
        remediation: &'static str,
        tracked_by: &'static str,
    ) -> Self {
        Self::Protocol {
            kind,
            detail: detail.into(),
            remediation,
            tracked_by,
        }
    }
}

#[derive(Clone)]
struct SearchState {
    config: SearchConfig,
    cooldown: Arc<Mutex<CooldownState>>,
}

#[derive(Debug, Clone)]
struct SearchConfig {
    curl_bin: PathBuf,
    brave_endpoint: String,
    brave_api_key: Option<String>,
    web_search_backend: WebSearchBackendMode,
    searxng_endpoint: Option<String>,
    linkup_endpoint: String,
    linkup_api_key: Option<String>,
    linkup_depth: String,
    firecrawl_endpoint: String,
    firecrawl_api_key: Option<String>,
    extract_backend: ExtractBackend,
    timeout: Duration,
    result_count: u32,
    http_client: reqwest::Client,
}

impl SearchConfig {
    fn from_env() -> Result<Self, SearchError> {
        let web_search_backend = WebSearchBackendMode::from_env()?;
        let extract_backend = ExtractBackend::from_env()?;
        let secrets_file = std::env::var_os("JAM_SECRETS_FILE").map(PathBuf::from);
        let firecrawl_api_key = search_api_key(
            FIRECRAWL_ENV_KEYS,
            FIRECRAWL_FILE_SECRET_KEYS,
            FIRECRAWL_PASS_SECRET_KEYS,
            secrets_file.as_deref(),
        )?;
        if extract_backend == ExtractBackend::Firecrawl && firecrawl_api_key.is_none() {
            return Err(SearchError::protocol(
                "missing-firecrawl-api-key",
                "JAM_SEARCH_EXTRACT_BACKEND=firecrawl requires a Firecrawl credential",
                "Seed jam/search/firecrawl in JAM_SECRETS_FILE or maestro pass before enabling Firecrawl extraction.",
                "comp-firecrawl-backend",
            ));
        }
        let brave_api_key = search_api_key(
            BRAVE_ENV_KEYS,
            BRAVE_FILE_SECRET_KEYS,
            BRAVE_PASS_SECRET_KEYS,
            secrets_file.as_deref(),
        )?;
        if web_search_backend.requires_brave() && brave_api_key.is_none() {
            return Err(SearchError::protocol(
                "missing-brave-api-key",
                "Brave Search credential is required for auto/brave search routing",
                "Seed jam/search/brave in JAM_SECRETS_FILE or maestro pass, or set JAM_SEARCH_WEB_BACKEND to a configured non-Brave backend.",
                "comp-brave-backend",
            ));
        }
        let searxng_endpoint = std::env::var("JAM_SEARXNG_ENDPOINT")
            .ok()
            .map(|endpoint| normalize_endpoint(&endpoint))
            .filter(|endpoint| !endpoint.is_empty());
        if web_search_backend == WebSearchBackendMode::Searxng && searxng_endpoint.is_none() {
            return Err(SearchError::protocol(
                "missing-searxng-endpoint",
                "JAM_SEARCH_WEB_BACKEND=searxng requires JAM_SEARXNG_ENDPOINT",
                "Configure a SearXNG instance with JSON output enabled, then set JAM_SEARXNG_ENDPOINT.",
                "comp-searxng-backend",
            ));
        }
        let linkup_api_key = search_api_key(
            LINKUP_ENV_KEYS,
            LINKUP_FILE_SECRET_KEYS,
            LINKUP_PASS_SECRET_KEYS,
            secrets_file.as_deref(),
        )?;
        if web_search_backend == WebSearchBackendMode::Linkup && linkup_api_key.is_none() {
            return Err(SearchError::protocol(
                "missing-linkup-api-key",
                "JAM_SEARCH_WEB_BACKEND=linkup requires a Linkup credential",
                "Seed jam/search/linkup in JAM_SECRETS_FILE or maestro pass before enabling Linkup search.",
                "comp-linkup-backend",
            ));
        }
        let linkup_depth = std::env::var("JAM_LINKUP_DEPTH")
            .unwrap_or_else(|_| DEFAULT_LINKUP_DEPTH.into())
            .trim()
            .to_owned();
        validate_linkup_depth(&linkup_depth)?;
        let timeout = std::env::var("JAM_SEARCH_TIMEOUT_SECS")
            .ok()
            .and_then(|raw| raw.parse().ok())
            .map_or(
                Duration::from_secs(DEFAULT_TIMEOUT_SECS),
                Duration::from_secs,
            );
        Ok(Self {
            curl_bin: std::env::var_os("JAM_CURL_BIN")
                .map_or_else(|| PathBuf::from(DEFAULT_CURL_BIN), PathBuf::from),
            brave_endpoint: std::env::var("JAM_BRAVE_SEARCH_ENDPOINT")
                .unwrap_or_else(|_| DEFAULT_BRAVE_ENDPOINT.into()),
            brave_api_key,
            web_search_backend,
            searxng_endpoint,
            linkup_endpoint: std::env::var("JAM_LINKUP_ENDPOINT")
                .unwrap_or_else(|_| DEFAULT_LINKUP_ENDPOINT.into()),
            linkup_api_key,
            linkup_depth,
            firecrawl_endpoint: normalize_endpoint(
                &std::env::var("JAM_FIRECRAWL_ENDPOINT")
                    .unwrap_or_else(|_| DEFAULT_FIRECRAWL_ENDPOINT.into()),
            ),
            firecrawl_api_key,
            extract_backend,
            timeout,
            result_count: std::env::var("JAM_SEARCH_RESULT_COUNT")
                .ok()
                .and_then(|raw| raw.parse().ok())
                .unwrap_or(DEFAULT_RESULT_COUNT),
            http_client: build_http_client(timeout)?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WebSearchBackendMode {
    Auto,
    Brave,
    Searxng,
    Linkup,
}

impl WebSearchBackendMode {
    fn from_env() -> Result<Self, SearchError> {
        match std::env::var("JAM_SEARCH_WEB_BACKEND")
            .unwrap_or_else(|_| "auto".into())
            .trim()
        {
            "" | "auto" => Ok(Self::Auto),
            "brave" => Ok(Self::Brave),
            "searxng" | "searx" => Ok(Self::Searxng),
            "linkup" => Ok(Self::Linkup),
            other => Err(SearchError::protocol(
                "invalid-search-backend",
                format!("unknown JAM_SEARCH_WEB_BACKEND value: {other}"),
                "Use auto, brave, searxng, or linkup.",
                "comp-search-router",
            )),
        }
    }

    fn requires_brave(self) -> bool {
        matches!(self, Self::Auto | Self::Brave)
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Brave => "brave",
            Self::Searxng => "searxng",
            Self::Linkup => "linkup",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WebSearchBackend {
    Brave,
    Searxng,
    Linkup,
}

impl WebSearchBackend {
    fn as_str(self) -> &'static str {
        match self {
            Self::Brave => "brave",
            Self::Searxng => "searxng",
            Self::Linkup => "linkup",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExtractBackend {
    Direct,
    Firecrawl,
}

impl ExtractBackend {
    fn from_env() -> Result<Self, SearchError> {
        match std::env::var("JAM_SEARCH_EXTRACT_BACKEND")
            .unwrap_or_else(|_| "direct".into())
            .trim()
        {
            "" | "direct" | "direct-fetch" => Ok(Self::Direct),
            "firecrawl" => Ok(Self::Firecrawl),
            other => Err(SearchError::protocol(
                "invalid-extract-backend",
                format!("unknown JAM_SEARCH_EXTRACT_BACKEND value: {other}"),
                "Use direct or firecrawl.",
                "comp-search-router",
            )),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::Firecrawl => "firecrawl",
        }
    }
}

#[derive(Debug, Clone, Default)]
struct CooldownState {
    brave_until: Option<DateTime<Utc>>,
    last_error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WebSearchInput {
    query: String,
    intent: Option<String>,
    time_range: Option<String>,
    domains: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct WebExtractInput {
    urls: Vec<String>,
    render_js: Option<bool>,
    include_images: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct WebCrawlInput {
    root_url: String,
    max_depth: u32,
    max_pages: Option<u32>,
    render_js: Option<bool>,
    include_images: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
struct WebSearchOutput {
    query: String,
    results: Vec<SearchResult>,
    routing: RoutingEnvelope,
    trace_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct WebExtractOutput {
    contents: Vec<ExtractedContent>,
    routing: RoutingEnvelope,
    trace_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct WebCrawlOutput {
    root_url: String,
    pages: Vec<ExtractedContent>,
    routing: RoutingEnvelope,
    trace_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct SearchResult {
    title: String,
    url: String,
    snippet: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ExtractedContent {
    url: String,
    title: Option<String>,
    text: String,
    images: Vec<String>,
}

#[derive(Debug)]
struct FetchedPage {
    content: ExtractedContent,
    links: Vec<Url>,
}

#[derive(Debug, Serialize, Deserialize)]
struct RoutingEnvelope {
    backend: String,
    reason: String,
    cooldown_until: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum Response {
    Ok(serde_json::Value),
    Error { error: ResponseError },
}

#[derive(Debug, Serialize)]
struct ResponseError {
    kind: String,
    detail: String,
    remediation: String,
    tracked_by: &'static str,
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    if let Err(err) = run().await {
        error!("jam-svc-search fatal: {err}");
        return std::process::ExitCode::FAILURE;
    }
    std::process::ExitCode::SUCCESS
}

async fn run() -> Result<(), ServiceError> {
    init_tracing();
    let nats_url = std::env::var("NATS_URL").unwrap_or_else(|_| "nats://127.0.0.1:4222".into());
    let nats_token = std::env::var("NATS_TOKEN").ok();
    let subject_prefix = configured_subject_prefix(SUBJECT_PREFIX_ENV, SUBJECT_PREFIX);
    let config = SearchConfig::from_env().map_err(|err| ServiceError::Reply(err.to_string()))?;

    info!(
        service = %SERVICE_NAME,
        version = %SERVICE_VERSION,
        nats = %nats_url,
        subject_prefix = %subject_prefix,
        brave_endpoint = %config.brave_endpoint,
        web_search_backend = %config.web_search_backend.as_str(),
        extract_backend = %config.extract_backend.as_str(),
        timeout_secs = config.timeout.as_secs(),
        "starting",
    );

    let nats = JamNats::connect(&nats_url, nats_token).await?;
    info!("connected to NATS");

    let state = SearchState {
        config,
        cooldown: Arc::default(),
    };
    let mut sub = nats
        .client()
        .subscribe(format!("{subject_prefix}.>"))
        .await
        .map_err(|e| ServiceError::Subscribe(e.to_string()))?;
    info!(subject = %format!("{subject_prefix}.>"), "subscribed");

    let shutdown = tokio::signal::ctrl_c();
    tokio::pin!(shutdown);
    let draining = Arc::new(AtomicBool::new(false));
    let active_requests = Arc::new(AtomicUsize::new(0));
    let mut drain_check = tokio::time::interval(Duration::from_millis(100));
    drain_check.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            _ = &mut shutdown => {
                info!("shutdown signal received");
                return Ok(());
            }
            _ = drain_check.tick(), if draining.load(Ordering::SeqCst) && active_requests.load(Ordering::SeqCst) == 0 => {
                info!("drain complete; exiting");
                return Ok(());
            }
            msg = sub.next() => {
                let Some(message) = msg else {
                    warn!("subscriber stream closed");
                    return Ok(());
                };
                let nats = nats.clone();
                let state = state.clone();
                let draining = draining.clone();
                let active_requests = active_requests.clone();
                active_requests.fetch_add(1, Ordering::SeqCst);
                tokio::spawn(async move {
                    let result = handle_request(&nats, &message, &state, &draining).await;
                    active_requests.fetch_sub(1, Ordering::SeqCst);
                    if let Err(err) = result {
                        warn!(subject = %message.subject, "handle: {err}");
                    }
                });
            }
        }
    }
}

fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("jam_svc_search=info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

async fn handle_request(
    nats: &JamNats,
    msg: &async_nats::Message,
    state: &SearchState,
    draining: &Arc<AtomicBool>,
) -> Result<(), ServiceError> {
    let method = method_from_subject(msg.subject.as_str()).unwrap_or("");
    debug!(subject = %msg.subject, method = %method, "received request");

    let response_ctx = msg
        .headers
        .as_ref()
        .and_then(jam_nats::extract_trace_from_headers);

    let response = match &response_ctx {
        Some(ctx) => dispatch(method, &msg.payload, state, ctx, nats).await,
        None => Response::Error {
            error: ResponseError {
                kind: "missing-trace".into(),
                detail: "tool.search requests must include Trace-Id headers".into(),
                remediation: "Use JamNats::request_traced for tool calls.".into(),
                tracked_by: "principle-tracing-chains-end-to-end",
            },
        },
    };

    let Some(reply_subject) = msg.reply.as_ref() else {
        warn!(subject = %msg.subject, method = %method, "no reply subject");
        return Ok(());
    };

    let payload = serde_json::to_vec(&response).expect("response serializes");
    if let Some(ctx) = response_ctx {
        nats.publish_bytes_traced(reply_subject.to_string(), Bytes::from(payload), &ctx)
            .await?;
    } else {
        return Err(ServiceError::Reply(
            "missing Trace-Id; refusing untraced reply publish".into(),
        ));
    }
    if method == "drain" {
        draining.store(true, Ordering::SeqCst);
    }
    Ok(())
}

async fn dispatch(
    method: &str,
    payload: &[u8],
    state: &SearchState,
    ctx: &TraceCtx,
    nats: &JamNats,
) -> Response {
    match method {
        "ping" => Response::Ok(serde_json::json!({
            "status": "ok",
            "service": SERVICE_NAME,
            "version": SERVICE_VERSION,
        })),
        "drain" => Response::Ok(serde_json::json!({
            "status": "draining",
            "service": SERVICE_NAME,
            "version": SERVICE_VERSION,
        })),
        "web-search" => match web_search(payload, state, ctx, nats, Utc::now()).await {
            Ok(output) => Response::Ok(serde_json::to_value(output).expect("output serializes")),
            Err(err) => error_response(err),
        },
        "web-extract" => match web_extract(payload, state, ctx).await {
            Ok(output) => Response::Ok(serde_json::to_value(output).expect("output serializes")),
            Err(err) => error_response(err),
        },
        "web-crawl" => match web_crawl(payload, state, ctx).await {
            Ok(output) => Response::Ok(serde_json::to_value(output).expect("output serializes")),
            Err(err) => error_response(err),
        },
        unknown => Response::Error {
            error: ResponseError {
                kind: "unknown-method".into(),
                detail: format!("{SUBJECT_PREFIX}.{unknown} is not a recognized search method"),
                remediation: "Use tool.search.web-search.".into(),
                tracked_by: "comp-jam-svc-search",
            },
        },
    }
}

fn build_http_client(timeout: Duration) -> Result<reqwest::Client, SearchError> {
    reqwest::Client::builder()
        .timeout(timeout)
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(format!("{SERVICE_NAME}/{SERVICE_VERSION}"))
        .build()
        .map_err(|err| {
            SearchError::protocol(
                "http-client-build-failed",
                err.to_string(),
                "Verify the HTTP client configuration in jam-svc-search.",
                "principle-failure-surfaces-immediately",
            )
        })
}

fn search_api_key(
    env_keys: &[&str],
    file_secret_keys: &[&str],
    pass_secret_keys: &[&str],
    secrets_file: Option<&Path>,
) -> Result<Option<String>, SearchError> {
    if let Some(value) = env_value(env_keys) {
        return Ok(Some(value));
    }
    if let Some(path) = secrets_file {
        if let Some(value) = first_file_secret(path, file_secret_keys)? {
            return Ok(Some(value));
        }
    }
    Ok(first_pass_secret(pass_secret_keys))
}

fn env_value(keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| std::env::var(key).ok())
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn first_file_secret(path: &Path, keys: &[&str]) -> Result<Option<String>, SearchError> {
    let backend = FileBackend::new(path);
    first_secret(&backend, keys, true)
}

fn first_pass_secret(keys: &[&str]) -> Option<String> {
    let backend = PassBackend::new("jam");
    first_secret(&backend, keys, false).unwrap_or(None)
}

fn first_secret(
    backend: &dyn SecretBackend,
    keys: &[&str],
    fail_on_backend_error: bool,
) -> Result<Option<String>, SearchError> {
    for key in keys {
        match backend.get(&SecretKey::new(*key)) {
            Ok(secret) => {
                let value = secret.expose_secret().trim().to_owned();
                if !value.is_empty() {
                    return Ok(Some(value));
                }
            }
            Err(SecretError::NotFound(_)) => {}
            Err(SecretError::Backend(err)) => {
                if fail_on_backend_error {
                    return Err(SearchError::protocol(
                        "invalid-search-secrets-file",
                        format!("failed reading search credential {key}: {err}"),
                        "Fix JAM_SECRETS_FILE or unset it and use env/pass credentials.",
                        "principle-failure-surfaces-immediately",
                    ));
                }
            }
            Err(SecretError::Io(err)) => {
                if fail_on_backend_error {
                    return Err(SearchError::protocol(
                        "invalid-search-secrets-file",
                        format!("failed reading search credential {key}: {err}"),
                        "Fix JAM_SECRETS_FILE or unset it and use env/pass credentials.",
                        "principle-failure-surfaces-immediately",
                    ));
                }
            }
        }
    }
    Ok(None)
}

async fn web_search(
    payload: &[u8],
    state: &SearchState,
    ctx: &TraceCtx,
    nats: &JamNats,
    now: DateTime<Utc>,
) -> Result<WebSearchOutput, SearchError> {
    let input = parse_web_search_input(payload)?;
    validate_search_input(&input)?;
    let backend = select_web_search_backend(&state.config, &input)?;
    let results = match backend {
        WebSearchBackend::Brave => {
            if let Some((until, last_error)) = state.brave_cooldown(now) {
                return Err(SearchError::protocol(
                    "backend-in-cooldown",
                    format!("Brave is in cooldown until {until}; last error: {last_error}"),
                    "Wait for cooldown expiry or configure another backend once supported.",
                    "comp-search-router",
                ));
            }

            match brave_search(&state.config, &input).await {
                Ok(results) => {
                    state.clear_brave_cooldown();
                    results
                }
                Err(err) => {
                    state.set_brave_cooldown(now, err.to_string());
                    return Err(err);
                }
            }
        }
        WebSearchBackend::Searxng => searxng_search(&state.config, &input).await?,
        WebSearchBackend::Linkup => linkup_search(&state.config, &input).await?,
    };
    let output = WebSearchOutput {
        query: routed_query(&input),
        results,
        routing: RoutingEnvelope {
            backend: backend.as_str().into(),
            reason: web_search_routing_reason(&input, backend),
            cooldown_until: None,
        },
        trace_id: ctx.trace_id.to_string(),
    };
    publish_search_event(nats, &output, ctx, now).await?;
    Ok(output)
}

fn select_web_search_backend(
    config: &SearchConfig,
    input: &WebSearchInput,
) -> Result<WebSearchBackend, SearchError> {
    match config.web_search_backend {
        WebSearchBackendMode::Brave => Ok(WebSearchBackend::Brave),
        WebSearchBackendMode::Searxng => Ok(WebSearchBackend::Searxng),
        WebSearchBackendMode::Linkup => Ok(WebSearchBackend::Linkup),
        WebSearchBackendMode::Auto => {
            if is_privacy_intent(input.intent.as_deref()) {
                if config.searxng_endpoint.is_some() {
                    return Ok(WebSearchBackend::Searxng);
                }
                return Err(SearchError::protocol(
                    "capability-unavailable",
                    "privacy-sensitive search requires JAM_SEARXNG_ENDPOINT",
                    "Configure SearXNG or remove the privacy-sensitive search intent.",
                    "comp-searxng-backend",
                ));
            }
            if is_source_backed_intent(input.intent.as_deref()) {
                if config.linkup_api_key.is_some() {
                    return Ok(WebSearchBackend::Linkup);
                }
                return Err(SearchError::protocol(
                    "capability-unavailable",
                    "source-backed search requires a Linkup credential",
                    "Configure Linkup in env, JAM_SECRETS_FILE, or maestro pass; otherwise remove the source-backed search intent.",
                    "comp-linkup-backend",
                ));
            }
            Ok(WebSearchBackend::Brave)
        }
    }
}

fn is_privacy_intent(intent: Option<&str>) -> bool {
    intent.is_some_and(|intent| {
        let intent = intent.to_ascii_lowercase();
        intent.contains("privacy") || intent.contains("private")
    })
}

fn is_source_backed_intent(intent: Option<&str>) -> bool {
    intent.is_some_and(|intent| {
        let intent = intent.to_ascii_lowercase();
        intent.contains("source")
            || intent.contains("citation")
            || intent.contains("cite")
            || intent.contains("ground")
    })
}

async fn web_extract(
    payload: &[u8],
    state: &SearchState,
    ctx: &TraceCtx,
) -> Result<WebExtractOutput, SearchError> {
    let input = parse_web_extract_input(payload)?;
    let use_firecrawl = state.use_firecrawl(input.render_js);
    let urls = validate_extract_input(&input, use_firecrawl)?;
    let include_images = input.include_images.unwrap_or(false);
    if use_firecrawl {
        return firecrawl_extract(state, ctx, &urls, include_images).await;
    }
    let mut contents = Vec::with_capacity(urls.len());
    for url in urls {
        contents.push(
            fetch_and_extract(&state.config, &url, include_images, "api-web-extract")
                .await?
                .content,
        );
    }
    Ok(WebExtractOutput {
        contents,
        routing: direct_fetch_routing("direct URL extraction"),
        trace_id: ctx.trace_id.to_string(),
    })
}

async fn web_crawl(
    payload: &[u8],
    state: &SearchState,
    ctx: &TraceCtx,
) -> Result<WebCrawlOutput, SearchError> {
    let input = parse_web_crawl_input(payload)?;
    let use_firecrawl = state.use_firecrawl(input.render_js);
    let root_url = validate_crawl_input(&input, use_firecrawl)?;
    let include_images = input.include_images.unwrap_or(false);
    let max_pages = input.max_pages.unwrap_or(DEFAULT_CRAWL_MAX_PAGES);
    if use_firecrawl {
        return firecrawl_crawl(state, ctx, &input, &root_url, include_images, max_pages).await;
    }
    let mut queue = VecDeque::from([(root_url.clone(), 0_u32)]);
    let mut seen = HashSet::from([canonical_url_key(&root_url)]);
    let mut pages = Vec::new();

    while let Some((url, depth)) = queue.pop_front() {
        if u32::try_from(pages.len()).unwrap_or(u32::MAX) >= max_pages {
            break;
        }
        let fetched =
            fetch_and_extract(&state.config, &url, include_images, "api-web-crawl").await?;
        if depth < input.max_depth {
            for link in fetched.links {
                if same_origin(&root_url, &link) {
                    let key = canonical_url_key(&link);
                    if seen.insert(key) {
                        queue.push_back((link, depth + 1));
                    }
                }
            }
        }
        pages.push(fetched.content);
    }

    Ok(WebCrawlOutput {
        root_url: root_url.to_string(),
        pages,
        routing: direct_fetch_routing("bounded same-origin crawl"),
        trace_id: ctx.trace_id.to_string(),
    })
}

impl SearchState {
    fn use_firecrawl(&self, render_js: Option<bool>) -> bool {
        self.config.extract_backend == ExtractBackend::Firecrawl || render_js.unwrap_or(false)
    }

    fn brave_cooldown(&self, now: DateTime<Utc>) -> Option<(DateTime<Utc>, String)> {
        let cooldown = self
            .cooldown
            .lock()
            .expect("cooldown mutex is not poisoned");
        let until = cooldown.brave_until?;
        if until > now {
            Some((
                until,
                cooldown
                    .last_error
                    .clone()
                    .unwrap_or_else(|| "unknown backend failure".into()),
            ))
        } else {
            None
        }
    }

    fn set_brave_cooldown(&self, now: DateTime<Utc>, last_error: String) {
        let mut cooldown = self
            .cooldown
            .lock()
            .expect("cooldown mutex is not poisoned");
        cooldown.brave_until = Some(now + chrono::Duration::seconds(COOLDOWN_SECS));
        cooldown.last_error = Some(last_error);
    }

    fn clear_brave_cooldown(&self) {
        let mut cooldown = self
            .cooldown
            .lock()
            .expect("cooldown mutex is not poisoned");
        cooldown.brave_until = None;
        cooldown.last_error = None;
    }
}

async fn brave_search(
    config: &SearchConfig,
    input: &WebSearchInput,
) -> Result<Vec<SearchResult>, SearchError> {
    let url = brave_url(config, input);
    let brave_api_key = config.brave_api_key.as_deref().ok_or_else(|| {
        SearchError::protocol(
            "missing-brave-api-key",
            "Brave Search token is not configured",
            "Set JAM_BRAVE_API_KEY / BRAVE_API_KEY, seed jam/search/brave, or route to another configured search backend.",
            "comp-brave-backend",
        )
    })?;
    let output = Command::new(&config.curl_bin)
        .arg("-fsS")
        .arg("--max-time")
        .arg(config.timeout.as_secs().to_string())
        .arg("-H")
        .arg("Accept: application/json")
        .arg("-H")
        .arg(format!("X-Subscription-Token: {brave_api_key}"))
        .arg(url)
        .output()
        .await
        .map_err(|err| {
            SearchError::protocol(
                "backend-command-failed",
                format!("failed to execute curl for Brave Search: {err}"),
                "Verify curl is installed and JAM_CURL_BIN is correct.",
                "principle-failure-surfaces-immediately",
            )
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        return Err(SearchError::protocol(
            "backend-request-failed",
            if stderr.is_empty() { stdout } else { stderr },
            "Verify Brave API token, endpoint, and network connectivity.",
            "comp-brave-backend",
        ));
    }
    let raw: Value = serde_json::from_slice(&output.stdout).map_err(|err| {
        SearchError::protocol(
            "backend-output-invalid",
            format!("Brave Search returned invalid JSON: {err}"),
            "Update jam-svc-search's Brave parser or inspect the backend response.",
            "comp-brave-backend",
        )
    })?;
    parse_brave_results(&raw)
}

async fn searxng_search(
    config: &SearchConfig,
    input: &WebSearchInput,
) -> Result<Vec<SearchResult>, SearchError> {
    let url = searxng_url(config, input)?;
    let response = config
        .http_client
        .get(url.clone())
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(|err| {
            SearchError::protocol(
                "backend-request-failed",
                format!("failed to query SearXNG at {url}: {err}"),
                "Verify JAM_SEARXNG_ENDPOINT, JSON output support, and network connectivity.",
                "comp-searxng-backend",
            )
        })?;
    read_backend_json(response, "comp-searxng-backend")
        .await
        .and_then(|raw| parse_searxng_results(&raw))
}

async fn linkup_search(
    config: &SearchConfig,
    input: &WebSearchInput,
) -> Result<Vec<SearchResult>, SearchError> {
    let api_key = linkup_api_key(config)?;
    let body = linkup_search_body(config, input);
    let response = config
        .http_client
        .post(&config.linkup_endpoint)
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await
        .map_err(|err| {
            SearchError::protocol(
                "backend-request-failed",
                format!(
                    "failed to query Linkup at {}: {err}",
                    config.linkup_endpoint
                ),
                "Verify JAM_LINKUP_ENDPOINT, API token, quota, and network connectivity.",
                "comp-linkup-backend",
            )
        })?;
    read_backend_json(response, "comp-linkup-backend")
        .await
        .and_then(|raw| parse_linkup_results(&raw))
}

async fn read_backend_json(
    response: reqwest::Response,
    tracked_by: &'static str,
) -> Result<Value, SearchError> {
    let status = response.status();
    let body = response.text().await.map_err(|err| {
        SearchError::protocol(
            "backend-body-read-failed",
            format!("failed to read backend response body: {err}"),
            "Verify network connectivity and retry.",
            tracked_by,
        )
    })?;
    let raw = serde_json::from_str::<Value>(&body).ok();
    if !status.is_success() {
        let detail = raw
            .as_ref()
            .and_then(backend_error_detail)
            .unwrap_or_else(|| truncate_chars(body.trim(), 1_000));
        return Err(SearchError::protocol(
            "backend-request-failed",
            format!("backend returned HTTP {status}: {detail}"),
            "Verify backend endpoint, credentials, quota, and request shape.",
            tracked_by,
        ));
    }
    let raw = raw.ok_or_else(|| {
        SearchError::protocol(
            "backend-output-invalid",
            "backend returned invalid JSON",
            "Update jam-svc-search's backend parser or inspect the backend response.",
            tracked_by,
        )
    })?;
    Ok(raw)
}

fn parse_web_search_input(payload: &[u8]) -> Result<WebSearchInput, SearchError> {
    serde_json::from_slice(payload).map_err(|err| {
        SearchError::protocol(
            "invalid-input",
            format!("tool.search.web-search payload is invalid JSON: {err}"),
            "Send {\"query\":\"...\"}.",
            "api-web-search",
        )
    })
}

fn validate_search_input(input: &WebSearchInput) -> Result<(), SearchError> {
    if input.query.trim().is_empty() {
        return Err(SearchError::protocol(
            "invalid-query",
            "query must not be empty",
            "Send a non-empty search query.",
            "api-web-search",
        ));
    }
    if input.domains.as_ref().is_some_and(|domains| {
        domains
            .iter()
            .any(|domain| domain.trim().is_empty() || domain.contains('/'))
    }) {
        return Err(SearchError::protocol(
            "invalid-domain",
            "domains must be hostnames without slashes",
            "Send domains such as docs.rs or bevyengine.org.",
            "api-web-search",
        ));
    }
    Ok(())
}

fn parse_web_extract_input(payload: &[u8]) -> Result<WebExtractInput, SearchError> {
    serde_json::from_slice(payload).map_err(|err| {
        SearchError::protocol(
            "invalid-input",
            format!("tool.search.web-extract payload is invalid JSON: {err}"),
            "Send {\"urls\":[\"https://example.org\"]}.",
            "api-web-extract",
        )
    })
}

fn parse_web_crawl_input(payload: &[u8]) -> Result<WebCrawlInput, SearchError> {
    serde_json::from_slice(payload).map_err(|err| {
        SearchError::protocol(
            "invalid-input",
            format!("tool.search.web-crawl payload is invalid JSON: {err}"),
            "Send {\"root_url\":\"https://example.org\",\"max_depth\":1}.",
            "api-web-crawl",
        )
    })
}

fn validate_extract_input(
    input: &WebExtractInput,
    allow_render_js: bool,
) -> Result<Vec<Url>, SearchError> {
    validate_render_js(input.render_js, allow_render_js, "api-web-extract")?;
    if input.urls.is_empty() || input.urls.len() > MAX_EXTRACT_URLS {
        return Err(SearchError::protocol(
            "invalid-url-count",
            format!("urls must contain 1..={MAX_EXTRACT_URLS} entries"),
            "Send a small batch of public HTTP(S) URLs.",
            "api-web-extract",
        ));
    }
    input
        .urls
        .iter()
        .map(|raw| validate_public_http_url(raw, "api-web-extract"))
        .collect()
}

fn validate_crawl_input(input: &WebCrawlInput, allow_render_js: bool) -> Result<Url, SearchError> {
    validate_render_js(input.render_js, allow_render_js, "api-web-crawl")?;
    if input.max_depth > MAX_CRAWL_DEPTH {
        return Err(SearchError::protocol(
            "invalid-depth",
            format!("max_depth must be <= {MAX_CRAWL_DEPTH}"),
            "Keep crawls bounded; use request-research for broad web exploration.",
            "api-web-crawl",
        ));
    }
    let max_pages = input.max_pages.unwrap_or(DEFAULT_CRAWL_MAX_PAGES);
    if max_pages == 0 || max_pages > MAX_CRAWL_PAGES {
        return Err(SearchError::protocol(
            "invalid-max-pages",
            format!("max_pages must be 1..={MAX_CRAWL_PAGES}"),
            "Keep crawls bounded; use request-research for broad web exploration.",
            "api-web-crawl",
        ));
    }
    validate_public_http_url(&input.root_url, "api-web-crawl")
}

fn validate_render_js(
    render_js: Option<bool>,
    allow_render_js: bool,
    tracked_by: &'static str,
) -> Result<(), SearchError> {
    if render_js.unwrap_or(false) && !allow_render_js {
        return Err(SearchError::protocol(
            "capability-unavailable",
            "render_js requires a JavaScript-capable extraction backend",
            "Seed jam/search/firecrawl and set JAM_SEARCH_EXTRACT_BACKEND=firecrawl, or omit render_js.",
            tracked_by,
        ));
    }
    Ok(())
}

fn validate_public_http_url(raw: &str, tracked_by: &'static str) -> Result<Url, SearchError> {
    let mut url = Url::parse(raw.trim()).map_err(|err| {
        SearchError::protocol(
            "invalid-url",
            format!("URL is invalid: {err}"),
            "Send an absolute public http:// or https:// URL.",
            tracked_by,
        )
    })?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(SearchError::protocol(
            "invalid-url-scheme",
            format!("unsupported URL scheme: {}", url.scheme()),
            "Send an absolute public http:// or https:// URL.",
            tracked_by,
        ));
    }
    let host = url.host_str().ok_or_else(|| {
        SearchError::protocol(
            "invalid-url-host",
            "URL must include a host",
            "Send an absolute public http:// or https:// URL.",
            tracked_by,
        )
    })?;
    if is_blocked_host(host) {
        return Err(SearchError::protocol(
            "blocked-url-host",
            format!("URL host is not allowed: {host}"),
            "Use public internet URLs only; local/private addresses are blocked.",
            tracked_by,
        ));
    }
    url.set_fragment(None);
    Ok(url)
}

fn is_blocked_host(host: &str) -> bool {
    let host = host
        .trim_matches(|ch| ch == '[' || ch == ']')
        .to_ascii_lowercase();
    if host == "localhost" || host.ends_with(".localhost") {
        return true;
    }
    host.parse::<IpAddr>().is_ok_and(is_blocked_ip)
}

fn is_blocked_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            ip.is_loopback()
                || ip.is_private()
                || ip.is_link_local()
                || ip.is_unspecified()
                || ip.is_broadcast()
                || ip.is_multicast()
        }
        IpAddr::V6(ip) => {
            ip.is_loopback()
                || ip.is_unspecified()
                || ip.is_multicast()
                || ip.is_unique_local()
                || ip.is_unicast_link_local()
        }
    }
}

async fn fetch_and_extract(
    config: &SearchConfig,
    url: &Url,
    include_images: bool,
    tracked_by: &'static str,
) -> Result<FetchedPage, SearchError> {
    let response = config
        .http_client
        .get(url.clone())
        .header(
            reqwest::header::ACCEPT,
            "text/html,application/xhtml+xml,text/plain;q=0.9,*/*;q=0.1",
        )
        .send()
        .await
        .map_err(|err| {
            SearchError::protocol(
                "extract-request-failed",
                format!("failed to fetch {url}: {err}"),
                "Verify network connectivity and the URL.",
                tracked_by,
            )
        })?;
    let status = response.status();
    if !status.is_success() {
        return Err(SearchError::protocol(
            "extract-request-failed",
            format!("{url} returned HTTP {status}"),
            "Verify the URL is reachable without authentication or redirects.",
            tracked_by,
        ));
    }
    let final_url = response.url().clone();
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_owned();
    let body = response.text().await.map_err(|err| {
        SearchError::protocol(
            "extract-body-read-failed",
            format!("failed to read {url}: {err}"),
            "Verify the URL returns text content.",
            tracked_by,
        )
    })?;
    Ok(extract_document(
        &final_url,
        &body,
        &content_type,
        include_images,
    ))
}

fn extract_document(
    url: &Url,
    body: &str,
    content_type: &str,
    include_images: bool,
) -> FetchedPage {
    if !looks_like_html(content_type, body) {
        return FetchedPage {
            content: ExtractedContent {
                url: url.to_string(),
                title: None,
                text: truncate_chars(&collapse_whitespace(body), MAX_EXTRACTED_TEXT_CHARS),
                images: Vec::new(),
            },
            links: Vec::new(),
        };
    }

    let document = Html::parse_document(body);
    let title_selector = Selector::parse("title").expect("static title selector");
    let body_selector = Selector::parse("body").expect("static body selector");
    let link_selector = Selector::parse("a[href]").expect("static link selector");
    let image_selector = Selector::parse("img[src]").expect("static image selector");

    let title = document
        .select(&title_selector)
        .next()
        .map(|title| collapse_whitespace(&title.text().collect::<Vec<_>>().join(" ")))
        .filter(|title| !title.is_empty());
    let text_source = document.select(&body_selector).next().map_or_else(
        || document.root_element().text().collect::<Vec<_>>().join(" "),
        |body| body.text().collect::<Vec<_>>().join(" "),
    );
    let text = truncate_chars(&collapse_whitespace(&text_source), MAX_EXTRACTED_TEXT_CHARS);
    let images = if include_images {
        document
            .select(&image_selector)
            .filter_map(|image| image.value().attr("src"))
            .filter_map(|src| url.join(src).ok())
            .map(|url| url.to_string())
            .collect()
    } else {
        Vec::new()
    };
    let links = document
        .select(&link_selector)
        .filter_map(|link| link.value().attr("href"))
        .filter_map(|href| url.join(href).ok())
        .filter(|url| validate_public_http_url(url.as_str(), "api-web-crawl").is_ok())
        .map(canonical_url)
        .collect();

    FetchedPage {
        content: ExtractedContent {
            url: url.to_string(),
            title,
            text,
            images,
        },
        links,
    }
}

fn looks_like_html(content_type: &str, body: &str) -> bool {
    content_type.contains("text/html")
        || content_type.contains("application/xhtml+xml")
        || body.trim_start().starts_with("<!doctype html")
        || body.trim_start().starts_with("<html")
}

fn collapse_whitespace(raw: &str) -> String {
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn truncate_chars(raw: &str, max_chars: usize) -> String {
    raw.chars().take(max_chars).collect()
}

fn canonical_url(mut url: Url) -> Url {
    url.set_fragment(None);
    url
}

fn canonical_url_key(url: &Url) -> String {
    let mut url = url.clone();
    url.set_fragment(None);
    url.to_string()
}

fn same_origin(left: &Url, right: &Url) -> bool {
    left.scheme() == right.scheme()
        && left.host_str() == right.host_str()
        && left.port_or_known_default() == right.port_or_known_default()
}

fn direct_fetch_routing(reason: &str) -> RoutingEnvelope {
    RoutingEnvelope {
        backend: "direct-fetch".into(),
        reason: reason.into(),
        cooldown_until: None,
    }
}

fn firecrawl_routing(reason: &str) -> RoutingEnvelope {
    RoutingEnvelope {
        backend: "firecrawl".into(),
        reason: reason.into(),
        cooldown_until: None,
    }
}

async fn firecrawl_extract(
    state: &SearchState,
    ctx: &TraceCtx,
    urls: &[Url],
    include_images: bool,
) -> Result<WebExtractOutput, SearchError> {
    let api_key = firecrawl_api_key(&state.config, "api-web-extract")?.to_owned();
    let mut contents = Vec::with_capacity(urls.len());
    for url in urls {
        let body = firecrawl_scrape_body(url, include_images, state.config.timeout);
        let raw = firecrawl_post_json(&state.config, &api_key, "scrape", &body, "api-web-extract")
            .await?;
        let data = firecrawl_response_data(&raw, "api-web-extract")?;
        contents.push(parse_firecrawl_page(url, data, include_images, "api-web-extract")?.content);
    }
    Ok(WebExtractOutput {
        contents,
        routing: firecrawl_routing("Firecrawl v2 scrape extraction"),
        trace_id: ctx.trace_id.to_string(),
    })
}

async fn firecrawl_crawl(
    state: &SearchState,
    ctx: &TraceCtx,
    input: &WebCrawlInput,
    root_url: &Url,
    include_images: bool,
    max_pages: u32,
) -> Result<WebCrawlOutput, SearchError> {
    let api_key = firecrawl_api_key(&state.config, "api-web-crawl")?.to_owned();
    let body = firecrawl_crawl_body(
        input,
        root_url,
        include_images,
        max_pages,
        state.config.timeout,
    );
    let started =
        firecrawl_post_json(&state.config, &api_key, "crawl", &body, "api-web-crawl").await?;
    if firecrawl_status(&started).as_deref() == Some("completed") {
        return firecrawl_crawl_output(
            &state.config,
            ctx,
            root_url,
            &api_key,
            &started,
            include_images,
            max_pages,
        )
        .await;
    }

    let status_url = firecrawl_crawl_poll_url(&state.config, &started, "api-web-crawl")?;
    let deadline = tokio::time::Instant::now() + state.config.timeout;
    loop {
        if tokio::time::Instant::now() >= deadline {
            return Err(SearchError::protocol(
                "firecrawl-crawl-timeout",
                format!(
                    "Firecrawl crawl for {root_url} did not complete within {} seconds",
                    state.config.timeout.as_secs()
                ),
                "Increase JAM_SEARCH_TIMEOUT_SECS or retry a smaller crawl.",
                "api-web-crawl",
            ));
        }

        let raw = firecrawl_get_json(&state.config, &api_key, &status_url, "api-web-crawl").await?;
        match firecrawl_status(&raw).as_deref() {
            Some("completed") => {
                return firecrawl_crawl_output(
                    &state.config,
                    ctx,
                    root_url,
                    &api_key,
                    &raw,
                    include_images,
                    max_pages,
                )
                .await;
            }
            Some("failed" | "cancelled" | "canceled") => {
                return Err(SearchError::protocol(
                    "firecrawl-crawl-failed",
                    firecrawl_error_detail(&raw)
                        .unwrap_or_else(|| format!("Firecrawl crawl for {root_url} failed")),
                    "Inspect the Firecrawl job, reduce crawl scope, or fall back to direct fetch.",
                    "api-web-crawl",
                ));
            }
            Some("scraping" | "queued" | "waiting" | "pending" | "running") => {}
            Some(status) => {
                return Err(SearchError::protocol(
                    "firecrawl-output-invalid",
                    format!("Firecrawl returned unknown crawl status: {status}"),
                    "Update jam-svc-search's Firecrawl parser or inspect the backend response.",
                    "comp-firecrawl-backend",
                ));
            }
            None => {
                return Err(SearchError::protocol(
                    "firecrawl-output-invalid",
                    "Firecrawl crawl status response missing status",
                    "Update jam-svc-search's Firecrawl parser or inspect the backend response.",
                    "comp-firecrawl-backend",
                ));
            }
        }

        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        tokio::time::sleep(remaining.min(Duration::from_secs(1))).await;
    }
}

async fn firecrawl_crawl_output(
    config: &SearchConfig,
    ctx: &TraceCtx,
    root_url: &Url,
    api_key: &str,
    raw: &Value,
    include_images: bool,
    max_pages: u32,
) -> Result<WebCrawlOutput, SearchError> {
    let pages =
        firecrawl_collect_crawl_pages(config, root_url, api_key, raw, include_images, max_pages)
            .await?;
    Ok(WebCrawlOutput {
        root_url: root_url.to_string(),
        pages,
        routing: firecrawl_routing("Firecrawl v2 bounded crawl"),
        trace_id: ctx.trace_id.to_string(),
    })
}

async fn firecrawl_collect_crawl_pages(
    config: &SearchConfig,
    root_url: &Url,
    api_key: &str,
    raw: &Value,
    include_images: bool,
    max_pages: u32,
) -> Result<Vec<ExtractedContent>, SearchError> {
    let limit = usize::try_from(max_pages).unwrap_or(usize::MAX);
    let mut pages = Vec::new();
    pages.extend(parse_firecrawl_crawl_pages(
        root_url,
        raw,
        include_images,
        limit,
        "api-web-crawl",
    )?);

    let mut next_url = firecrawl_next_url(raw).map(str::to_owned);
    while pages.len() < limit {
        let Some(next) = next_url else {
            break;
        };
        let next_url_trusted = trusted_firecrawl_url(config, &next, "api-web-crawl")?;
        let next_raw =
            firecrawl_get_json(config, api_key, &next_url_trusted, "api-web-crawl").await?;
        pages.extend(parse_firecrawl_crawl_pages(
            root_url,
            &next_raw,
            include_images,
            limit.saturating_sub(pages.len()),
            "api-web-crawl",
        )?);
        next_url = firecrawl_next_url(&next_raw).map(str::to_owned);
    }
    Ok(pages)
}

async fn firecrawl_post_json(
    config: &SearchConfig,
    api_key: &str,
    path: &str,
    body: &Value,
    tracked_by: &'static str,
) -> Result<Value, SearchError> {
    let url = endpoint_join(&config.firecrawl_endpoint, path);
    let response = config
        .http_client
        .post(&url)
        .bearer_auth(api_key)
        .json(body)
        .send()
        .await
        .map_err(|err| {
            SearchError::protocol(
                "firecrawl-request-failed",
                format!("failed to POST {url}: {err}"),
                "Verify Firecrawl endpoint, API token, and network connectivity.",
                tracked_by,
            )
        })?;
    firecrawl_read_json(response, tracked_by).await
}

async fn firecrawl_get_json(
    config: &SearchConfig,
    api_key: &str,
    url: &str,
    tracked_by: &'static str,
) -> Result<Value, SearchError> {
    let response = config
        .http_client
        .get(url)
        .bearer_auth(api_key)
        .send()
        .await
        .map_err(|err| {
            SearchError::protocol(
                "firecrawl-request-failed",
                format!("failed to GET {url}: {err}"),
                "Verify Firecrawl endpoint, API token, and network connectivity.",
                tracked_by,
            )
        })?;
    firecrawl_read_json(response, tracked_by).await
}

async fn firecrawl_read_json(
    response: reqwest::Response,
    tracked_by: &'static str,
) -> Result<Value, SearchError> {
    let status = response.status();
    let body = response.text().await.map_err(|err| {
        SearchError::protocol(
            "firecrawl-body-read-failed",
            format!("failed to read Firecrawl response body: {err}"),
            "Verify network connectivity and retry.",
            tracked_by,
        )
    })?;
    let raw = serde_json::from_str::<Value>(&body).ok();
    if !status.is_success() {
        let detail = raw
            .as_ref()
            .and_then(firecrawl_error_detail)
            .unwrap_or_else(|| truncate_chars(body.trim(), 1_000));
        return Err(SearchError::protocol(
            "firecrawl-request-failed",
            format!("Firecrawl returned HTTP {status}: {detail}"),
            "Verify Firecrawl endpoint, API token, quota, and request shape.",
            tracked_by,
        ));
    }
    let raw = raw.ok_or_else(|| {
        SearchError::protocol(
            "firecrawl-output-invalid",
            "Firecrawl returned non-JSON output",
            "Update jam-svc-search's Firecrawl parser or inspect the backend response.",
            "comp-firecrawl-backend",
        )
    })?;
    if raw.get("success").and_then(Value::as_bool) == Some(false) {
        return Err(SearchError::protocol(
            "firecrawl-request-failed",
            firecrawl_error_detail(&raw)
                .unwrap_or_else(|| "Firecrawl reported success=false".into()),
            "Verify Firecrawl endpoint, API token, quota, and request shape.",
            tracked_by,
        ));
    }
    Ok(raw)
}

fn firecrawl_api_key<'a>(
    config: &'a SearchConfig,
    tracked_by: &'static str,
) -> Result<&'a str, SearchError> {
    config
        .firecrawl_api_key
        .as_deref()
        .filter(|key| !key.trim().is_empty())
        .ok_or_else(|| {
            SearchError::protocol(
                "capability-unavailable",
                "Firecrawl extraction requires a Firecrawl credential",
                "Seed jam/search/firecrawl in env, JAM_SECRETS_FILE, or maestro pass; otherwise omit render_js.",
                tracked_by,
            )
        })
}

fn firecrawl_scrape_body(url: &Url, include_images: bool, timeout: Duration) -> Value {
    serde_json::json!({
        "url": url.as_str(),
        "formats": firecrawl_formats(include_images),
        "onlyMainContent": true,
        "timeout": timeout_ms(timeout),
    })
}

fn firecrawl_crawl_body(
    input: &WebCrawlInput,
    root_url: &Url,
    include_images: bool,
    max_pages: u32,
    timeout: Duration,
) -> Value {
    serde_json::json!({
        "url": root_url.as_str(),
        "limit": max_pages,
        "maxDiscoveryDepth": input.max_depth,
        "allowExternalLinks": false,
        "allowSubdomains": false,
        "scrapeOptions": {
            "formats": firecrawl_formats(include_images),
            "onlyMainContent": true,
            "timeout": timeout_ms(timeout),
        },
    })
}

fn firecrawl_formats(include_images: bool) -> Vec<&'static str> {
    let mut formats = vec!["markdown", "links"];
    if include_images {
        formats.push("images");
    }
    formats
}

fn timeout_ms(timeout: Duration) -> u64 {
    u64::try_from(timeout.as_millis()).unwrap_or(u64::MAX)
}

fn firecrawl_response_data<'a>(
    raw: &'a Value,
    tracked_by: &'static str,
) -> Result<&'a Value, SearchError> {
    raw.get("data").ok_or_else(|| {
        SearchError::protocol(
            "firecrawl-output-invalid",
            "Firecrawl scrape response missing data object",
            "Update jam-svc-search's Firecrawl parser or inspect the backend response.",
            tracked_by,
        )
    })
}

fn parse_firecrawl_crawl_pages(
    root_url: &Url,
    raw: &Value,
    include_images: bool,
    remaining: usize,
    tracked_by: &'static str,
) -> Result<Vec<ExtractedContent>, SearchError> {
    let pages = raw.get("data").and_then(Value::as_array).ok_or_else(|| {
        SearchError::protocol(
            "firecrawl-output-invalid",
            "Firecrawl crawl status response missing data[]",
            "Update jam-svc-search's Firecrawl parser or inspect the backend response.",
            "comp-firecrawl-backend",
        )
    })?;
    pages
        .iter()
        .take(remaining)
        .map(|page| Ok(parse_firecrawl_page(root_url, page, include_images, tracked_by)?.content))
        .collect()
}

fn parse_firecrawl_page(
    requested_url: &Url,
    raw: &Value,
    include_images: bool,
    tracked_by: &'static str,
) -> Result<FetchedPage, SearchError> {
    let page_url = firecrawl_page_url(requested_url, raw, tracked_by)?;
    let text = firecrawl_text(&page_url, raw).ok_or_else(|| {
        SearchError::protocol(
            "firecrawl-output-invalid",
            "Firecrawl page missing markdown/html/rawHtml/text content",
            "Update jam-svc-search's Firecrawl parser or inspect the backend response.",
            "comp-firecrawl-backend",
        )
    })?;
    Ok(FetchedPage {
        content: ExtractedContent {
            url: page_url.to_string(),
            title: firecrawl_title(raw),
            text,
            images: firecrawl_images(&page_url, raw, include_images),
        },
        links: firecrawl_links(&page_url, raw),
    })
}

fn firecrawl_page_url(
    requested_url: &Url,
    raw: &Value,
    tracked_by: &'static str,
) -> Result<Url, SearchError> {
    let Some(raw_url) = firecrawl_metadata_string(raw, "sourceURL")
        .or_else(|| firecrawl_metadata_string(raw, "url"))
        .or_else(|| raw.get("url").and_then(Value::as_str))
    else {
        return Ok(requested_url.clone());
    };
    let joined = requested_url.join(raw_url).map_err(|err| {
        SearchError::protocol(
            "firecrawl-output-invalid",
            format!("Firecrawl returned invalid page URL {raw_url:?}: {err}"),
            "Update jam-svc-search's Firecrawl parser or inspect the backend response.",
            tracked_by,
        )
    })?;
    validate_public_http_url(joined.as_str(), tracked_by)
}

fn firecrawl_text(page_url: &Url, raw: &Value) -> Option<String> {
    for key in ["markdown", "text", "summary"] {
        if let Some(text) = raw.get(key).and_then(Value::as_str).map(str::trim) {
            if !text.is_empty() {
                return Some(truncate_chars(text, MAX_EXTRACTED_TEXT_CHARS));
            }
        }
    }
    for key in ["html", "rawHtml"] {
        if let Some(html) = raw.get(key).and_then(Value::as_str).map(str::trim) {
            if !html.is_empty() {
                return Some(
                    extract_document(page_url, html, "text/html", false)
                        .content
                        .text,
                );
            }
        }
    }
    None
}

fn firecrawl_title(raw: &Value) -> Option<String> {
    firecrawl_metadata_string(raw, "title")
        .or_else(|| raw.get("title").and_then(Value::as_str))
        .map(collapse_whitespace)
        .filter(|title| !title.is_empty())
}

fn firecrawl_links(page_url: &Url, raw: &Value) -> Vec<Url> {
    let Some(links) = raw.get("links").and_then(Value::as_array) else {
        return Vec::new();
    };
    links
        .iter()
        .filter_map(Value::as_str)
        .filter_map(|link| page_url.join(link).ok())
        .filter(|url| validate_public_http_url(url.as_str(), "api-web-crawl").is_ok())
        .map(canonical_url)
        .collect()
}

fn firecrawl_images(page_url: &Url, raw: &Value, include_images: bool) -> Vec<String> {
    if !include_images {
        return Vec::new();
    }
    let mut seen = HashSet::new();
    let mut images = Vec::new();
    if let Some(raw_images) = raw.get("images").and_then(Value::as_array) {
        for image in raw_images.iter().filter_map(Value::as_str) {
            push_public_url_string(page_url, image, &mut seen, &mut images);
        }
    }
    for key in ["ogImage", "og:image", "image"] {
        if let Some(image) = firecrawl_metadata_string(raw, key) {
            push_public_url_string(page_url, image, &mut seen, &mut images);
        }
    }
    images
}

fn push_public_url_string(
    base_url: &Url,
    raw: &str,
    seen: &mut HashSet<String>,
    output: &mut Vec<String>,
) {
    if let Ok(url) = base_url.join(raw) {
        if validate_public_http_url(url.as_str(), "api-web-extract").is_ok() {
            let canonical = canonical_url(url).to_string();
            if seen.insert(canonical.clone()) {
                output.push(canonical);
            }
        }
    }
}

fn firecrawl_metadata_string<'a>(raw: &'a Value, key: &str) -> Option<&'a str> {
    raw.get("metadata")
        .and_then(|metadata| metadata.get(key))
        .and_then(Value::as_str)
}

fn firecrawl_status(raw: &Value) -> Option<String> {
    raw.get("status")
        .and_then(Value::as_str)
        .map(|status| status.trim().to_ascii_lowercase())
        .filter(|status| !status.is_empty())
}

fn firecrawl_next_url(raw: &Value) -> Option<&str> {
    raw.get("next")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|next| !next.is_empty())
}

fn firecrawl_crawl_poll_url(
    config: &SearchConfig,
    raw: &Value,
    tracked_by: &'static str,
) -> Result<String, SearchError> {
    if let Some(id) = raw
        .get("id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|id| !id.is_empty())
    {
        return Ok(endpoint_join(
            &config.firecrawl_endpoint,
            &format!("crawl/{id}"),
        ));
    }
    let Some(url) = raw
        .get("url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|url| !url.is_empty())
    else {
        return Err(SearchError::protocol(
            "firecrawl-output-invalid",
            "Firecrawl crawl start response missing id or url",
            "Update jam-svc-search's Firecrawl parser or inspect the backend response.",
            "comp-firecrawl-backend",
        ));
    };
    trusted_firecrawl_url(config, url, tracked_by)
}

fn trusted_firecrawl_url(
    config: &SearchConfig,
    raw: &str,
    tracked_by: &'static str,
) -> Result<String, SearchError> {
    let base = Url::parse(&format!(
        "{}/",
        normalize_endpoint(&config.firecrawl_endpoint)
    ))
    .map_err(|err| {
        SearchError::protocol(
            "invalid-firecrawl-endpoint",
            format!("JAM_FIRECRAWL_ENDPOINT is invalid: {err}"),
            "Set JAM_FIRECRAWL_ENDPOINT to an absolute Firecrawl API base URL.",
            "comp-firecrawl-backend",
        )
    })?;
    let candidate = base.join(raw).map_err(|err| {
        SearchError::protocol(
            "firecrawl-output-invalid",
            format!("Firecrawl returned invalid status URL {raw:?}: {err}"),
            "Update jam-svc-search's Firecrawl parser or inspect the backend response.",
            tracked_by,
        )
    })?;
    if !same_origin(&base, &candidate) {
        return Err(SearchError::protocol(
            "firecrawl-output-invalid",
            format!("Firecrawl returned cross-origin status URL: {candidate}"),
            "Inspect the backend response before allowing cross-origin polling.",
            "comp-firecrawl-backend",
        ));
    }
    Ok(candidate.to_string())
}

fn firecrawl_error_detail(raw: &Value) -> Option<String> {
    for key in ["error", "message", "detail"] {
        if let Some(text) = raw.get(key).and_then(Value::as_str).map(str::trim) {
            if !text.is_empty() {
                return Some(text.to_owned());
            }
        }
    }
    raw.get("data")
        .and_then(|data| data.get("metadata"))
        .and_then(|metadata| metadata.get("error"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(ToOwned::to_owned)
}

fn normalize_endpoint(raw: &str) -> String {
    raw.trim().trim_end_matches('/').to_owned()
}

fn endpoint_join(base: &str, path: &str) -> String {
    format!(
        "{}/{}",
        normalize_endpoint(base),
        path.trim_start_matches('/')
    )
}

fn searxng_url(config: &SearchConfig, input: &WebSearchInput) -> Result<Url, SearchError> {
    let endpoint = config.searxng_endpoint.as_deref().ok_or_else(|| {
        SearchError::protocol(
            "capability-unavailable",
            "SearXNG search requires JAM_SEARXNG_ENDPOINT",
            "Configure a SearXNG instance with JSON output enabled.",
            "comp-searxng-backend",
        )
    })?;
    let mut url = Url::parse(endpoint).map_err(|err| {
        SearchError::protocol(
            "invalid-searxng-endpoint",
            format!("JAM_SEARXNG_ENDPOINT is invalid: {err}"),
            "Set JAM_SEARXNG_ENDPOINT to an absolute SearXNG search URL.",
            "comp-searxng-backend",
        )
    })?;
    if matches!(url.path(), "" | "/") {
        url.set_path("search");
    }
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("q", &routed_query(input));
        query.append_pair("format", "json");
        query.append_pair("pageno", "1");
        if let Some(time_range) = searxng_time_range(input.time_range.as_deref()) {
            query.append_pair("time_range", time_range);
        }
    }
    Ok(url)
}

fn searxng_time_range(time_range: Option<&str>) -> Option<&'static str> {
    match time_range.map(str::trim) {
        Some("day" | "24h" | "past-day") => Some("day"),
        Some("month" | "30d" | "past-month") => Some("month"),
        Some("year" | "365d" | "past-year") => Some("year"),
        _ => None,
    }
}

fn linkup_search_body(config: &SearchConfig, input: &WebSearchInput) -> Value {
    let mut body = serde_json::json!({
        "q": input.query.trim(),
        "depth": config.linkup_depth,
        "outputType": "searchResults",
        "maxResults": config.result_count,
    });
    if let Some(domains) = normalized_domains(input) {
        body["includeDomains"] = serde_json::json!(domains);
    }
    body
}

fn normalized_domains(input: &WebSearchInput) -> Option<Vec<String>> {
    let mut seen = HashSet::new();
    let domains = input
        .domains
        .as_ref()?
        .iter()
        .map(|domain| domain.trim())
        .filter(|domain| !domain.is_empty())
        .filter_map(|domain| {
            if seen.insert(domain.to_owned()) {
                Some(domain.to_owned())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    (!domains.is_empty()).then_some(domains)
}

fn linkup_api_key(config: &SearchConfig) -> Result<&str, SearchError> {
    config
        .linkup_api_key
        .as_deref()
        .filter(|key| !key.trim().is_empty())
        .ok_or_else(|| {
            SearchError::protocol(
                "capability-unavailable",
                "Linkup search requires a Linkup credential",
                "Seed jam/search/linkup in env, JAM_SECRETS_FILE, or maestro pass.",
                "comp-linkup-backend",
            )
        })
}

fn validate_linkup_depth(depth: &str) -> Result<(), SearchError> {
    match depth {
        "fast" | "standard" | "deep" => Ok(()),
        other => Err(SearchError::protocol(
            "invalid-linkup-depth",
            format!("unknown JAM_LINKUP_DEPTH value: {other}"),
            "Use fast, standard, or deep.",
            "comp-linkup-backend",
        )),
    }
}

fn brave_url(config: &SearchConfig, input: &WebSearchInput) -> String {
    let mut params = vec![
        format!("q={}", percent_encode(&routed_query(input))),
        format!("count={}", config.result_count),
    ];
    if let Some(freshness) = brave_freshness(input.time_range.as_deref()) {
        params.push(format!("freshness={freshness}"));
    }
    format!("{}?{}", config.brave_endpoint, params.join("&"))
}

fn routed_query(input: &WebSearchInput) -> String {
    let mut query = input.query.trim().to_owned();
    if let Some(domains) = &input.domains {
        let mut seen = HashSet::new();
        for domain in domains {
            let domain = domain.trim();
            if !domain.is_empty() && seen.insert(domain.to_owned()) {
                query.push_str(" site:");
                query.push_str(domain);
            }
        }
    }
    query
}

fn brave_freshness(time_range: Option<&str>) -> Option<&'static str> {
    match time_range.map(str::trim) {
        Some("day" | "24h" | "past-day") => Some("pd"),
        Some("week" | "7d" | "past-week") => Some("pw"),
        Some("month" | "30d" | "past-month") => Some("pm"),
        Some("year" | "365d" | "past-year") => Some("py"),
        _ => None,
    }
}

fn web_search_routing_reason(input: &WebSearchInput, backend: WebSearchBackend) -> String {
    let intent = input.intent.as_deref().unwrap_or("fast factual lookup");
    match backend {
        WebSearchBackend::Brave => format!("Brave is the configured starter backend for {intent}"),
        WebSearchBackend::Searxng => {
            format!("SearXNG selected for privacy-sensitive search intent: {intent}")
        }
        WebSearchBackend::Linkup => {
            format!("Linkup selected for source-backed search intent: {intent}")
        }
    }
}

fn parse_brave_results(raw: &Value) -> Result<Vec<SearchResult>, SearchError> {
    let Some(results) = raw
        .get("web")
        .and_then(|web| web.get("results"))
        .and_then(Value::as_array)
    else {
        return Err(SearchError::protocol(
            "backend-output-invalid",
            "Brave Search JSON missing web.results[]",
            "Update jam-svc-search's Brave parser or inspect the backend response.",
            "comp-brave-backend",
        ));
    };
    Ok(results
        .iter()
        .filter_map(|item| {
            Some(SearchResult {
                title: item.get("title")?.as_str()?.to_owned(),
                url: item.get("url")?.as_str()?.to_owned(),
                snippet: item
                    .get("description")
                    .or_else(|| item.get("snippet"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned(),
            })
        })
        .collect())
}

fn parse_searxng_results(raw: &Value) -> Result<Vec<SearchResult>, SearchError> {
    let results = raw
        .get("results")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            SearchError::protocol(
                "backend-output-invalid",
                "SearXNG JSON missing results[]",
                "Enable JSON output in SearXNG or update jam-svc-search's parser.",
                "comp-searxng-backend",
            )
        })?;
    Ok(results
        .iter()
        .filter_map(|item| {
            Some(SearchResult {
                title: item.get("title")?.as_str()?.to_owned(),
                url: item.get("url")?.as_str()?.to_owned(),
                snippet: item
                    .get("content")
                    .or_else(|| item.get("snippet"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned(),
            })
        })
        .collect())
}

fn parse_linkup_results(raw: &Value) -> Result<Vec<SearchResult>, SearchError> {
    let results = raw
        .get("results")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            SearchError::protocol(
                "backend-output-invalid",
                "Linkup JSON missing results[]",
                "Update jam-svc-search's Linkup parser or inspect the backend response.",
                "comp-linkup-backend",
            )
        })?;
    Ok(results
        .iter()
        .filter_map(|item| {
            Some(SearchResult {
                title: item
                    .get("name")
                    .or_else(|| item.get("title"))?
                    .as_str()?
                    .to_owned(),
                url: item.get("url")?.as_str()?.to_owned(),
                snippet: item
                    .get("content")
                    .or_else(|| item.get("snippet"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned(),
            })
        })
        .collect())
}

fn backend_error_detail(raw: &Value) -> Option<String> {
    for key in ["error", "message", "detail"] {
        if let Some(text) = raw.get(key).and_then(Value::as_str).map(str::trim) {
            if !text.is_empty() {
                return Some(text.to_owned());
            }
        }
    }
    None
}

async fn publish_search_event(
    nats: &JamNats,
    output: &WebSearchOutput,
    ctx: &TraceCtx,
    ts: DateTime<Utc>,
) -> Result<(), SearchError> {
    let payload = SearchWebSearch {
        query: output.query.clone(),
        backend: output.routing.backend.clone(),
        routing_reason: output.routing.reason.clone(),
        result_count: u32::try_from(output.results.len()).unwrap_or(u32::MAX),
        ts,
    };
    let envelope = EventEnvelope::new(
        SearchWebSearch::EVENT_TYPE,
        SearchWebSearch::EVENT_SUBTYPE_VERSION,
        0,
        ctx.trace_id.to_string(),
        SERVICE_NAME,
        payload,
    );
    nats.publish_traced(
        format!("journal.{}", SearchWebSearch::EVENT_TYPE),
        &envelope,
        ctx,
    )
    .await
    .map_err(|err| {
        SearchError::protocol(
            "journal-publish-failed",
            err.to_string(),
            "Verify NATS is running and jam-nats-bridge is healthy.",
            "principle-failure-surfaces-immediately",
        )
    })
}

fn error_response(err: SearchError) -> Response {
    match err {
        SearchError::Protocol {
            kind,
            detail,
            remediation,
            tracked_by,
        } => Response::Error {
            error: ResponseError {
                kind: kind.into(),
                detail,
                remediation: remediation.into(),
                tracked_by,
            },
        },
    }
}

fn method_from_subject(subject: &str) -> Option<&str> {
    let (before_last, last) = subject.rsplit_once('.')?;
    let previous = before_last
        .rsplit_once('.')
        .map_or(before_last, |(_, previous)| previous);
    if matches!(previous, "ping" | "drain") {
        Some(previous)
    } else {
        Some(last)
    }
}

fn configured_subject_prefix(service_env: &str, default_prefix: &str) -> String {
    std::env::var(service_env)
        .or_else(|_| std::env::var("JAM_TOOL_SUBJECT_PREFIX"))
        .ok()
        .filter(|prefix| !prefix.trim().is_empty())
        .unwrap_or_else(|| default_prefix.into())
}

fn percent_encode(raw: &str) -> String {
    let mut encoded = String::new();
    for byte in raw.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(char::from(byte));
        } else if byte == b' ' {
            encoded.push('+');
        } else {
            let _ = write!(encoded, "%{byte:02X}");
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn test_config() -> SearchConfig {
        let timeout = Duration::from_secs(5);
        SearchConfig {
            curl_bin: PathBuf::from("curl"),
            brave_endpoint: "https://example.test/search".into(),
            brave_api_key: Some("token".into()),
            web_search_backend: WebSearchBackendMode::Auto,
            searxng_endpoint: Some("https://searx.example/search".into()),
            linkup_endpoint: DEFAULT_LINKUP_ENDPOINT.into(),
            linkup_api_key: Some("linkup-token".into()),
            linkup_depth: DEFAULT_LINKUP_DEPTH.into(),
            firecrawl_endpoint: DEFAULT_FIRECRAWL_ENDPOINT.into(),
            firecrawl_api_key: Some("firecrawl-token".into()),
            extract_backend: ExtractBackend::Direct,
            timeout,
            result_count: 3,
            http_client: build_http_client(timeout).unwrap(),
        }
    }

    #[test]
    fn versioned_subject_prefix_subscription_keeps_methods_parseable() {
        let prefix = "tool.search.v047";

        assert_eq!(format!("{prefix}.>"), "tool.search.v047.>");
        assert_eq!(
            method_from_subject("tool.search.v047.web-search"),
            Some("web-search")
        );
        assert_eq!(method_from_subject("tool.search.v047.ping"), Some("ping"));
    }

    #[test]
    fn percent_encoding_matches_query_string_needs() {
        assert_eq!(
            percent_encode("bevy ecs 0.16 system+query"),
            "bevy+ecs+0.16+system%2Bquery"
        );
    }

    #[test]
    fn routed_query_appends_unique_domain_filters() {
        let input = WebSearchInput {
            query: "bevy resources".into(),
            intent: None,
            time_range: None,
            domains: Some(vec![
                "docs.rs".into(),
                "docs.rs".into(),
                "bevyengine.org".into(),
            ]),
        };

        assert_eq!(
            routed_query(&input),
            "bevy resources site:docs.rs site:bevyengine.org"
        );
    }

    #[test]
    fn search_api_key_reads_jam_secrets_file() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        writeln!(
            tmp,
            r#"[secrets]
"jam/search/brave" = "brave-file-key"
"#
        )
        .unwrap();

        let key = search_api_key(&[], BRAVE_FILE_SECRET_KEYS, &[], Some(tmp.path())).unwrap();

        assert_eq!(key.as_deref(), Some("brave-file-key"));
    }

    #[test]
    fn search_api_key_rejects_invalid_jam_secrets_file() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        writeln!(tmp, "[secrets").unwrap();

        let err = search_api_key(&[], BRAVE_FILE_SECRET_KEYS, &[], Some(tmp.path())).unwrap_err();

        assert!(err.to_string().contains("invalid-search-secrets-file"));
    }

    #[test]
    fn auto_router_selects_privacy_and_source_backends_when_configured() {
        let config = test_config();
        let privacy = WebSearchInput {
            query: "private lookup".into(),
            intent: Some("privacy-sensitive".into()),
            time_range: None,
            domains: None,
        };
        let sourced = WebSearchInput {
            query: "answer with citations".into(),
            intent: Some("source-backed answer".into()),
            time_range: None,
            domains: None,
        };
        let factual = WebSearchInput {
            query: "fast lookup".into(),
            intent: None,
            time_range: None,
            domains: None,
        };

        assert_eq!(
            select_web_search_backend(&config, &privacy).unwrap(),
            WebSearchBackend::Searxng
        );
        assert_eq!(
            select_web_search_backend(&config, &sourced).unwrap(),
            WebSearchBackend::Linkup
        );
        assert_eq!(
            select_web_search_backend(&config, &factual).unwrap(),
            WebSearchBackend::Brave
        );
    }

    #[test]
    fn brave_url_includes_freshness_and_count() {
        let config = test_config();
        let input = WebSearchInput {
            query: "rust traits".into(),
            intent: Some("docs".into()),
            time_range: Some("week".into()),
            domains: None,
        };

        assert_eq!(
            brave_url(&config, &input),
            "https://example.test/search?q=rust+traits&count=3&freshness=pw"
        );
    }

    #[test]
    fn searxng_url_requests_json_results() {
        let config = test_config();
        let input = WebSearchInput {
            query: "rust traits".into(),
            intent: Some("privacy-sensitive".into()),
            time_range: Some("month".into()),
            domains: Some(vec!["docs.rs".into()]),
        };

        let url = searxng_url(&config, &input).unwrap();
        let rendered = url.as_str();

        assert!(rendered.starts_with("https://searx.example/search?"));
        assert!(rendered.contains("format=json"));
        assert!(rendered.contains("pageno=1"));
        assert!(rendered.contains("time_range=month"));
        assert!(rendered.contains("q=rust+traits+site%3Adocs.rs"));
    }

    #[test]
    fn parses_brave_results() {
        let raw = serde_json::json!({
            "web": {
                "results": [
                    {
                        "title": "Bevy ECS",
                        "url": "https://docs.rs/bevy_ecs",
                        "description": "Entity component system docs"
                    }
                ]
            }
        });

        let parsed = parse_brave_results(&raw).unwrap();

        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].title, "Bevy ECS");
        assert_eq!(parsed[0].snippet, "Entity component system docs");
    }

    #[test]
    fn parses_searxng_results() {
        let raw = serde_json::json!({
            "results": [
                {
                    "title": "SearXNG Docs",
                    "url": "https://docs.searxng.org/dev/search_api.html",
                    "content": "SearXNG supports querying via a simple HTTP API."
                }
            ]
        });

        let parsed = parse_searxng_results(&raw).unwrap();

        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].title, "SearXNG Docs");
        assert_eq!(
            parsed[0].snippet,
            "SearXNG supports querying via a simple HTTP API."
        );
    }

    #[test]
    fn linkup_body_and_parser_use_search_results_shape() {
        let config = test_config();
        let input = WebSearchInput {
            query: "Microsoft 2024 revenue".into(),
            intent: Some("source-backed answer".into()),
            time_range: None,
            domains: Some(vec!["microsoft.com".into(), "microsoft.com".into()]),
        };

        let body = linkup_search_body(&config, &input);

        assert_eq!(body["q"], "Microsoft 2024 revenue");
        assert_eq!(body["depth"], DEFAULT_LINKUP_DEPTH);
        assert_eq!(body["outputType"], "searchResults");
        assert_eq!(body["maxResults"], 3);
        assert_eq!(body["includeDomains"], serde_json::json!(["microsoft.com"]));

        let raw = serde_json::json!({
            "results": [
                {
                    "type": "text",
                    "name": "Microsoft 2024 Annual Report",
                    "url": "https://www.microsoft.com/investor/reports/ar24/index.html",
                    "content": "Microsoft Cloud revenue increased."
                }
            ]
        });
        let parsed = parse_linkup_results(&raw).unwrap();

        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].title, "Microsoft 2024 Annual Report");
        assert_eq!(parsed[0].snippet, "Microsoft Cloud revenue increased.");
    }

    #[test]
    fn cooldown_blocks_until_expiry() {
        let state = SearchState {
            config: test_config(),
            cooldown: Arc::default(),
        };
        let now = DateTime::parse_from_rfc3339("2026-05-06T09:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        state.set_brave_cooldown(now, "boom".into());

        assert!(state
            .brave_cooldown(now + chrono::Duration::minutes(59))
            .is_some());
        assert!(state
            .brave_cooldown(now + chrono::Duration::hours(1))
            .is_none());
    }

    #[test]
    fn validates_extract_urls_and_blocks_local_targets() {
        let input = WebExtractInput {
            urls: vec!["https://example.org/docs".into()],
            render_js: None,
            include_images: None,
        };

        let urls = validate_extract_input(&input, false).unwrap();

        assert_eq!(urls[0].as_str(), "https://example.org/docs");
        assert!(validate_public_http_url("http://127.0.0.1:8080", "api-web-extract").is_err());
        assert!(validate_public_http_url("file:///etc/passwd", "api-web-extract").is_err());
    }

    #[test]
    fn crawl_validation_bounds_depth_pages_and_rendering() {
        let too_deep = WebCrawlInput {
            root_url: "https://example.org".into(),
            max_depth: MAX_CRAWL_DEPTH + 1,
            max_pages: Some(1),
            render_js: None,
            include_images: None,
        };
        let render_js = WebCrawlInput {
            root_url: "https://example.org".into(),
            max_depth: 1,
            max_pages: Some(1),
            render_js: Some(true),
            include_images: None,
        };

        assert!(validate_crawl_input(&too_deep, false).is_err());
        assert!(validate_crawl_input(&render_js, false).is_err());
        assert!(validate_crawl_input(&render_js, true).is_ok());
    }

    #[test]
    fn extracts_html_text_images_and_safe_links() {
        let url = Url::parse("https://example.org/docs/index.html").unwrap();
        let page = extract_document(
            &url,
            r#"
            <!doctype html>
            <html>
              <head><title>Example Docs</title></head>
              <body>
                <h1>Hello</h1>
                <p>Jamboree search extraction.</p>
                <img src="/logo.png">
                <a href="/next">Next</a>
                <a href="http://127.0.0.1/private">Private</a>
              </body>
            </html>
            "#,
            "text/html",
            true,
        );

        assert_eq!(page.content.title.as_deref(), Some("Example Docs"));
        assert!(page
            .content
            .text
            .contains("Hello Jamboree search extraction."));
        assert_eq!(page.content.images, vec!["https://example.org/logo.png"]);
        assert_eq!(page.links.len(), 1);
        assert_eq!(page.links[0].as_str(), "https://example.org/next");
    }

    #[test]
    fn firecrawl_scrape_body_requests_markdown_links_and_images() {
        let url = Url::parse("https://example.org/docs").unwrap();
        let body = firecrawl_scrape_body(&url, true, Duration::from_secs(12));

        assert_eq!(body["url"], "https://example.org/docs");
        assert_eq!(
            body["formats"],
            serde_json::json!(["markdown", "links", "images"])
        );
        assert_eq!(body["onlyMainContent"], true);
        assert_eq!(body["timeout"], 12_000);
    }

    #[test]
    fn firecrawl_crawl_body_keeps_scope_bounded() {
        let root_url = Url::parse("https://example.org").unwrap();
        let input = WebCrawlInput {
            root_url: root_url.to_string(),
            max_depth: 2,
            max_pages: Some(7),
            render_js: Some(true),
            include_images: Some(false),
        };

        let body = firecrawl_crawl_body(&input, &root_url, false, 7, Duration::from_secs(5));

        assert_eq!(body["url"], "https://example.org/");
        assert_eq!(body["limit"], 7);
        assert_eq!(body["maxDiscoveryDepth"], 2);
        assert_eq!(body["allowExternalLinks"], false);
        assert_eq!(body["allowSubdomains"], false);
        assert_eq!(
            body["scrapeOptions"]["formats"],
            serde_json::json!(["markdown", "links"])
        );
    }

    #[test]
    fn parses_firecrawl_page_content_links_and_images() {
        let requested_url = Url::parse("https://example.org/docs").unwrap();
        let raw = serde_json::json!({
            "markdown": "# Title\n\nBody",
            "metadata": {
                "title": "Title",
                "sourceURL": "https://example.org/docs"
            },
            "links": [
                "https://example.org/next",
                "http://127.0.0.1/private"
            ],
            "images": [
                "/img.png",
                "http://127.0.0.1/image.png"
            ]
        });

        let page = parse_firecrawl_page(&requested_url, &raw, true, "api-web-extract").unwrap();

        assert_eq!(page.content.url, "https://example.org/docs");
        assert_eq!(page.content.title.as_deref(), Some("Title"));
        assert_eq!(page.content.text, "# Title\n\nBody");
        assert_eq!(page.content.images, vec!["https://example.org/img.png"]);
        assert_eq!(page.links.len(), 1);
        assert_eq!(page.links[0].as_str(), "https://example.org/next");
    }

    #[test]
    fn firecrawl_crawl_poll_url_prefers_job_id() {
        let config = test_config();
        let raw = serde_json::json!({
            "success": true,
            "id": "crawl-123",
            "url": "https://other.example/crawl/crawl-123"
        });

        let url = firecrawl_crawl_poll_url(&config, &raw, "api-web-crawl").unwrap();

        assert_eq!(url, "https://api.firecrawl.dev/v2/crawl/crawl-123");
    }

    #[test]
    fn firecrawl_crawl_poll_url_preserves_endpoint_path_for_relative_urls() {
        let config = test_config();
        let raw = serde_json::json!({
            "success": true,
            "url": "crawl/crawl-123"
        });

        let url = firecrawl_crawl_poll_url(&config, &raw, "api-web-crawl").unwrap();

        assert_eq!(url, "https://api.firecrawl.dev/v2/crawl/crawl-123");
    }

    #[test]
    fn firecrawl_crawl_poll_url_rejects_cross_origin_url() {
        let config = test_config();
        let raw = serde_json::json!({
            "success": true,
            "url": "https://other.example/crawl/crawl-123"
        });

        assert!(firecrawl_crawl_poll_url(&config, &raw, "api-web-crawl").is_err());
    }
}
