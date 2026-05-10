//! `jam-svc-evolve` - skill evolution coordination.
//!
//! The service exposes `tool.evolve.request-skill-evolution`, resolves a
//! Jamboree skill, and invokes the vendored Hermes self-evolution adapter as a
//! subprocess. The subprocess boundary preserves §17.1 / §2.9: the trusted
//! service does not import Hermes modules.

#![deny(missing_docs)]

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use futures::StreamExt;
use jam_nats::async_nats;
use jam_nats::JamNats;
use jam_trace::TraceCtx;
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use tracing::{debug, error, info, warn};

const SERVICE_NAME: &str = "jam-svc-evolve";
const SERVICE_VERSION: &str = env!("CARGO_PKG_VERSION");
const SUBJECT_PREFIX: &str = "tool.evolve";
const SUBJECT_PREFIX_ENV: &str = "JAM_EVOLVE_SUBJECT_PREFIX";
const DEFAULT_ITERATIONS: u32 = 10;
const MAX_SKILL_NAME_LEN: usize = 200;
const MAX_OUTPUT_TAIL: usize = 4_000;

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
enum EvolveError {
    #[error("{kind}: {detail}")]
    Protocol {
        kind: &'static str,
        detail: String,
        remediation: String,
        tracked_by: &'static str,
    },
}

