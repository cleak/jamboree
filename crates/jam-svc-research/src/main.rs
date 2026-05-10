//! `jam-svc-research` - tiered deep research request service.
//!
//! This first slice establishes the traced NATS boundary and the uniform
//! `~/.jam/research/<id>/` output shape from §4.10. Explicit fake mode remains
//! available for smoke coverage; real provider calls require seeded
//! provider-specific credentials.

#![deny(missing_docs)]

use std::fs::{self, File};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use chrono::Utc;
use futures::StreamExt;
use jam_nats::async_nats;
use jam_nats::JamNats;
use jam_secrets::{ExposeSecret, FileBackend, PassBackend, SecretBackend, SecretError, SecretKey};
use jam_trace::TraceCtx;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::{debug, error, info, warn};

const SERVICE_NAME: &str = "jam-svc-research";
const SERVICE_VERSION: &str = env!("CARGO_PKG_VERSION");
const SUBJECT_PREFIX: &str = "tool.research";
const SUBJECT_PREFIX_ENV: &str = "JAM_RESEARCH_SUBJECT_PREFIX";
const MAX_QUESTION_LEN: usize = 20_000;
const MAX_SCOPE_LEN: usize = 500;
const DEFAULT_HTTP_TIMEOUT_SECS: u64 = 120;
const DEFAULT_POLL_INTERVAL_MILLIS: u64 = 15_000;
const DEFAULT_MAX_POLLS: u32 = 120;
const DEFAULT_TAVILY_BASE_URL: &str = "https://api.tavily.com";
const DEFAULT_PERPLEXITY_BASE_URL: &str = "https://api.perplexity.ai";
const DEFAULT_EXA_BASE_URL: &str = "https://api.exa.ai";
const DEFAULT_PARALLEL_BASE_URL: &str = "https://api.parallel.ai";

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
enum ResearchError {
    #[error("{kind}: {detail}")]
    Protocol {
        kind: &'static str,
        detail: String,
        remediation: String,
        tracked_by: &'static str,
    },
}

impl ResearchError {
    fn protocol(
        kind: &'static str,
        detail: impl Into<String>,
        remediation: impl Into<String>,
        tracked_by: &'static str,
    ) -> Self {
        Self::Protocol {
            kind,
            detail: detail.into(),
            remediation: remediation.into(),
            tracked_by,
        }
    }
}

#[derive(Debug, Clone)]
struct ResearchConfig {
    research_root: PathBuf,
    fake_provider: bool,
    tempyr_graph_dir: Option<PathBuf>,
    http_timeout: Duration,
    poll_interval: Duration,
    max_polls: u32,
    secrets_file: Option<PathBuf>,
    tavily_base_url: String,
    perplexity_base_url: String,
    exa_base_url: String,
    parallel_base_url: String,
}

impl ResearchConfig {
    fn from_env() -> Result<Self, ResearchError> {
        let jam_home = env_path("JAM_HOME")
            .or_else(default_jam_home)
            .ok_or_else(|| {
                ResearchError::protocol(
                    "missing-jam-home",
                    "JAM_HOME is unset and HOME is unavailable",
                    "Set JAM_HOME=/home/maestro/.jam for runtime services.",
                    "principle-failure-surfaces-immediately",
                )
            })?;
        let research_root =
            env_path("JAM_RESEARCH_ROOT").unwrap_or_else(|| jam_home.join("research"));
        let tempyr_graph_dir = env_path("JAM_RESEARCH_TEMPYR_GRAPH_DIR");
        if let Some(graph_dir) = &tempyr_graph_dir {
            validate_existing_graph_dir(graph_dir)?;
        }
        Ok(Self {
            research_root,
            fake_provider: env_bool("JAM_RESEARCH_FAKE_PROVIDER", false),
            tempyr_graph_dir,
            http_timeout: env_duration_secs(
                "JAM_RESEARCH_HTTP_TIMEOUT_SECS",
                DEFAULT_HTTP_TIMEOUT_SECS,
            )?,
            poll_interval: env_duration_millis(
                "JAM_RESEARCH_POLL_INTERVAL_MS",
                DEFAULT_POLL_INTERVAL_MILLIS,
            )?,
            max_polls: env_u32("JAM_RESEARCH_MAX_POLLS", DEFAULT_MAX_POLLS)?,
            secrets_file: env_path("JAM_SECRETS_FILE"),
            tavily_base_url: env_string("JAM_TAVILY_BASE_URL", DEFAULT_TAVILY_BASE_URL),
            perplexity_base_url: env_string("JAM_PERPLEXITY_BASE_URL", DEFAULT_PERPLEXITY_BASE_URL),
            exa_base_url: env_string("JAM_EXA_BASE_URL", DEFAULT_EXA_BASE_URL),
            parallel_base_url: env_string("JAM_PARALLEL_BASE_URL", DEFAULT_PARALLEL_BASE_URL),
        })
    }
}

#[derive(Debug, Clone)]
struct SelectedProvider {
    name: &'static str,
    kind: ProviderKind,
    api_key: String,
}

#[derive(Debug, Clone, Copy)]
enum ProviderKind {
    Tavily,
    Sonar { model: &'static str },
    ExaDeepReasoning,
    Parallel { processor: &'static str },
}

#[derive(Debug, Clone, Copy)]
struct ProviderCandidate {
    name: &'static str,
    kind: ProviderKind,
    env_keys: &'static [&'static str],
    file_secret_keys: &'static [&'static str],
    pass_secret_keys: &'static [&'static str],
}

const TAVILY_KEYS: &[&str] = &["JAM_TAVILY_API_KEY", "TAVILY_API_KEY"];
const PERPLEXITY_KEYS: &[&str] = &["JAM_PERPLEXITY_API_KEY", "PERPLEXITY_API_KEY"];
const EXA_KEYS: &[&str] = &["JAM_EXA_API_KEY", "EXA_API_KEY"];
const PARALLEL_KEYS: &[&str] = &["JAM_PARALLEL_API_KEY", "PARALLEL_API_KEY"];
const TAVILY_FILE_SECRET_KEYS: &[&str] = &["jam/search/tavily", "jam/research/tavily-api-key"];
const TAVILY_PASS_SECRET_KEYS: &[&str] = &["search/tavily", "research/tavily-api-key"];
const PERPLEXITY_FILE_SECRET_KEYS: &[&str] =
    &["jam/search/perplexity", "jam/research/perplexity-api-key"];
const PERPLEXITY_PASS_SECRET_KEYS: &[&str] = &["search/perplexity", "research/perplexity-api-key"];
const EXA_FILE_SECRET_KEYS: &[&str] = &["jam/search/exa", "jam/research/exa-api-key"];
const EXA_PASS_SECRET_KEYS: &[&str] = &["search/exa", "research/exa-api-key"];
const PARALLEL_FILE_SECRET_KEYS: &[&str] = &["jam/research/parallel-api-key"];
const PARALLEL_PASS_SECRET_KEYS: &[&str] = &["research/parallel-api-key"];

const QUICK_CANDIDATES: &[ProviderCandidate] = &[
    ProviderCandidate {
        name: "tavily",
        kind: ProviderKind::Tavily,
        env_keys: TAVILY_KEYS,
        file_secret_keys: TAVILY_FILE_SECRET_KEYS,
        pass_secret_keys: TAVILY_PASS_SECRET_KEYS,
    },
    ProviderCandidate {
        name: "sonar",
        kind: ProviderKind::Sonar { model: "sonar" },
        env_keys: PERPLEXITY_KEYS,
        file_secret_keys: PERPLEXITY_FILE_SECRET_KEYS,
        pass_secret_keys: PERPLEXITY_PASS_SECRET_KEYS,
    },
];

const STANDARD_CANDIDATES: &[ProviderCandidate] = &[ProviderCandidate {
    name: "sonar-pro",
    kind: ProviderKind::Sonar { model: "sonar-pro" },
    env_keys: PERPLEXITY_KEYS,
    file_secret_keys: PERPLEXITY_FILE_SECRET_KEYS,
    pass_secret_keys: PERPLEXITY_PASS_SECRET_KEYS,
}];

const DEEP_CANDIDATES: &[ProviderCandidate] = &[
    ProviderCandidate {
        name: "exa-deep-reasoning",
        kind: ProviderKind::ExaDeepReasoning,
        env_keys: EXA_KEYS,
        file_secret_keys: EXA_FILE_SECRET_KEYS,
        pass_secret_keys: EXA_PASS_SECRET_KEYS,
    },
    ProviderCandidate {
        name: "parallel-pro",
        kind: ProviderKind::Parallel { processor: "pro" },
        env_keys: PARALLEL_KEYS,
        file_secret_keys: PARALLEL_FILE_SECRET_KEYS,
        pass_secret_keys: PARALLEL_PASS_SECRET_KEYS,
    },
    ProviderCandidate {
        name: "sonar-reasoning-pro",
        kind: ProviderKind::Sonar {
            model: "sonar-reasoning-pro",
        },
        env_keys: PERPLEXITY_KEYS,
        file_secret_keys: PERPLEXITY_FILE_SECRET_KEYS,
        pass_secret_keys: PERPLEXITY_PASS_SECRET_KEYS,
    },
];

#[derive(Debug, Deserialize)]
struct RequestResearchInput {
    question: String,
    tier: String,
    scope: Option<String>,
    deadline: Option<String>,
}

#[derive(Debug, Serialize)]
struct ResearchHandle {
    research_id: String,
    tier: ResearchTier,
    provider: String,
    status: String,
    output_dir: String,
    trace_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum ResearchTier {
    Quick,
    Standard,
    Deep,
}

impl ResearchTier {
    fn parse(raw: &str) -> Result<Self, ResearchError> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "quick" => Ok(Self::Quick),
            "standard" => Ok(Self::Standard),
            "deep" => Ok(Self::Deep),
            _ => Err(ResearchError::protocol(
                "invalid-tier",
                format!("tier {raw:?} is not one of quick, standard, deep"),
                "Use the tier names from api-request-research.",
                "api-request-research",
            )),
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::Quick => "quick",
            Self::Standard => "standard",
            Self::Deep => "deep",
        }
    }
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

#[derive(Debug, Serialize)]
struct ResearchJournalEvent<'a> {
    research_id: &'a str,
    question: &'a str,
    tier: &'a str,
    provider: &'a str,
    status: &'a str,
    output_dir: &'a str,
}

#[derive(Debug, Serialize)]
struct ResearchTempyrNodeCreatedEvent<'a> {
    research_id: &'a str,
    node_id: &'a str,
    output_dir: &'a str,
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    if let Err(err) = run().await {
        error!("jam-svc-research fatal: {err}");
        return std::process::ExitCode::FAILURE;
    }
    std::process::ExitCode::SUCCESS
}