impl EvolveError {
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
struct EvolveConfig {
    skills_dir: PathBuf,
    candidate_dir: PathBuf,
    adapter: PathBuf,
    vendor_dir: PathBuf,
    uv_bin: PathBuf,
    use_uv: bool,
    python_bin: PathBuf,
    dry_run: bool,
    iterations: u32,
    optimizer_model: String,
    eval_model: String,
    dataset_dir: Option<PathBuf>,
}

impl EvolveConfig {
    fn from_env() -> Result<Self, EvolveError> {
        let repo_root = env_path("JAM_REPO_ROOT")
            .or_else(|| std::env::current_dir().ok())
            .ok_or_else(|| {
                EvolveError::protocol(
                    "missing-repo-root",
                    "could not determine Jamboree repo root",
                    "Set JAM_REPO_ROOT=/home/caleb/jamboree.",
                    "principle-failure-surfaces-immediately",
                )
            })?;
        let jam_home = env_path("JAM_HOME")
            .or_else(default_jam_home)
            .ok_or_else(|| {
                EvolveError::protocol(
                    "missing-jam-home",
                    "JAM_HOME is unset and HOME is unavailable",
                    "Set JAM_HOME=/home/maestro/.jam for runtime services.",
                    "principle-failure-surfaces-immediately",
                )
            })?;
        let skills_dir =
            env_path("JAM_EVOLVE_SKILLS_DIR").unwrap_or_else(|| repo_root.join("skills"));
        let candidate_dir = env_path("JAM_EVOLVE_CANDIDATE_DIR")
            .unwrap_or_else(|| jam_home.join("skills-evolution-candidates"));
        let adapter = env_path("JAM_EVOLVE_ADAPTER")
            .unwrap_or_else(|| repo_root.join("evolution/jamboree_evolve_skill.py"));
        let vendor_dir = env_path("JAM_EVOLVE_VENDOR_DIR")
            .unwrap_or_else(|| repo_root.join("evolution/hermes-agent-self-evolution"));
        let config = Self {
            skills_dir,
            candidate_dir,
            adapter,
            vendor_dir,
            uv_bin: env_path("JAM_EVOLVE_UV_BIN").unwrap_or_else(|| PathBuf::from("uv")),
            use_uv: env_bool("JAM_EVOLVE_USE_UV", true),
            python_bin: env_path("JAM_EVOLVE_PYTHON").unwrap_or_else(|| PathBuf::from("python3")),
            dry_run: env_bool("JAM_EVOLVE_DRY_RUN", false),
            iterations: env_u32("JAM_EVOLVE_ITERATIONS", DEFAULT_ITERATIONS),
            optimizer_model: std::env::var("JAM_EVOLVE_OPTIMIZER_MODEL")
                .unwrap_or_else(|_| "openai/gpt-4.1".into()),
            eval_model: std::env::var("JAM_EVOLVE_EVAL_MODEL")
                .unwrap_or_else(|_| "openai/gpt-4.1-mini".into()),
            dataset_dir: env_path("JAM_EVOLVE_DATASET_DIR"),
        };
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<(), EvolveError> {
        require_dir(
            &self.skills_dir,
            "missing-skills-dir",
            "JAM_EVOLVE_SKILLS_DIR",
        )?;
        require_file(
            &self.adapter,
            "missing-evolution-adapter",
            "JAM_EVOLVE_ADAPTER",
        )?;
        require_file(
            &self.vendor_dir.join("pyproject.toml"),
            "missing-evolution-vendor",
            "JAM_EVOLVE_VENDOR_DIR",
        )?;
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
struct RequestSkillEvolutionInput {
    skill_name: String,
    eval_source: Option<String>,
    reason: Option<String>,
}

#[derive(Debug, Serialize)]
struct RequestSkillEvolutionOutput {
    status: String,
    skill_name: String,
    skill_path: String,
    candidate_path: Option<String>,
    eval_source: String,
    dry_run: bool,
    trace_id: String,
    stdout_tail: String,
    stderr_tail: String,
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
        error!("jam-svc-evolve fatal: {err}");
        return std::process::ExitCode::FAILURE;
    }
    std::process::ExitCode::SUCCESS
}

async fn run() -> Result<(), ServiceError> {
    init_tracing();

    let nats_url = std::env::var("NATS_URL").unwrap_or_else(|_| "nats://127.0.0.1:4222".into());
    let nats_token = std::env::var("NATS_TOKEN").ok();
    let subject_prefix = configured_subject_prefix(SUBJECT_PREFIX_ENV, SUBJECT_PREFIX);
    let config = EvolveConfig::from_env().map_err(|err| ServiceError::Reply(err.to_string()))?;

    info!(
        service = %SERVICE_NAME,
        version = %SERVICE_VERSION,
        nats = %nats_url,
        subject_prefix = %subject_prefix,
        skills_dir = %config.skills_dir.display(),
        adapter = %config.adapter.display(),
        dry_run = config.dry_run,
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
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("jam_svc_evolve=info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

async fn handle_request(
    nats: &JamNats,
    msg: &async_nats::Message,
    subject_prefix: &str,
    config: &EvolveConfig,
    draining: &Arc<AtomicBool>,
) -> Result<(), ServiceError> {
    let method = method_from_subject(msg.subject.as_str(), subject_prefix).unwrap_or("");
    debug!(subject = %msg.subject, method = %method, "received request");

    let response_ctx = msg
        .headers
        .as_ref()
        .and_then(jam_nats::extract_trace_from_headers);

    let response = match &response_ctx {
        Some(ctx) => dispatch(method, &msg.payload, ctx, config).await,
        None => Response::Error {
            error: ResponseError {
                kind: "missing-trace".into(),
                detail: "tool.evolve requests must include Trace-Id headers".into(),
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

async fn dispatch(method: &str, payload: &[u8], ctx: &TraceCtx, config: &EvolveConfig) -> Response {
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
        "request-skill-evolution" => match request_skill_evolution(payload, ctx, config).await {
            Ok(output) => Response::Ok(serde_json::to_value(output).expect("output serializes")),
            Err(err) => error_response(err),
        },
        unknown => Response::Error {
            error: ResponseError {
                kind: "unknown-method".into(),
                detail: format!("{SUBJECT_PREFIX}.{unknown} is not a recognized evolve method"),
                remediation: "Use tool.evolve.request-skill-evolution.".into(),
                tracked_by: "api-request-skill-evolution",
            },
        },
    }
}

async fn request_skill_evolution(
    payload: &[u8],
    ctx: &TraceCtx,
    config: &EvolveConfig,
) -> Result<RequestSkillEvolutionOutput, EvolveError> {
    let input = parse_request_input(payload)?;
    let skill_name = validate_skill_name(&input.skill_name)?;
    let eval_source = validate_eval_source(input.eval_source.as_deref())?;
    if let Some(reason) = input.reason.as_deref() {
        validate_reason(reason)?;
    }
    let skill_path = find_skill(&config.skills_dir, &skill_name)?;
    let output = run_adapter(config, &skill_name, &skill_path, &eval_source).await?;
    let status = if config.dry_run {
        "dry-run-complete"
    } else {
        "candidate-written"
    };
    Ok(RequestSkillEvolutionOutput {
        status: status.into(),
        skill_name,
        skill_path: skill_path.display().to_string(),
        candidate_path: output.candidate_path,
        eval_source,
        dry_run: config.dry_run,
        trace_id: ctx.trace_id.to_string(),
        stdout_tail: tail(&output.stdout, MAX_OUTPUT_TAIL),
        stderr_tail: tail(&output.stderr, MAX_OUTPUT_TAIL),
    })
}

fn parse_request_input(payload: &[u8]) -> Result<RequestSkillEvolutionInput, EvolveError> {
    serde_json::from_slice(payload).map_err(|err| {
        EvolveError::protocol(
            "invalid-input",
            format!("tool.evolve.request-skill-evolution payload is invalid JSON: {err}"),
            "Send {\"skill_name\":\"task-types/light-edit\"}.",
            "api-request-skill-evolution",
        )
    })
}

fn validate_skill_name(raw: &str) -> Result<String, EvolveError> {
    let skill_name = raw.trim();
    if skill_name.is_empty() {
        return Err(EvolveError::protocol(
            "invalid-skill-name",
            "skill_name must not be empty",
            "Pass a skill scope or file stem under the configured skills directory.",
            "api-request-skill-evolution",
        ));
    }
    if skill_name.len() > MAX_SKILL_NAME_LEN || skill_name.contains('\0') {
        return Err(EvolveError::protocol(
            "invalid-skill-name",
            "skill_name is too long or contains NUL",
            "Use a short skill scope such as task-types/light-edit.",
            "api-request-skill-evolution",
        ));
    }
    if skill_name.contains("..") {
        return Err(EvolveError::protocol(
            "invalid-skill-name",
            "skill_name must not contain '..'",
            "Use a skill scope; arbitrary path traversal is not supported.",
            "principle-native-fs-only",
        ));
    }
    Ok(skill_name.to_owned())
}

fn validate_eval_source(raw: Option<&str>) -> Result<String, EvolveError> {
    let eval_source = raw.unwrap_or("golden").trim();
    match eval_source {
        "golden" | "synthetic" | "sessiondb" => Ok(eval_source.to_owned()),
        _ => Err(EvolveError::protocol(
            "invalid-eval-source",
            format!("eval_source {eval_source:?} is not one of golden, synthetic, sessiondb"),
            "Use the eval_source values supported by the vendored Hermes adapter.",
            "api-request-skill-evolution",
        )),
    }
}

fn validate_reason(reason: &str) -> Result<(), EvolveError> {
    if reason.len() > 1_000 || reason.contains('\0') {
        return Err(EvolveError::protocol(
            "invalid-reason",
            "reason is too long or contains NUL",
            "Send a short human-readable reason.",
            "api-request-skill-evolution",
        ));
    }
    Ok(())
}

#[derive(Debug)]
struct AdapterOutput {
    stdout: String,
    stderr: String,
    candidate_path: Option<String>,
}

async fn run_adapter(
    config: &EvolveConfig,
    skill_name: &str,
    skill_path: &Path,
    eval_source: &str,
) -> Result<AdapterOutput, EvolveError> {
    let mut command = build_adapter_command(config, skill_path, eval_source);
    let output = command.output().await.map_err(|err| {
        EvolveError::protocol(
            "evolution-subprocess-failed",
            format!("failed to start evolution subprocess for {skill_name}: {err}"),
            "Verify uv/python and the vendored Hermes evolution dependencies are installed.",
            "comp-hermes-evolution-subsystem",
        )
    })?;

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    if !output.status.success() {
        return Err(EvolveError::protocol(
            "evolution-subprocess-failed",
            format!(
                "evolution subprocess for {skill_name} exited with status {}: {}{}",
                output.status,
                tail(&stdout, MAX_OUTPUT_TAIL),
                tail(&stderr, MAX_OUTPUT_TAIL),
            ),
            "Seed the DSPy/LiteLLM model credential, provide a dataset for golden evals, then retry.",
            "task-vendor-hermes-evolution",
        ));
    }

    let candidate_path = stdout
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| {
            !line.is_empty()
                && Path::new(line)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("diff"))
        })
        .map(ToOwned::to_owned);
    if !config.dry_run && candidate_path.is_none() {
        return Err(EvolveError::protocol(
            "missing-candidate-diff",
            "evolution subprocess succeeded but did not print a candidate diff path",
            "Inspect the adapter output and ensure it wrote to skills-evolution-candidates.",
            "comp-hermes-evolution-subsystem",
        ));
    }

    Ok(AdapterOutput {
        stdout,
        stderr,
        candidate_path,
    })
}

fn build_adapter_command(config: &EvolveConfig, skill_path: &Path, eval_source: &str) -> Command {
    let mut command = if config.use_uv {
        let mut cmd = Command::new(&config.uv_bin);
        cmd.arg("run")
            .arg("--no-project")
            .arg("--with-editable")
            .arg(&config.vendor_dir)
            .arg("python");
        cmd
    } else {
        Command::new(&config.python_bin)
    };

    command
        .arg(&config.adapter)
        .arg("--skill-path")
        .arg(skill_path)
        .arg("--candidate-dir")
        .arg(&config.candidate_dir)
        .arg("--eval-source")
        .arg(eval_source)
        .arg("--iterations")
        .arg(config.iterations.to_string())
        .arg("--optimizer-model")
        .arg(&config.optimizer_model)
        .arg("--eval-model")
        .arg(&config.eval_model);
    if let Some(dataset_path) = dataset_path_for(config, skill_path) {
        command.arg("--dataset-path").arg(dataset_path);
    }
    if config.dry_run {
        command.arg("--dry-run");
    }
    command
}

fn dataset_path_for(config: &EvolveConfig, skill_path: &Path) -> Option<PathBuf> {
    config.dataset_dir.as_ref().map(|dataset_dir| {
        let stem = skill_path
            .file_stem()
            .map_or_else(|| OsString::from("skill"), std::ffi::OsStr::to_os_string);
        dataset_dir.join(stem)
    })
}

fn find_skill(skills_dir: &Path, skill_name: &str) -> Result<PathBuf, EvolveError> {
    let canonical_skills_dir = skills_dir.canonicalize().map_err(|err| {
        EvolveError::protocol(
            "skills-dir-unavailable",
            format!("canonicalize {}: {err}", skills_dir.display()),
            "Verify JAM_EVOLVE_SKILLS_DIR points at the Jamboree skills directory.",
            "comp-jam-svc-evolve",
        )
    })?;
    let mut files = Vec::new();
    collect_markdown_files(&canonical_skills_dir, &mut files)?;
    files.sort();
    let wanted_slug = slugify(skill_name);
    for path in files {
        if skill_matches(&path, &canonical_skills_dir, skill_name, &wanted_slug)? {
            return Ok(path);
        }
    }
    Err(EvolveError::protocol(
        "skill-not-found",
        format!(
            "skill {skill_name:?} was not found under {}",
            canonical_skills_dir.display()
        ),
        "Pass a skill scope or file stem that exists under JAM_EVOLVE_SKILLS_DIR.",
        "api-request-skill-evolution",
    ))
}

fn collect_markdown_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), EvolveError> {
    let entries = std::fs::read_dir(dir).map_err(|err| {
        EvolveError::protocol(
            "skills-dir-unavailable",
            format!("read {}: {err}", dir.display()),
            "Verify the skills directory is readable by the runtime user.",
            "comp-jam-svc-evolve",
        )
    })?;
    for entry in entries {
        let entry = entry.map_err(|err| {
            EvolveError::protocol(
                "skills-dir-unavailable",
                format!("read directory entry in {}: {err}", dir.display()),
                "Verify the skills directory is readable by the runtime user.",
                "comp-jam-svc-evolve",
            )
        })?;
        let path = entry.path();
        if path.is_dir() {
            collect_markdown_files(&path, out)?;
        } else if path.extension().is_some_and(|ext| ext == "md") {
            out.push(path);
        }
    }
    Ok(())
}

fn skill_matches(
    path: &Path,
    skills_dir: &Path,
    skill_name: &str,
    wanted_slug: &str,
) -> Result<bool, EvolveError> {
    let stem_matches = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .is_some_and(|stem| stem == skill_name || slugify(stem) == wanted_slug);
    if stem_matches {
        return Ok(true);
    }
    let rel_matches = path
        .strip_prefix(skills_dir)
        .ok()
        .and_then(Path::to_str)
        .is_some_and(|rel| {
            let rel_no_ext = rel.strip_suffix(".md").unwrap_or(rel);
            rel_no_ext == skill_name || slugify(rel_no_ext) == wanted_slug
        });
    if rel_matches {
        return Ok(true);
    }
    let raw = std::fs::read_to_string(path).map_err(|err| {
        EvolveError::protocol(
            "skill-read-failed",
            format!("read {}: {err}", path.display()),
            "Verify the skill file is readable by the runtime user.",
            "api-request-skill-evolution",
        )
    })?;
    let scope = frontmatter_value(&raw, "scope");
    Ok(scope
        .as_deref()
        .is_some_and(|scope| scope == skill_name || slugify(scope) == wanted_slug))
}

fn frontmatter_value(raw: &str, key: &str) -> Option<String> {
    if !raw.trim_start().starts_with("---") {
        return None;
    }
    let mut lines = raw.lines();
    if lines.next()?.trim() != "---" {
        return None;
    }
    for line in lines {
        let trimmed = line.trim();
        if trimmed == "---" {
            return None;
        }
        if let Some(value) = trimmed.strip_prefix(&format!("{key}:")) {
            return Some(value.trim().trim_matches(['"', '\'']).to_owned());
        }
    }
    None
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
    slug.trim_matches('-').to_owned()
}

fn error_response(err: EvolveError) -> Response {
    match err {
        EvolveError::Protocol {
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

fn env_u32(key: &str, default: u32) -> u32 {
    std::env::var(key)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn default_jam_home() -> Option<PathBuf> {
    env_path("HOME").map(|home| home.join(".jam"))
}

fn require_dir(path: &Path, kind: &'static str, env_key: &'static str) -> Result<(), EvolveError> {
    if path.is_dir() {
        Ok(())
    } else {
        Err(EvolveError::protocol(
            kind,
            format!("{} does not exist or is not a directory", path.display()),
            format!("Set {env_key} to a readable directory."),
            "principle-failure-surfaces-immediately",
        ))
    }
}

fn require_file(path: &Path, kind: &'static str, env_key: &'static str) -> Result<(), EvolveError> {
    if path.is_file() {
        Ok(())
    } else {
        Err(EvolveError::protocol(
            kind,
            format!("{} does not exist or is not a file", path.display()),
            format!("Set {env_key} to the vendored evolution path."),
            "principle-failure-surfaces-immediately",
        ))
    }
}

fn tail(value: &str, max_len: usize) -> String {
    let char_count = value.chars().count();
    if char_count <= max_len {
        return value.to_owned();
    }
    value.chars().skip(char_count - max_len).collect()
}

#[cfg(test)]
mod tests {
    use super::{frontmatter_value, slugify};

    #[test]
    fn scope_frontmatter_is_read() {
        let raw = "---\nscope: task-types/light-edit\n---\n\n# Skill\n";
        assert_eq!(
            frontmatter_value(raw, "scope").as_deref(),
            Some("task-types/light-edit"),
        );
    }

    #[test]
    fn slugify_normalizes_scopes() {
        assert_eq!(slugify("task-types/light_edit"), "task-types-light-edit");
    }
}