async fn run() -> Result<(), ServiceError> {
    init_tracing();

    let nats_url = std::env::var("NATS_URL").unwrap_or_else(|_| "nats://127.0.0.1:4222".into());
    let nats_token = std::env::var("NATS_TOKEN").ok();
    let subject_prefix = configured_subject_prefix(SUBJECT_PREFIX_ENV, SUBJECT_PREFIX);
    let config = ResearchConfig::from_env().map_err(|err| ServiceError::Reply(err.to_string()))?;

    info!(
        service = %SERVICE_NAME,
        version = %SERVICE_VERSION,
        nats = %nats_url,
        subject_prefix = %subject_prefix,
        research_root = %config.research_root.display(),
        fake_provider = config.fake_provider,
        "starting",
    );

    let nats = JamNats::connect(&nats_url, nats_token).await?;
    info!("connected to NATS");

    let mut sub = nats
        .client()
        .subscribe(format!("{subject_prefix}.>"))
        .await
        .map_err(|err| ServiceError::Subscribe(err.to_string()))?;
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
                let config = config.clone();
                let subject_prefix = subject_prefix.clone();
                let draining = draining.clone();
                let active_requests = active_requests.clone();
                active_requests.fetch_add(1, Ordering::SeqCst);
                tokio::spawn(async move {
                    let result =
                        handle_request(&nats, &message, &subject_prefix, &config, &draining).await;
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
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("jam_svc_research=info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

async fn handle_request(
    nats: &JamNats,
    msg: &async_nats::Message,
    subject_prefix: &str,
    config: &ResearchConfig,
    draining: &Arc<AtomicBool>,
) -> Result<(), ServiceError> {
    let method = method_from_subject(msg.subject.as_str(), subject_prefix).unwrap_or("");
    debug!(subject = %msg.subject, method = %method, "received request");

    let response_ctx = msg
        .headers
        .as_ref()
        .and_then(jam_nats::extract_trace_from_headers);

    let response = match &response_ctx {
        Some(ctx) => dispatch(method, &msg.payload, ctx, nats, config).await,
        None => Response::Error {
            error: ResponseError {
                kind: "missing-trace".into(),
                detail: "tool.research requests must include Trace-Id headers".into(),
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
    ctx: &TraceCtx,
    nats: &JamNats,
    config: &ResearchConfig,
) -> Response {
    match method {
        "ping" => Response::Ok(json!({
            "status": "ok",
            "service": SERVICE_NAME,
            "version": SERVICE_VERSION,
        })),
        "drain" => Response::Ok(json!({
            "status": "draining",
            "service": SERVICE_NAME,
            "version": SERVICE_VERSION,
        })),
        "request-research" => match request_research(payload, ctx, nats, config).await {
            Ok(handle) => Response::Ok(serde_json::to_value(handle).expect("handle serializes")),
            Err(err) => error_response(err),
        },
        unknown => Response::Error {
            error: ResponseError {
                kind: "unknown-method".into(),
                detail: format!("{SUBJECT_PREFIX}.{unknown} is not a recognized research method"),
                remediation: "Use tool.research.request-research.".into(),
                tracked_by: "api-request-research",
            },
        },
    }
}

async fn request_research(
    payload: &[u8],
    ctx: &TraceCtx,
    nats: &JamNats,
    config: &ResearchConfig,
) -> Result<ResearchHandle, ResearchError> {
    let input = parse_request(payload)?;
    let question = validate_question(&input.question)?;
    let tier = ResearchTier::parse(&input.tier)?;
    let scope = validate_optional_text(input.scope.as_deref(), "scope", MAX_SCOPE_LEN)?;
    validate_optional_text(input.deadline.as_deref(), "deadline", 100)?;
    let provider = select_provider(tier, config)?;
    let provider_name = provider.name.to_owned();
    let research_id = research_id(scope.as_deref(), ctx);
    let output_dir = config.research_root.join(&research_id);

    publish_research_journal(
        nats,
        "journal.research.requested",
        &ResearchJournalFields {
            research_id: &research_id,
            question: &question,
            tier,
            provider: &provider_name,
            status: "requested",
            output_dir: &output_dir,
        },
        ctx,
    )
    .await?;

    if config.fake_provider {
        write_fake_research_output(
            &output_dir,
            &research_id,
            &question,
            tier,
            &provider_name,
            ctx,
        )?;
        maybe_create_tempyr_node(nats, config, &output_dir, &research_id, ctx).await?;
        publish_research_journal(
            nats,
            "journal.research.completed",
            &ResearchJournalFields {
                research_id: &research_id,
                question: &question,
                tier,
                provider: &provider_name,
                status: "completed",
                output_dir: &output_dir,
            },
            ctx,
        )
        .await?;
        return Ok(ResearchHandle {
            research_id,
            tier,
            provider: provider_name,
            status: "completed".into(),
            output_dir: output_dir.display().to_string(),
            trace_id: ctx.trace_id.to_string(),
        });
    }

    let output = run_provider_research(&provider, &question, scope.as_deref(), config).await?;
    write_provider_research_output(
        &output_dir,
        &research_id,
        &question,
        tier,
        provider.name,
        &output,
        ctx,
    )?;
    maybe_create_tempyr_node(nats, config, &output_dir, &research_id, ctx).await?;
    publish_research_journal(
        nats,
        "journal.research.completed",
        &ResearchJournalFields {
            research_id: &research_id,
            question: &question,
            tier,
            provider: provider.name,
            status: "completed",
            output_dir: &output_dir,
        },
        ctx,
    )
    .await?;
    Ok(ResearchHandle {
        research_id,
        tier,
        provider: provider.name.to_owned(),
        status: "completed".into(),
        output_dir: output_dir.display().to_string(),
        trace_id: ctx.trace_id.to_string(),
    })
}

fn parse_request(payload: &[u8]) -> Result<RequestResearchInput, ResearchError> {
    serde_json::from_slice(payload).map_err(|err| {
        ResearchError::protocol(
            "invalid-input",
            format!("tool.research.request-research payload is invalid JSON: {err}"),
            "Send {\"question\":\"...\",\"tier\":\"deep\"}.",
            "api-request-research",
        )
    })
}

fn validate_question(raw: &str) -> Result<String, ResearchError> {
    let question = raw.trim();
    if question.is_empty() {
        return Err(ResearchError::protocol(
            "invalid-question",
            "question must not be empty",
            "Send a concrete research question.",
            "api-request-research",
        ));
    }
    if question.len() > MAX_QUESTION_LEN || question.contains('\0') {
        return Err(ResearchError::protocol(
            "invalid-question",
            "question is too long or contains NUL",
            "Keep research questions below 20KB and remove control characters.",
            "api-request-research",
        ));
    }
    Ok(question.to_owned())
}

fn validate_optional_text(
    raw: Option<&str>,
    field: &'static str,
    max_len: usize,
) -> Result<Option<String>, ResearchError> {
    let Some(value) = raw else {
        return Ok(None);
    };
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    if value.len() > max_len || value.contains('\0') {
        return Err(ResearchError::protocol(
            "invalid-field",
            format!("{field} is too long or contains NUL"),
            format!("Keep {field} under {max_len} bytes."),
            "api-request-research",
        ));
    }
    Ok(Some(value.to_owned()))
}

fn select_provider(
    tier: ResearchTier,
    config: &ResearchConfig,
) -> Result<SelectedProvider, ResearchError> {
    if config.fake_provider {
        return Ok(SelectedProvider {
            name: match tier {
                ResearchTier::Quick => "fake-quick",
                ResearchTier::Standard => "fake-standard",
                ResearchTier::Deep => "fake-deep",
            },
            kind: ProviderKind::Tavily,
            api_key: String::new(),
        });
    }
    let candidates = match tier {
        ResearchTier::Quick => QUICK_CANDIDATES,
        ResearchTier::Standard => STANDARD_CANDIDATES,
        ResearchTier::Deep => DEEP_CANDIDATES,
    };
    for candidate in candidates {
        if let Some(api_key) = provider_api_key(candidate, config)? {
            return Ok(SelectedProvider {
                name: candidate.name,
                kind: candidate.kind,
                api_key,
            });
        }
    }
    Err(ResearchError::protocol(
        "missing-research-provider-credential",
        format!(
            "no provider credential is configured for {} research",
            tier.as_str()
        ),
        "Seed Tavily, Exa, Parallel, or Perplexity credentials in env, JAM_SECRETS_FILE, or maestro pass.",
        "task-jam-svc-research",
    ))
}

fn provider_api_key(
    candidate: &ProviderCandidate,
    config: &ResearchConfig,
) -> Result<Option<String>, ResearchError> {
    if let Some(api_key) = env_value(candidate.env_keys) {
        return Ok(Some(api_key));
    }
    if let Some(path) = config.secrets_file.as_deref() {
        if let Some(api_key) = first_file_secret(path, candidate.file_secret_keys)? {
            return Ok(Some(api_key));
        }
    }
    Ok(first_pass_secret(candidate.pass_secret_keys))
}

fn first_file_secret(path: &Path, keys: &[&str]) -> Result<Option<String>, ResearchError> {
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
) -> Result<Option<String>, ResearchError> {
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
                    return Err(ResearchError::protocol(
                        "invalid-research-secrets-file",
                        format!("failed reading research credential {key}: {err}"),
                        "Fix JAM_SECRETS_FILE or unset it and use env/pass credentials.",
                        "principle-failure-surfaces-immediately",
                    ));
                }
            }
            Err(SecretError::Io(err)) => {
                if fail_on_backend_error {
                    return Err(ResearchError::protocol(
                        "invalid-research-secrets-file",
                        format!("failed reading research credential {key}: {err}"),
                        "Fix JAM_SECRETS_FILE or unset it and use env/pass credentials.",
                        "principle-failure-surfaces-immediately",
                    ));
                }
            }
        }
    }
    Ok(None)
}

#[derive(Debug)]
struct ProviderResearchOutput {
    provider_request_id: Option<String>,
    report_markdown: String,
    findings: Value,
    sources: Vec<Value>,
    transcript: Vec<Value>,
    raw_response: Value,
}

async fn run_provider_research(
    provider: &SelectedProvider,
    question: &str,
    scope: Option<&str>,
    config: &ResearchConfig,
) -> Result<ProviderResearchOutput, ResearchError> {
    let client = reqwest::Client::builder()
        .timeout(config.http_timeout)
        .build()
        .map_err(|err| {
            ResearchError::protocol(
                "research-http-client-build-failed",
                err.to_string(),
                "Verify the local TLS and DNS configuration for provider calls.",
                "principle-failure-surfaces-immediately",
            )
        })?;
    let prompt = research_prompt(question, scope);
    match provider.kind {
        ProviderKind::Tavily => run_tavily_research(&client, provider, &prompt, config).await,
        ProviderKind::Sonar { model } => {
            run_sonar_research(&client, provider, model, &prompt, config).await
        }
        ProviderKind::ExaDeepReasoning => {
            run_exa_deep_reasoning_research(&client, provider, &prompt, config).await
        }
        ProviderKind::Parallel { processor } => {
            run_parallel_research(&client, provider, processor, &prompt, config).await
        }
    }
}

async fn run_tavily_research(
    client: &reqwest::Client,
    provider: &SelectedProvider,
    prompt: &str,
    config: &ResearchConfig,
) -> Result<ProviderResearchOutput, ResearchError> {
    let create_url = endpoint(&config.tavily_base_url, "/research");
    let create = post_json(
        client,
        provider.name,
        &create_url,
        auth_headers(AuthStyle::Bearer, &provider.api_key)?,
        &json!({
            "input": prompt,
            "model": "mini",
            "stream": false,
            "citation_format": "numbered",
        }),
    )
    .await?;
    let request_id = required_string(&create, "request_id", provider.name)?;
    let mut transcript = vec![json!({
        "event": "provider-request-created",
        "provider": provider.name,
        "provider_request_id": request_id,
        "raw": create,
    })];

    for attempt in 0..config.max_polls {
        if attempt > 0 {
            tokio::time::sleep(config.poll_interval).await;
        }
        let status_url = endpoint(&config.tavily_base_url, &format!("/research/{request_id}"));
        let status = get_json(
            client,
            provider.name,
            &status_url,
            auth_headers(AuthStyle::Bearer, &provider.api_key)?,
        )
        .await?;
        transcript.push(json!({
            "event": "provider-poll",
            "provider": provider.name,
            "attempt": attempt + 1,
            "raw": status,
        }));
        match status_field(&status).as_deref() {
            Some("completed") => {
                return Ok(tavily_output(request_id, status, transcript));
            }
            Some("failed") => {
                return Err(provider_failed(provider.name, &status));
            }
            Some("pending" | "running" | "queued") | None => {}
            Some(other) => {
                warn!(
                    provider = provider.name,
                    status = other,
                    "unknown provider status"
                );
            }
        }
    }

    Err(provider_timeout(provider.name, config.max_polls))
}

async fn run_sonar_research(
    client: &reqwest::Client,
    provider: &SelectedProvider,
    model: &str,
    prompt: &str,
    config: &ResearchConfig,
) -> Result<ProviderResearchOutput, ResearchError> {
    let url = endpoint(&config.perplexity_base_url, "/v1/sonar");
    let response = post_json(
        client,
        provider.name,
        &url,
        auth_headers(AuthStyle::Bearer, &provider.api_key)?,
        &json!({
            "model": model,
            "messages": [
                {
                    "role": "system",
                    "content": "You are Jamboree's research adapter. Return a grounded Markdown research report with citations and concise findings.",
                },
                {
                    "role": "user",
                    "content": prompt,
                },
            ],
            "temperature": 0.2,
        }),
    )
    .await?;
    sonar_output(provider.name, response)
}

async fn run_exa_deep_reasoning_research(
    client: &reqwest::Client,
    provider: &SelectedProvider,
    prompt: &str,
    config: &ResearchConfig,
) -> Result<ProviderResearchOutput, ResearchError> {
    let url = endpoint(&config.exa_base_url, "/search");
    let response = post_json(
        client,
        provider.name,
        &url,
        auth_headers(AuthStyle::ApiKey, &provider.api_key)?,
        &json!({
            "query": prompt,
            "type": "deep-reasoning",
            "numResults": 10,
            "systemPrompt": "Synthesize a concise Markdown research report. Prefer primary sources and include a short findings list.",
            "contents": {
                "highlights": true,
            },
            "outputSchema": {
                "type": "object",
                "properties": {
                    "report": {
                        "type": "string",
                        "description": "Markdown report with citations.",
                    },
                    "findings": {
                        "type": "array",
                        "items": { "type": "string" },
                    },
                },
                "required": ["report"],
                "additionalProperties": true,
            },
        }),
    )
    .await?;
    exa_output(provider.name, response)
}

async fn run_parallel_research(
    client: &reqwest::Client,
    provider: &SelectedProvider,
    processor: &str,
    prompt: &str,
    config: &ResearchConfig,
) -> Result<ProviderResearchOutput, ResearchError> {
    let create_url = endpoint(&config.parallel_base_url, "/v1/tasks/runs");
    let create = post_json(
        client,
        provider.name,
        &create_url,
        auth_headers(AuthStyle::ApiKey, &provider.api_key)?,
        &json!({
            "input": prompt,
            "processor": processor,
            "task_spec": {
                "output_schema": {
                    "type": "text",
                    "description": "Markdown research report with inline citations.",
                },
            },
        }),
    )
    .await?;
    let run_id = required_string(&create, "run_id", provider.name)?;
    let mut transcript = vec![json!({
        "event": "provider-request-created",
        "provider": provider.name,
        "provider_request_id": run_id,
        "raw": create,
    })];

    for attempt in 0..config.max_polls {
        if attempt > 0 {
            tokio::time::sleep(config.poll_interval).await;
        }
        let status_url = endpoint(
            &config.parallel_base_url,
            &format!("/v1/tasks/runs/{run_id}"),
        );
        let status = get_json(
            client,
            provider.name,
            &status_url,
            auth_headers(AuthStyle::ApiKey, &provider.api_key)?,
        )
        .await?;
        transcript.push(json!({
            "event": "provider-poll",
            "provider": provider.name,
            "attempt": attempt + 1,
            "raw": status,
        }));
        match status_field(&status).as_deref() {
            Some("completed") => {
                let result_url = endpoint(
                    &config.parallel_base_url,
                    &format!("/v1/tasks/runs/{run_id}/result"),
                );
                let result = get_json(
                    client,
                    provider.name,
                    &format!("{result_url}?timeout=1"),
                    auth_headers(AuthStyle::ApiKey, &provider.api_key)?,
                )
                .await?;
                transcript.push(json!({
                    "event": "provider-result",
                    "provider": provider.name,
                    "raw": result,
                }));
                return Ok(parallel_output(run_id, result, transcript));
            }
            Some("failed" | "cancelled" | "cancelling") => {
                return Err(provider_failed(provider.name, &status));
            }
            Some("queued" | "running" | "action_required") | None => {}
            Some(other) => {
                warn!(
                    provider = provider.name,
                    status = other,
                    "unknown provider status"
                );
            }
        }
    }

    Err(provider_timeout(provider.name, config.max_polls))
}

#[derive(Debug, Clone, Copy)]
enum AuthStyle {
    Bearer,
    ApiKey,
}

fn auth_headers(style: AuthStyle, api_key: &str) -> Result<HeaderMap, ResearchError> {
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    match style {
        AuthStyle::Bearer => {
            let value = HeaderValue::from_str(&format!("Bearer {api_key}")).map_err(|err| {
                ResearchError::protocol(
                    "invalid-provider-credential",
                    err.to_string(),
                    "Remove control characters from the configured provider API key.",
                    "task-jam-svc-research",
                )
            })?;
            headers.insert(AUTHORIZATION, value);
        }
        AuthStyle::ApiKey => {
            let value = HeaderValue::from_str(api_key).map_err(|err| {
                ResearchError::protocol(
                    "invalid-provider-credential",
                    err.to_string(),
                    "Remove control characters from the configured provider API key.",
                    "task-jam-svc-research",
                )
            })?;
            headers.insert("x-api-key", value);
        }
    }
    Ok(headers)
}

async fn post_json(
    client: &reqwest::Client,
    provider: &str,
    url: &str,
    headers: HeaderMap,
    body: &Value,
) -> Result<Value, ResearchError> {
    let response = client
        .post(url)
        .headers(headers)
        .json(body)
        .send()
        .await
        .map_err(|err| provider_transport_error(provider, &err))?;
    response_json(provider, response).await
}

async fn get_json(
    client: &reqwest::Client,
    provider: &str,
    url: &str,
    headers: HeaderMap,
) -> Result<Value, ResearchError> {
    let response = client
        .get(url)
        .headers(headers)
        .send()
        .await
        .map_err(|err| provider_transport_error(provider, &err))?;
    response_json(provider, response).await
}

async fn response_json(
    provider: &str,
    response: reqwest::Response,
) -> Result<Value, ResearchError> {
    let status = response.status();
    let body = response.text().await.map_err(|err| {
        ResearchError::protocol(
            "research-provider-read-failed",
            format!("{provider}: {err}"),
            "Retry the provider request; if this persists, check provider status.",
            "principle-failure-surfaces-immediately",
        )
    })?;
    if !status.is_success() {
        return Err(ResearchError::protocol(
            "research-provider-http-failed",
            format!("{provider} returned HTTP {status}: {}", compact_body(&body)),
            "Check provider credentials, account quota, and request shape.",
            "task-jam-svc-research",
        ));
    }
    serde_json::from_str(&body).map_err(|err| {
        ResearchError::protocol(
            "research-provider-invalid-json",
            format!("{provider} returned invalid JSON: {err}"),
            "Check whether the provider API contract changed.",
            "task-jam-svc-research",
        )
    })
}

fn tavily_output(
    request_id: String,
    status: Value,
    transcript: Vec<Value>,
) -> ProviderResearchOutput {
    let report_markdown = markdown_from_value(status.get("content"));
    let sources = value_array(status.get("sources"));
    ProviderResearchOutput {
        provider_request_id: Some(request_id),
        report_markdown,
        findings: json!({
            "content": status.get("content").cloned().unwrap_or(Value::Null),
            "source_count": sources.len(),
        }),
        sources,
        transcript,
        raw_response: status,
    }
}

fn sonar_output(provider: &str, response: Value) -> Result<ProviderResearchOutput, ResearchError> {
    let Some(report) = response
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
    else {
        return Err(invalid_provider_response(
            provider,
            "missing choices[0].message.content",
        ));
    };
    let mut sources = value_array(response.get("search_results"));
    if sources.is_empty() {
        sources = response
            .get("citations")
            .and_then(Value::as_array)
            .map_or_else(Vec::new, |citations| {
                citations
                    .iter()
                    .filter_map(Value::as_str)
                    .map(|url| json!({ "url": url }))
                    .collect()
            });
    }
    Ok(ProviderResearchOutput {
        provider_request_id: response
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_owned),
        report_markdown: report.to_owned(),
        findings: json!({
            "usage": response.get("usage").cloned().unwrap_or(Value::Null),
            "related_questions": response.get("related_questions").cloned().unwrap_or(Value::Null),
            "source_count": sources.len(),
        }),
        sources,
        transcript: vec![json!({
            "event": "provider-response",
            "provider": provider,
            "raw": response,
        })],
        raw_response: response,
    })
}

fn exa_output(provider: &str, response: Value) -> Result<ProviderResearchOutput, ResearchError> {
    let output = response
        .get("output")
        .and_then(|value| value.get("content"));
    let report_markdown = output
        .and_then(|value| value.get("report"))
        .and_then(Value::as_str)
        .map_or_else(|| markdown_from_value(output), str::to_owned);
    if report_markdown.trim().is_empty() {
        return Err(invalid_provider_response(
            provider,
            "missing output.content.report",
        ));
    }
    let mut sources = value_array(response.get("results"));
    sources.extend(
        response
            .pointer("/output/grounding")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .flat_map(|grounding| value_array(grounding.get("citations"))),
    );
    Ok(ProviderResearchOutput {
        provider_request_id: response
            .get("requestId")
            .and_then(Value::as_str)
            .map(str::to_owned),
        report_markdown,
        findings: json!({
            "output": output.cloned().unwrap_or(Value::Null),
            "cost_dollars": response.get("costDollars").cloned().unwrap_or(Value::Null),
            "search_type": response.get("searchType").cloned().unwrap_or(Value::Null),
            "source_count": sources.len(),
        }),
        sources,
        transcript: vec![json!({
            "event": "provider-response",
            "provider": provider,
            "raw": response,
        })],
        raw_response: response,
    })
}

fn parallel_output(
    run_id: String,
    result: Value,
    transcript: Vec<Value>,
) -> ProviderResearchOutput {
    let report_markdown = markdown_from_value(result.pointer("/output/content"));
    let sources = value_array(result.pointer("/output/basis"));
    ProviderResearchOutput {
        provider_request_id: Some(run_id),
        report_markdown,
        findings: json!({
            "run": result.get("run").cloned().unwrap_or(Value::Null),
            "output_type": result.pointer("/output/type").cloned().unwrap_or(Value::Null),
            "source_count": sources.len(),
        }),
        sources,
        transcript,
        raw_response: result,
    }
}

fn write_provider_research_output(
    output_dir: &Path,
    research_id: &str,
    question: &str,
    tier: ResearchTier,
    provider: &str,
    output: &ProviderResearchOutput,
    ctx: &TraceCtx,
) -> Result<(), ResearchError> {
    fs::create_dir_all(output_dir).map_err(|err| {
        ResearchError::protocol(
            "research-output-write-failed",
            format!("create {}: {err}", output_dir.display()),
            "Verify JAM_RESEARCH_ROOT is writable by the runtime user.",
            "comp-jam-svc-research",
        )
    })?;
    write_file(&output_dir.join("report.md"), &output.report_markdown)?;
    write_file(
        &output_dir.join("findings.json"),
        &serde_json::to_string_pretty(&json!({
            "research_id": research_id,
            "question": question,
            "tier": tier.as_str(),
            "provider": provider,
            "provider_request_id": output.provider_request_id.as_deref(),
            "findings": output.findings.clone(),
        }))
        .expect("findings serializes"),
    )?;
    write_jsonl_values(&output_dir.join("sources.jsonl"), &output.sources)?;
    let mut transcript = output.transcript.clone();
    transcript.push(json!({
        "at": Utc::now(),
        "event": "provider-completed",
        "trace_id": ctx.trace_id.to_string(),
    }));
    write_jsonl_values(&output_dir.join("transcript.jsonl"), &transcript)?;
    write_file(
        &output_dir.join("metadata.json"),
        &serde_json::to_string_pretty(&json!({
            "research_id": research_id,
            "tier": tier.as_str(),
            "provider": provider,
            "provider_request_id": output.provider_request_id.as_deref(),
            "status": "completed",
            "trace_id": ctx.trace_id.to_string(),
            "completed_at": Utc::now(),
            "raw_response": output.raw_response.clone(),
        }))
        .expect("metadata serializes"),
    )?;
    Ok(())
}

struct ResearchJournalFields<'a> {
    research_id: &'a str,
    question: &'a str,
    tier: ResearchTier,
    provider: &'a str,
    status: &'a str,
    output_dir: &'a Path,
}

async fn publish_research_journal(
    nats: &JamNats,
    subject: &str,
    fields: &ResearchJournalFields<'_>,
    ctx: &TraceCtx,
) -> Result<(), ResearchError> {
    let output_dir = fields.output_dir.display().to_string();
    let event = ResearchJournalEvent {
        research_id: fields.research_id,
        question: fields.question,
        tier: fields.tier.as_str(),
        provider: fields.provider,
        status: fields.status,
        output_dir: &output_dir,
    };
    nats.publish_traced(subject, &event, ctx)
        .await
        .map_err(|err| {
            ResearchError::protocol(
                "research-journal-publish-failed",
                err.to_string(),
                "Verify NATS is running and journal streams are configured.",
                "principle-failure-surfaces-immediately",
            )
        })
}

async fn maybe_create_tempyr_node(
    nats: &JamNats,
    config: &ResearchConfig,
    output_dir: &Path,
    research_id: &str,
    ctx: &TraceCtx,
) -> Result<Option<String>, ResearchError> {
    let Some(graph_dir) = &config.tempyr_graph_dir else {
        return Ok(None);
    };
    let node_id = create_tempyr_research_node(graph_dir, output_dir, research_id)?;
    let output_dir = output_dir.display().to_string();
    let event = ResearchTempyrNodeCreatedEvent {
        research_id,
        node_id: &node_id,
        output_dir: &output_dir,
    };
    nats.publish_traced("journal.research.tempyr-node-created", &event, ctx)
        .await
        .map_err(|err| {
            ResearchError::protocol(
                "research-tempyr-node-event-failed",
                err.to_string(),
                "Verify NATS is running and journal streams are configured.",
                "principle-failure-surfaces-immediately",
            )
        })?;
    Ok(Some(node_id))
}

fn create_tempyr_research_node(
    graph_dir: &Path,
    output_dir: &Path,
    research_id: &str,
) -> Result<String, ResearchError> {
    let findings = read_json_file(&output_dir.join("findings.json"))?;
    let metadata = read_json_file(&output_dir.join("metadata.json"))?;
    let report = fs::read_to_string(output_dir.join("report.md")).map_err(|err| {
        ResearchError::protocol(
            "research-output-read-failed",
            format!("read report.md from {}: {err}", output_dir.display()),
            "Verify provider output exists before running the completion handler.",
            "comp-research-completion-handler",
        )
    })?;
    let sources = read_jsonl_file(&output_dir.join("sources.jsonl"))?;
    let node_id = format!("note-research-{}", slugify(research_id));
    let node_path = graph_dir.join("notes").join(format!("{node_id}.md"));
    let provider = metadata
        .get("provider")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let tier = metadata
        .get("tier")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let trace_id = metadata
        .get("trace_id")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let body = research_node_body(research_id, &report, &sources, &findings);
    fs::create_dir_all(graph_dir.join("notes")).map_err(|err| {
        ResearchError::protocol(
            "research-tempyr-node-write-failed",
            format!(
                "create graph notes dir under {}: {err}",
                graph_dir.display()
            ),
            "Verify JAM_RESEARCH_TEMPYR_GRAPH_DIR points to a writable Tempyr graph.",
            "comp-research-completion-handler",
        )
    })?;
    write_file(
        &node_path,
        &format!(
            "---\nid: {node_id}\ntype: note\ncreated: {}\nupdated: {}\nresearch_provider: {provider}\nresearch_tier: {tier}\ntrace_id: {trace_id}\nsources_count: {}\nedges: []\n---\n{body}\n",
            Utc::now().to_rfc3339(),
            Utc::now().to_rfc3339(),
            sources.len(),
        ),
    )?;
    Ok(node_id)
}

fn research_node_body(
    research_id: &str,
    report: &str,
    sources: &[Value],
    findings: &Value,
) -> String {
    let mut body = format!(
        "# Research: {research_id}\n\n{}\n\n## Sources\n",
        report.trim()
    );
    if sources.is_empty() {
        body.push_str("\nNo structured sources recorded.\n");
    } else {
        for source in sources {
            body.push_str("\n- ");
            body.push_str(&source_link(source));
            body.push('\n');
        }
    }
    body.push_str("\n## Structured Findings\n\n```json\n");
    body.push_str(&serde_json::to_string_pretty(findings).expect("findings serializes"));
    body.push_str("\n```\n");
    body
}

fn source_link(source: &Value) -> String {
    let title = source
        .get("title")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("source");
    let Some(url) = source.get("url").and_then(Value::as_str) else {
        return title.to_owned();
    };
    format!("[{title}]({url})")
}

fn read_json_file(path: &Path) -> Result<Value, ResearchError> {
    let raw = fs::read_to_string(path).map_err(|err| {
        ResearchError::protocol(
            "research-output-read-failed",
            format!("read {}: {err}", path.display()),
            "Verify provider output exists before running the completion handler.",
            "comp-research-completion-handler",
        )
    })?;
    serde_json::from_str(&raw).map_err(|err| {
        ResearchError::protocol(
            "research-output-invalid-json",
            format!("parse {}: {err}", path.display()),
            "Inspect provider output and retry the research request.",
            "comp-research-completion-handler",
        )
    })
}

fn read_jsonl_file(path: &Path) -> Result<Vec<Value>, ResearchError> {
    let raw = fs::read_to_string(path).map_err(|err| {
        ResearchError::protocol(
            "research-output-read-failed",
            format!("read {}: {err}", path.display()),
            "Verify provider output exists before running the completion handler.",
            "comp-research-completion-handler",
        )
    })?;
    raw.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| {
            serde_json::from_str(line).map_err(|err| {
                ResearchError::protocol(
                    "research-output-invalid-json",
                    format!("parse {} JSONL line: {err}", path.display()),
                    "Inspect provider output and retry the research request.",
                    "comp-research-completion-handler",
                )
            })
        })
        .collect()
}

fn write_fake_research_output(
    output_dir: &Path,
    research_id: &str,
    question: &str,
    tier: ResearchTier,
    provider: &str,
    ctx: &TraceCtx,
) -> Result<(), ResearchError> {
    fs::create_dir_all(output_dir).map_err(|err| {
        ResearchError::protocol(
            "research-output-write-failed",
            format!("create {}: {err}", output_dir.display()),
            "Verify JAM_RESEARCH_ROOT is writable by the runtime user.",
            "comp-jam-svc-research",
        )
    })?;
    write_file(
        &output_dir.join("report.md"),
        &format!(
            "# Research Report\n\nFake provider smoke output for `{research_id}`.\n\nQuestion: {question}\n"
        ),
    )?;
    write_file(
        &output_dir.join("findings.json"),
        &serde_json::to_string_pretty(&json!({
            "research_id": research_id,
            "question": question,
            "tier": tier.as_str(),
            "provider": provider,
            "findings": [],
        }))
        .expect("findings serializes"),
    )?;
    write_file(&output_dir.join("sources.jsonl"), "")?;
    write_jsonl(
        &output_dir.join("transcript.jsonl"),
        &json!({
            "at": Utc::now(),
            "event": "fake-provider-completed",
            "trace_id": ctx.trace_id.to_string(),
        }),
    )?;
    write_file(
        &output_dir.join("metadata.json"),
        &serde_json::to_string_pretty(&json!({
            "research_id": research_id,
            "tier": tier.as_str(),
            "provider": provider,
            "status": "completed",
            "trace_id": ctx.trace_id.to_string(),
            "completed_at": Utc::now(),
        }))
        .expect("metadata serializes"),
    )?;
    Ok(())
}

fn write_file(path: &Path, contents: &str) -> Result<(), ResearchError> {
    fs::write(path, contents).map_err(|err| {
        ResearchError::protocol(
            "research-output-write-failed",
            format!("write {}: {err}", path.display()),
            "Verify the research output directory is writable.",
            "comp-jam-svc-research",
        )
    })
}

fn write_jsonl(path: &Path, value: &serde_json::Value) -> Result<(), ResearchError> {
    let mut file = File::create(path).map_err(|err| {
        ResearchError::protocol(
            "research-output-write-failed",
            format!("create {}: {err}", path.display()),
            "Verify the research output directory is writable.",
            "comp-jam-svc-research",
        )
    })?;
    writeln!(file, "{value}").map_err(|err| {
        ResearchError::protocol(
            "research-output-write-failed",
            format!("write {}: {err}", path.display()),
            "Verify the research output directory is writable.",
            "comp-jam-svc-research",
        )
    })
}

fn write_jsonl_values(path: &Path, values: &[Value]) -> Result<(), ResearchError> {
    let mut file = File::create(path).map_err(|err| {
        ResearchError::protocol(
            "research-output-write-failed",
            format!("create {}: {err}", path.display()),
            "Verify the research output directory is writable.",
            "comp-jam-svc-research",
        )
    })?;
    for value in values {
        writeln!(file, "{value}").map_err(|err| {
            ResearchError::protocol(
                "research-output-write-failed",
                format!("write {}: {err}", path.display()),
                "Verify the research output directory is writable.",
                "comp-jam-svc-research",
            )
        })?;
    }
    Ok(())
}

fn endpoint(base_url: &str, path: &str) -> String {
    format!(
        "{}/{}",
        base_url.trim_end_matches('/'),
        path.trim_start_matches('/')
    )
}

fn research_prompt(question: &str, scope: Option<&str>) -> String {
    scope.map_or_else(
        || question.to_owned(),
        |scope| format!("Scope: {scope}\n\nQuestion: {question}"),
    )
}

fn required_string(value: &Value, field: &str, provider: &str) -> Result<String, ResearchError> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|raw| !raw.trim().is_empty())
        .map(str::to_owned)
        .ok_or_else(|| invalid_provider_response(provider, &format!("missing {field}")))
}

fn status_field(value: &Value) -> Option<String> {
    value
        .get("status")
        .and_then(Value::as_str)
        .map(|status| status.trim().to_ascii_lowercase())
}

fn markdown_from_value(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(raw)) => raw.to_owned(),
        Some(Value::Null) | None => String::new(),
        Some(other) => serde_json::to_string_pretty(other).expect("JSON value serializes"),
    }
}

fn value_array(value: Option<&Value>) -> Vec<Value> {
    value
        .and_then(Value::as_array)
        .map_or_else(Vec::new, Clone::clone)
}

fn provider_transport_error(provider: &str, err: &reqwest::Error) -> ResearchError {
    ResearchError::protocol(
        "research-provider-transport-failed",
        format!("{provider}: {err}"),
        "Check network access, provider base URL configuration, and provider status.",
        "task-jam-svc-research",
    )
}

fn provider_failed(provider: &str, response: &Value) -> ResearchError {
    ResearchError::protocol(
        "research-provider-task-failed",
        format!(
            "{provider} reported failure: {}",
            compact_body(&response.to_string())
        ),
        "Inspect transcript.jsonl for the provider response, then retry or switch tiers.",
        "task-jam-svc-research",
    )
}

fn provider_timeout(provider: &str, max_polls: u32) -> ResearchError {
    ResearchError::protocol(
        "research-provider-timeout",
        format!("{provider} did not complete after {max_polls} polls"),
        "Increase JAM_RESEARCH_MAX_POLLS or retry with a lower-cost tier.",
        "task-jam-svc-research",
    )
}

fn invalid_provider_response(provider: &str, detail: &str) -> ResearchError {
    ResearchError::protocol(
        "research-provider-invalid-response",
        format!("{provider}: {detail}"),
        "Check whether the provider API contract changed.",
        "task-jam-svc-research",
    )
}

fn compact_body(body: &str) -> String {
    let compact = body.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.len() > 500 {
        format!("{}...", &compact[..500])
    } else {
        compact
    }
}

fn validate_existing_graph_dir(path: &Path) -> Result<(), ResearchError> {
    if path.is_dir() {
        return Ok(());
    }
    Err(ResearchError::protocol(
        "invalid-research-tempyr-graph-dir",
        format!("{} is not an existing graph directory", path.display()),
        "Set JAM_RESEARCH_TEMPYR_GRAPH_DIR to the canonical Tempyr graph directory or unset it.",
        "principle-failure-surfaces-immediately",
    ))
}

fn research_id(scope: Option<&str>, ctx: &TraceCtx) -> String {
    let prefix = scope.map_or_else(|| "research".to_owned(), slugify);
    let trace = ctx.trace_id.to_string();
    format!("{prefix}-{}", &trace[trace.len() - 8..])
}

fn slugify(value: &str) -> String {
    let mut slug = String::new();
    let mut last_dash = false;
    for ch in value.chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch);
            last_dash = false;
        } else if !last_dash {
            slug.push('-');
            last_dash = true;
        }
    }
    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        "research".into()
    } else {
        slug.into()
    }
}

fn error_response(err: ResearchError) -> Response {
    match err {
        ResearchError::Protocol {
            kind,
            detail,
            remediation,
            tracked_by,
        } => Response::Error {
            error: ResponseError {
                kind: kind.into(),
                detail,
                remediation,
                tracked_by,
            },
        },
    }
}

fn method_from_subject<'a>(subject: &'a str, subject_prefix: &str) -> Option<&'a str> {
    subject.strip_prefix(&format!("{subject_prefix}."))
}

fn configured_subject_prefix(env_key: &str, default: &str) -> String {
    std::env::var(env_key)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| default.to_owned())
}

fn env_path(key: &str) -> Option<PathBuf> {
    std::env::var_os(key)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn env_bool(key: &str, default: bool) -> bool {
    std::env::var(key).map_or(default, |value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

fn env_value(keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        std::env::var(key)
            .ok()
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
    })
}

fn env_string(key: &str, default: &str) -> String {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default.to_owned())
}

fn env_u32(key: &str, default: u32) -> Result<u32, ResearchError> {
    let Some(raw) = std::env::var(key)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
    else {
        return Ok(default);
    };
    raw.parse::<u32>().map_err(|err| {
        ResearchError::protocol(
            "invalid-research-config",
            format!("{key}={raw:?} is not a positive integer: {err}"),
            "Use an unsigned integer value or unset the variable.",
            "principle-failure-surfaces-immediately",
        )
    })
}

fn env_duration_secs(key: &str, default_secs: u64) -> Result<Duration, ResearchError> {
    env_u64(key, default_secs).map(Duration::from_secs)
}

fn env_duration_millis(key: &str, default_millis: u64) -> Result<Duration, ResearchError> {
    env_u64(key, default_millis).map(Duration::from_millis)
}

fn env_u64(key: &str, default: u64) -> Result<u64, ResearchError> {
    let Some(raw) = std::env::var(key)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
    else {
        return Ok(default);
    };
    raw.parse::<u64>().map_err(|err| {
        ResearchError::protocol(
            "invalid-research-config",
            format!("{key}={raw:?} is not a positive integer: {err}"),
            "Use an unsigned integer value or unset the variable.",
            "principle-failure-surfaces-immediately",
        )
    })
}

fn default_jam_home() -> Option<PathBuf> {
    env_path("HOME").map(|home| home.join(".jam"))
}

#[cfg(test)]
mod tests {
    use super::{
        create_tempyr_research_node, run_exa_deep_reasoning_research, run_parallel_research,
        run_sonar_research, run_tavily_research, select_provider, slugify, ProviderKind,
        ResearchConfig, ResearchTier, SelectedProvider,
    };
    use axum::extract::Path as AxumPath;
    use axum::routing::{get, post};
    use axum::{Json, Router};
    use serde_json::{json, Value};
    use std::fs;
    use std::path::PathBuf;
    use std::time::Duration;
    use tokio::net::TcpListener;

    #[test]
    fn slugify_research_scope() {
        assert_eq!(slugify("Blueberry/terrain LOD"), "blueberry-terrain-lod");
    }

    #[test]
    fn fake_provider_uses_tier_name() {
        let config = ResearchConfig {
            research_root: PathBuf::from("/tmp/research"),
            fake_provider: true,
            tempyr_graph_dir: None,
            http_timeout: Duration::from_secs(1),
            poll_interval: Duration::from_millis(1),
            max_polls: 2,
            secrets_file: None,
            tavily_base_url: String::new(),
            perplexity_base_url: String::new(),
            exa_base_url: String::new(),
            parallel_base_url: String::new(),
        };
        assert_eq!(
            select_provider(ResearchTier::Deep, &config).unwrap().name,
            "fake-deep",
        );
    }

    #[test]
    fn deep_provider_uses_exa_secret_from_file_backend() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        fs::write(
            tmp.path(),
            "[secrets]\n\"jam/search/exa\" = \"exa-test-key\"\n",
        )
        .unwrap();
        let config = ResearchConfig {
            research_root: PathBuf::from("/tmp/research"),
            fake_provider: false,
            tempyr_graph_dir: None,
            http_timeout: Duration::from_secs(1),
            poll_interval: Duration::from_millis(1),
            max_polls: 2,
            secrets_file: Some(tmp.path().to_path_buf()),
            tavily_base_url: String::new(),
            perplexity_base_url: String::new(),
            exa_base_url: String::new(),
            parallel_base_url: String::new(),
        };

        let provider = select_provider(ResearchTier::Deep, &config).unwrap();

        assert_eq!(provider.name, "exa-deep-reasoning");
        assert_eq!(provider.api_key, "exa-test-key");
    }

    #[tokio::test]
    async fn tavily_adapter_posts_then_polls_completed_report() {
        async fn create(Json(body): Json<Value>) -> Json<Value> {
            assert!(body["input"]
                .as_str()
                .is_some_and(|input| input.contains("Blueberry")));
            Json(json!({
                "request_id": "research-1",
                "status": "pending",
            }))
        }

        async fn poll(AxumPath(request_id): AxumPath<String>) -> Json<Value> {
            assert_eq!(request_id, "research-1");
            Json(json!({
                "request_id": request_id,
                "status": "completed",
                "content": "# Report\n\nBlueberry findings.",
                "sources": [
                    {"title": "Source", "url": "https://example.com/source"}
                ],
            }))
        }

        let base_url = spawn_mock(
            Router::new()
                .route("/research", post(create))
                .route("/research/{request_id}", get(poll)),
        )
        .await;
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap();
        let config = test_config(&base_url);
        let provider = SelectedProvider {
            name: "tavily",
            kind: ProviderKind::Tavily,
            api_key: "test-key".into(),
        };

        let output = run_tavily_research(&client, &provider, "Research Blueberry", &config)
            .await
            .unwrap();

        assert_eq!(output.provider_request_id.as_deref(), Some("research-1"));
        assert!(output.report_markdown.contains("Blueberry findings"));
        assert_eq!(output.sources.len(), 1);
        assert_eq!(output.transcript.len(), 2);
    }

    #[tokio::test]
    async fn sonar_adapter_normalizes_report_usage_and_sources() {
        async fn sonar(Json(body): Json<Value>) -> Json<Value> {
            assert_eq!(body["model"], "sonar-pro");
            Json(json!({
                "id": "chat-1",
                "choices": [
                    {
                        "message": {
                            "role": "assistant",
                            "content": "Grounded report."
                        }
                    }
                ],
                "usage": {"total_tokens": 123},
                "search_results": [
                    {"title": "Doc", "url": "https://example.com/doc"}
                ],
            }))
        }

        let base_url = spawn_mock(Router::new().route("/v1/sonar", post(sonar))).await;
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap();
        let mut config = test_config("http://unused");
        config.perplexity_base_url = base_url;
        let provider = SelectedProvider {
            name: "sonar-pro",
            kind: ProviderKind::Sonar { model: "sonar-pro" },
            api_key: "test-key".into(),
        };

        let output = run_sonar_research(
            &client,
            &provider,
            "sonar-pro",
            "Research Blueberry",
            &config,
        )
        .await
        .unwrap();

        assert_eq!(output.provider_request_id.as_deref(), Some("chat-1"));
        assert_eq!(output.report_markdown, "Grounded report.");
        assert_eq!(output.sources.len(), 1);
        assert_eq!(output.findings["usage"]["total_tokens"], 123);
    }

    #[tokio::test]
    async fn exa_adapter_uses_deep_reasoning_search_endpoint() {
        async fn search(Json(body): Json<Value>) -> Json<Value> {
            assert_eq!(body["type"], "deep-reasoning");
            Json(json!({
                "requestId": "exa-1",
                "searchType": "deep-reasoning",
                "results": [
                    {"title": "Result", "url": "https://example.com/result"}
                ],
                "output": {
                    "content": {
                        "report": "Exa report.",
                        "findings": ["Finding"]
                    },
                    "grounding": [
                        {
                            "field": "report",
                            "citations": [
                                {"title": "Citation", "url": "https://example.com/citation"}
                            ]
                        }
                    ]
                },
                "costDollars": {"total": 0.015}
            }))
        }

        let base_url = spawn_mock(Router::new().route("/search", post(search))).await;
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap();
        let mut config = test_config("http://unused");
        config.exa_base_url = base_url;
        let provider = SelectedProvider {
            name: "exa-deep-reasoning",
            kind: ProviderKind::ExaDeepReasoning,
            api_key: "test-key".into(),
        };

        let output =
            run_exa_deep_reasoning_research(&client, &provider, "Research Blueberry", &config)
                .await
                .unwrap();

        assert_eq!(output.provider_request_id.as_deref(), Some("exa-1"));
        assert_eq!(output.report_markdown, "Exa report.");
        assert_eq!(output.sources.len(), 2);
    }

    #[tokio::test]
    async fn parallel_adapter_creates_polls_and_fetches_result() {
        async fn create(Json(body): Json<Value>) -> Json<Value> {
            assert_eq!(body["processor"], "pro");
            Json(json!({
                "run_id": "trun_1",
                "status": "queued",
                "is_active": true,
                "processor": "pro",
            }))
        }

        async fn poll(AxumPath(run_id): AxumPath<String>) -> Json<Value> {
            assert_eq!(run_id, "trun_1");
            Json(json!({
                "run_id": run_id,
                "status": "completed",
                "is_active": false,
                "processor": "pro",
            }))
        }

        async fn result(AxumPath(run_id): AxumPath<String>) -> Json<Value> {
            assert_eq!(run_id, "trun_1");
            Json(json!({
                "run": {
                    "run_id": run_id,
                    "status": "completed",
                    "processor": "pro"
                },
                "output": {
                    "type": "text",
                    "content": "Parallel report.",
                    "basis": [
                        {"field": "content", "citations": [{"url": "https://example.com"}]}
                    ]
                }
            }))
        }

        let base_url = spawn_mock(
            Router::new()
                .route("/v1/tasks/runs", post(create))
                .route("/v1/tasks/runs/{run_id}", get(poll))
                .route("/v1/tasks/runs/{run_id}/result", get(result)),
        )
        .await;
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap();
        let mut config = test_config("http://unused");
        config.parallel_base_url = base_url;
        let provider = SelectedProvider {
            name: "parallel-pro",
            kind: ProviderKind::Parallel { processor: "pro" },
            api_key: "test-key".into(),
        };

        let output =
            run_parallel_research(&client, &provider, "pro", "Research Blueberry", &config)
                .await
                .unwrap();

        assert_eq!(output.provider_request_id.as_deref(), Some("trun_1"));
        assert_eq!(output.report_markdown, "Parallel report.");
        assert_eq!(output.sources.len(), 1);
        assert_eq!(output.transcript.len(), 3);
    }

    #[test]
    fn completion_handler_creates_stable_tempyr_note() {
        let root =
            std::env::temp_dir().join(format!("jam-research-node-test-{}", std::process::id()));
        let graph = root.join("graph");
        let output = root.join("research").join("blueberry-research-00000013");
        fs::create_dir_all(&graph).unwrap();
        fs::create_dir_all(&output).unwrap();
        fs::write(output.join("report.md"), "# Report\n\nUseful research.").unwrap();
        fs::write(
            output.join("findings.json"),
            serde_json::to_string_pretty(&json!({
                "research_id": "blueberry-research-00000013",
                "findings": ["Useful"],
            }))
            .unwrap(),
        )
        .unwrap();
        fs::write(
            output.join("sources.jsonl"),
            "{\"title\":\"Source\",\"url\":\"https://example.com/source\"}\n",
        )
        .unwrap();
        fs::write(
            output.join("metadata.json"),
            serde_json::to_string_pretty(&json!({
                "provider": "fake-deep",
                "tier": "deep",
                "trace_id": "01HXDK00000000000000000013",
            }))
            .unwrap(),
        )
        .unwrap();

        let node_id =
            create_tempyr_research_node(&graph, &output, "blueberry-research-00000013").unwrap();
        let node = fs::read_to_string(graph.join("notes").join(format!("{node_id}.md"))).unwrap();

        assert_eq!(node_id, "note-research-blueberry-research-00000013");
        assert!(node.contains("research_provider: fake-deep"));
        assert!(node.contains("[Source](https://example.com/source)"));
        assert!(node.contains("Useful research."));
        fs::remove_dir_all(root).unwrap();
    }

    async fn spawn_mock(router: Router) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        format!("http://{addr}")
    }

    fn test_config(base_url: &str) -> ResearchConfig {
        ResearchConfig {
            research_root: PathBuf::from("/tmp/research"),
            fake_provider: false,
            tempyr_graph_dir: None,
            http_timeout: Duration::from_secs(2),
            poll_interval: Duration::from_millis(1),
            max_polls: 2,
            secrets_file: None,
            tavily_base_url: base_url.to_owned(),
            perplexity_base_url: base_url.to_owned(),
            exa_base_url: base_url.to_owned(),
            parallel_base_url: base_url.to_owned(),
        }
    }
}
