//! The `jam` CLI binary — user-facing entry point.
//!
//! Per spec §11.4 + `comp-jam-cli-binary`. Phase 0 implements `setup` and
//! `doctor` against the [`jam-setup`] check set; remaining subcommands are
//! well-structured TODO stubs that name what they will do, who consumes
//! them, and which task in `graph/tasks/` tracks the work.

use chrono::{DateTime, SecondsFormat, Utc};
use clap::{Parser, Subcommand, ValueEnum};
use jam_events::generated::{
    Event, QuotaExhausted, QuotaExhaustedSoon, QuotaRefilled, TaskAbandoned, TaskRequested,
};
use jam_events::EventEnvelope;
use jam_nats::JamNats;
use jam_setup::{CheckSeverity, CheckStatus};
use jam_trace::{TraceCtx, TraceId};
use jam_ui_server::auth::TokenStore;
use jam_ui_server::trace_replay::{find_traces_in_journal, TraceFindResult};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use serde_yaml::{Mapping, Value as YamlValue};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Component, Path, PathBuf};
use std::process::{Command as ProcessCommand, ExitCode};
use std::time::Duration;

#[derive(Parser)]
#[command(
    name = "jam",
    version,
    about = "Jamboree orchestrator CLI",
    long_about = "Drives the multi-coding-agent orchestrator for Caleb's Bevy/Rust voxel game."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run preflight environment checks; refuses to install on failure.
    Setup,

    /// Same checks as `jam setup`; informational anytime.
    Doctor {
        /// Fetch and rebase the local jamboree checkout onto origin. Passes
        /// `--autostash` to git rebase, so uncommitted edits are stashed
        /// before rebase and re-applied afterward.
        #[arg(long)]
        auto_rebase: bool,
    },

    /// Spawn / list / show / cleanup tasks.
    Task {
        #[command(subcommand)]
        action: TaskAction,
    },

    /// Trace replay and search.
    Trace {
        #[command(subcommand)]
        action: TraceAction,
    },

    /// Quota inspection.
    Quota {
        #[command(subcommand)]
        action: QuotaAction,
    },

    /// Hot-patch a tool service.
    Patch {
        #[command(subcommand)]
        action: PatchAction,
    },

    /// Tool service health checks.
    Health {
        #[command(subcommand)]
        action: HealthAction,
    },

    /// UI session token management + lifecycle.
    Ui {
        #[command(subcommand)]
        action: UiAction,
    },

    /// Pause new spawns.
    PauseDispatch {
        /// One-line reason for pausing.
        #[arg(long)]
        reason: String,
        /// NATS URL (defaults to NATS_URL or nats://127.0.0.1:4222).
        #[arg(long)]
        nats_url: Option<String>,
    },

    /// Resume spawning after pause.
    ResumeDispatch {
        /// NATS URL (defaults to NATS_URL or nats://127.0.0.1:4222).
        #[arg(long)]
        nats_url: Option<String>,
    },

    /// Maestro session lifecycle (resume aborted, abandon aborted).
    Maestro {
        #[command(subcommand)]
        action: MaestroAction,
    },

    /// Tempyr canonical worktree management.
    Tempyr {
        #[command(subcommand)]
        action: TempyrAction,
    },

    /// Build, stage to maestro, and hot-patch a tool service.
    ///
    /// One-shot deploy from the caleb-side checkout: `cargo build --release`
    /// the service crate (unless `--from` is given), copy the artifact across
    /// the user boundary into `~maestro/.jam/staging/`, then invoke
    /// `jam patch apply` as maestro to swap the routing manifest. Relies on
    /// the NOPASSWD `caleb -> maestro` sudoers rule from `security-setup.md`.
    Deploy {
        /// Tool service names. Each must match a `jam-svc-<service>` crate, or
        /// `maestro` for the Python orchestrator app. Multiple names deploy in
        /// the given order. With `--dirty` and no names, services are inferred
        /// from `git status` (paths under `crates/jam-svc-*/` and `maestro/`).
        #[arg(num_args = 0..)]
        services: Vec<String>,
        /// Deploy every component whose source has uncommitted changes in the
        /// working tree. Mutually exclusive with explicit service names.
        #[arg(long, conflicts_with_all = ["version", "from"])]
        dirty: bool,
        /// Version override. Defaults to workspace version, with `-<short-sha>-dirty`
        /// suffix when the working tree has uncommitted changes. Single-service only;
        /// not supported for `maestro`.
        #[arg(long)]
        version: Option<String>,
        /// Pre-built binary path. Skips the `cargo build` step. Single-service only;
        /// not supported for `maestro`.
        #[arg(long)]
        from: Option<PathBuf>,
        /// NATS URL forwarded to `jam patch apply`.
        #[arg(long)]
        nats_url: Option<String>,
    },
}

#[derive(Subcommand)]
enum TaskAction {
    /// Spawn a new task.
    Spawn {
        /// Task description.
        description: String,
        /// Project (defaults to "blueberry").
        #[arg(long, default_value = "blueberry")]
        project: String,
        /// Task class (e.g. light-edit, compile-heavy-rust, ecs-refactor).
        #[arg(long)]
        task_class: Option<String>,
        /// Priority (low | normal | high).
        #[arg(long, default_value = "normal")]
        priority: String,
        /// NATS URL (defaults to NATS_URL or nats://127.0.0.1:4222).
        #[arg(long)]
        nats_url: Option<String>,
    },
    /// List active tasks.
    List,
    /// Show task detail.
    Show {
        /// Task ID.
        task_id: String,
    },
    /// Mark a stale task abandoned.
    Abandon {
        /// Task ID.
        task_id: String,
        /// Reason recorded in the lifecycle journal.
        #[arg(long)]
        reason: Option<String>,
        /// NATS URL (defaults to NATS_URL or nats://127.0.0.1:4222).
        #[arg(long)]
        nats_url: Option<String>,
    },
    /// Clean up orphaned worktrees.
    Cleanup,
}

#[derive(Subcommand)]
enum TraceAction {
    /// Replay a trace from durable storage.
    Replay {
        /// Trace ID (ULID).
        trace_id: String,
        /// Maximum parent-trace depth to walk (default 5).
        #[arg(long, default_value_t = 5)]
        max_depth: u32,
    },
    /// Find traces matching a filter.
    Find {
        /// Filter expression (e.g. `harness=codex-cli AND outcome=failed AND since=last-7d`).
        filter: String,
        /// Maximum matching traces to print.
        #[arg(long, default_value_t = 25)]
        limit: usize,
    },
}

#[derive(Subcommand)]
enum QuotaAction {
    /// Show current quota state across all harnesses.
    Show {
        /// Harness ID or harness/window key to filter, e.g. codex-cli or codex-cli/local-messages.
        #[arg(long)]
        harness_id: Option<String>,
        /// NATS URL (defaults to NATS_URL or nats://127.0.0.1:4222).
        #[arg(long)]
        nats_url: Option<String>,
        /// Timeout in seconds.
        #[arg(long, default_value_t = 5)]
        timeout_secs: u64,
    },
    /// Manually publish an observed quota state correction.
    Recalibrate {
        /// Harness ID, e.g. codex-cli, claude-code, opencode-deepseek.
        #[arg(long)]
        harness: String,
        /// Window kind, e.g. local-messages, cloud-tasks, code-reviews, rate-limit, api-budget.
        #[arg(long)]
        window_kind: String,
        /// Corrected state to publish.
        #[arg(long, value_enum)]
        status: QuotaRecalibrateStatus,
        /// Remaining fraction for low quota states, e.g. 0.08 for 8%.
        #[arg(long)]
        remaining: Option<f64>,
        /// Reset time for exhausted subscription windows, as RFC 3339 UTC.
        #[arg(long)]
        resets_at: Option<String>,
        /// NATS URL (defaults to NATS_URL or nats://127.0.0.1:4222).
        #[arg(long)]
        nats_url: Option<String>,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum QuotaRecalibrateStatus {
    /// Publish quota.refilled.
    Available,
    /// Publish quota.exhausted.
    Exhausted,
    /// Publish quota.exhausted-soon.
    Low,
}

#[derive(Subcommand)]
enum PatchAction {
    /// Publish patch.staged for a staged binary; the patch-agent does §20.3.
    Apply {
        /// Tool service name (e.g. observe, session).
        service: String,
        /// New version string.
        version: String,
        /// Path to the binary the agent should install. Defaults to
        /// `<jam_home>/staging/jam-svc-<service>-<version>`.
        #[arg(long)]
        staging_path: Option<PathBuf>,
        /// NATS URL (defaults to NATS_URL or nats://127.0.0.1:4222).
        #[arg(long)]
        nats_url: Option<String>,
        /// Maximum time to wait for the patch-agent to emit a terminal event.
        #[arg(long, default_value_t = 60)]
        timeout_secs: u64,
    },
    /// Publish patch.rollback-requested; the patch-agent does §20.4.
    Rollback {
        /// Tool service name (e.g. observe, session).
        service: String,
        /// Reason recorded in the patch.rolled-back event.
        #[arg(long, default_value = "manual rollback")]
        reason: String,
        /// NATS URL (defaults to NATS_URL or nats://127.0.0.1:4222).
        #[arg(long)]
        nats_url: Option<String>,
        /// Maximum time to wait for the patch-agent to emit a terminal event.
        #[arg(long, default_value_t = 60)]
        timeout_secs: u64,
    },
}

#[derive(Subcommand)]
enum HealthAction {
    /// Ping a tool service and require an ok response within the timeout.
    Ping {
        /// Tool service name (e.g. observe, session, worktree, repo).
        service: String,
        /// Explicit subject override, e.g. tool.observe.ping.v047.
        #[arg(long)]
        subject: Option<String>,
        /// NATS URL (defaults to NATS_URL or nats://127.0.0.1:4222).
        #[arg(long)]
        nats_url: Option<String>,
        /// Timeout in seconds.
        #[arg(long, default_value_t = 5)]
        timeout_secs: u64,
    },
}

#[derive(Subcommand)]
enum UiAction {
    /// Issue a new session token.
    Token {
        /// User id attributed to UI actions made with this token.
        #[arg(long, default_value = "human:caleb")]
        user_id: String,
    },
    /// Revoke a token by ID.
    TokenRevoke {
        /// Token ID.
        id: String,
    },
    /// Revoke all tokens.
    TokenRevokeAll,
}

#[derive(Subcommand)]
enum MaestroAction {
    /// Resume an aborted Maestro session with a fresh budget.
    Resume {
        /// Session ID.
        session_id: String,
        /// Budget extension in USD.
        #[arg(long)]
        budget_extension: f64,
    },
    /// Discard an aborted session.
    Abandon {
        /// Session ID.
        session_id: String,
    },
}

#[derive(Subcommand)]
enum TempyrAction {
    /// Manage the canonical Tempyr worktree.
    #[command(name = "canonical-worktree")]
    CanonicalWorktree {
        #[command(subcommand)]
        action: CanonicalWorktreeAction,
    },
}

#[derive(Subcommand)]
enum CanonicalWorktreeAction {
    /// Remove, recreate, and replay derived task state.
    Recreate,
    /// Reapply task lifecycle journal events without recreating the worktree.
    ReplayTasks,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Setup => run_setup(),
        Command::Doctor { auto_rebase } => run_doctor(auto_rebase),
        Command::Task { action } => run_task(action),
        Command::Trace { action } => run_trace(action),
        Command::Quota { action } => run_quota(action),
        Command::Patch { action } => run_patch(action),
        Command::Health { action } => run_health(action),
        Command::Ui { action } => run_ui(action),
        Command::PauseDispatch { reason, nats_url } => run_pause_dispatch(reason, nats_url),
        Command::ResumeDispatch { nats_url } => run_resume_dispatch(nats_url),
        Command::Maestro { action } => run_maestro(action),
        Command::Tempyr { action } => run_tempyr(action),
        Command::Deploy {
            services,
            dirty,
            version,
            from,
            nats_url,
        } => run_deploy(services, dirty, version, from, nats_url),
    }
}

fn run_setup() -> ExitCode {
    print_header("jam setup — preflight checks");
    let status = print_run_outcomes(true);
    if status != ExitCode::SUCCESS {
        return status;
    }
    match CanonicalWorktreeConfig::from_env().and_then(|config| ensure_canonical_worktree(&config))
    {
        Ok(outcome) => {
            eprintln!("  \x1b[32m✓\x1b[0m {}", outcome.summary());
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("  \x1b[31m✗\x1b[0m canonical Tempyr worktree setup failed: {err}");
            ExitCode::from(1)
        }
    }
}

fn run_doctor(auto_rebase: bool) -> ExitCode {
    if auto_rebase {
        if let Err(err) = run_jamboree_auto_rebase() {
            eprintln!("\x1b[33m!\x1b[0m auto-rebase: {err}");
        }
    }
    print_header("jam doctor — environment health");
    print_run_outcomes(false)
}

/// Fetch and rebase `/home/caleb/jamboree` onto origin so the local tree
/// catches up with auto-merged PRs. Idempotent — no-op when the tree is
/// already current. Uses `--autostash` to keep dirty edits intact.
///
/// This is the explicit-opt-in counterpart to `JamboreeCheckoutFreshCheck`'s
/// warning: the check tells you you're behind; this fixes it.
fn run_jamboree_auto_rebase() -> Result<(), String> {
    let repo = "/home/caleb/jamboree";
    eprintln!("auto-rebase: fetching origin in {repo}…");
    let fetch = ProcessCommand::new("git")
        .args(["-C", repo, "fetch", "--prune", "--tags"])
        .status()
        .map_err(|err| format!("git fetch: {err}"))?;
    if !fetch.success() {
        return Err(format!("git fetch exited {}", fetch.code().unwrap_or(-1)));
    }
    let rebase = ProcessCommand::new("git")
        .args(["-C", repo, "rebase", "--autostash", "@{u}"])
        .status()
        .map_err(|err| format!("git rebase: {err}"))?;
    if !rebase.success() {
        return Err(format!(
            "git rebase --autostash @{{u}} exited {}; resolve manually",
            rebase.code().unwrap_or(-1)
        ));
    }
    eprintln!("auto-rebase: done");
    Ok(())
}

fn print_header(title: &str) {
    eprintln!();
    eprintln!("\x1b[1m{title}\x1b[0m");
    eprintln!();
}

fn print_run_outcomes(gating: bool) -> ExitCode {
    let outcomes = jam_setup::run_all_checks();
    let mut required_failures = 0u32;
    let mut warnings = 0u32;
    let mut skips = 0u32;
    let mut passes = 0u32;

    for outcome in &outcomes {
        let glyph = match outcome.status {
            CheckStatus::Pass => "\x1b[32m✓\x1b[0m",
            CheckStatus::Warn => "\x1b[33m!\x1b[0m",
            CheckStatus::Fail => "\x1b[31m✗\x1b[0m",
            CheckStatus::Skip => "\x1b[36m∼\x1b[0m",
        };
        eprintln!("  {glyph} {} — {}", outcome.id, outcome.summary);
        if let Some(remediation) = &outcome.remediation {
            for line in remediation.lines() {
                eprintln!("      {line}");
            }
        }

        match (outcome.status, outcome.severity) {
            (CheckStatus::Pass, _) => passes += 1,
            (CheckStatus::Fail, CheckSeverity::Required) => required_failures += 1,
            (CheckStatus::Warn | CheckStatus::Fail, _) => warnings += 1,
            (CheckStatus::Skip, _) => skips += 1,
        }
    }

    eprintln!();
    eprintln!(
        "  \x1b[1msummary\x1b[0m: {passes} passed, {warnings} warnings, {skips} deferred, {required_failures} required failures"
    );
    eprintln!();

    if gating && required_failures > 0 {
        eprintln!(
            "\x1b[31msetup aborted\x1b[0m: {required_failures} required check(s) failed. \
             Re-run after resolving."
        );
        ExitCode::from(1)
    } else if !gating && required_failures > 0 {
        eprintln!(
            "\x1b[31m{required_failures} required check(s) failing — orchestrator may not function correctly.\x1b[0m"
        );
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

fn run_task(action: TaskAction) -> ExitCode {
    let result = match action {
        TaskAction::Spawn {
            description,
            project,
            task_class,
            priority,
            nats_url,
        } => run_task_spawn(description, project, task_class, priority, nats_url),
        TaskAction::List => run_task_list(),
        TaskAction::Show { task_id } => run_task_show(&task_id),
        TaskAction::Abandon {
            task_id,
            reason,
            nats_url,
        } => run_task_abandon(task_id, reason, nats_url),
        TaskAction::Cleanup => {
            Err("jam task cleanup is tracked by task-cli-task-spawn-list-show".into())
        }
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("jam task failed: {err}");
            ExitCode::from(1)
        }
    }
}

fn run_task_spawn(
    description: String,
    project: String,
    task_class: Option<String>,
    priority: String,
    nats_url: Option<String>,
) -> Result<(), String> {
    let task_class = task_class.unwrap_or_else(|| "light-edit".into());
    let requested_by = format!("human:{}", current_user());
    let trace_ctx = TraceCtx::new_root(
        "cli.task.spawn",
        format!("user spawned task: {description}"),
    );
    let task_id = task_id_for(&description, &trace_ctx);
    let payload = TaskRequested {
        task_id: task_id.clone(),
        description,
        project,
        task_class,
        priority,
        requested_by: requested_by.clone(),
    };
    let envelope = EventEnvelope::new(
        TaskRequested::EVENT_TYPE,
        TaskRequested::EVENT_SUBTYPE_VERSION,
        0,
        trace_ctx.trace_id.to_string(),
        requested_by,
        payload,
    );
    let nats_url = nats_url.unwrap_or_else(default_nats_url);
    let nats_token = resolve_nats_token();
    let trace_id = trace_ctx.trace_id.to_string();
    let publish_trace_ctx = trace_ctx.clone();

    run_async(async move {
        let nats = JamNats::connect(&nats_url, nats_token)
            .await
            .map_err(|err| format!("connect {nats_url}: {err}"))?;
        ensure_dispatch_state_bucket(&nats).await?;
        if let Some(paused) = dispatch_pause_status(&nats).await? {
            if paused.dispatch_paused {
                let reason = paused.reason.as_deref().unwrap_or("no reason recorded");
                return Err(format!(
                    "dispatch paused since {} by {}: {reason}",
                    paused.changed_at.to_rfc3339_opts(SecondsFormat::Secs, true),
                    paused.changed_by,
                ));
            }
        }
        nats.publish_traced("journal.task.requested", &envelope, &publish_trace_ctx)
            .await
            .map_err(|err| format!("publish journal.task.requested: {err}"))?;
        Ok::<(), String>(())
    })?;

    println!("task_id: {task_id}");
    println!("trace_id: {trace_id}");
    Ok(())
}

fn run_task_abandon(
    task_id: String,
    reason: Option<String>,
    nats_url: Option<String>,
) -> Result<(), String> {
    validate_task_id(&task_id)?;
    let reason = reason.unwrap_or_else(|| "stale task has no running Picker process".into());
    if reason.trim().is_empty() {
        return Err("reason must not be empty".into());
    }
    let requested_by = format!("human:{}", current_user());
    let trace_ctx = TraceCtx::new_root(
        "cli.task.abandon",
        format!("user abandoned task: {task_id}"),
    );
    let abandoned_at = Utc::now();
    let payload = TaskAbandoned {
        task_id: task_id.clone(),
        reason: reason.clone(),
        abandoned_at,
    };
    let envelope = EventEnvelope::new(
        TaskAbandoned::EVENT_TYPE,
        TaskAbandoned::EVENT_SUBTYPE_VERSION,
        0,
        trace_ctx.trace_id.to_string(),
        requested_by,
        payload,
    );
    let nats_url = nats_url.unwrap_or_else(default_nats_url);
    let nats_token = resolve_nats_token();
    let trace_id = trace_ctx.trace_id.to_string();
    let publish_trace_ctx = trace_ctx.clone();

    run_async(async move {
        let nats = JamNats::connect(&nats_url, nats_token)
            .await
            .map_err(|err| format!("connect {nats_url}: {err}"))?;
        nats.publish_traced("journal.task.abandoned", &envelope, &publish_trace_ctx)
            .await
            .map_err(|err| format!("publish journal.task.abandoned: {err}"))?;
        Ok::<(), String>(())
    })?;

    println!("task_id: {task_id}");
    println!("status: abandoned");
    println!("reason: {reason}");
    println!("trace_id: {trace_id}");
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MaestroResumeRequest {
    schema_version: u32,
    session_id: String,
    requested_at: chrono::DateTime<Utc>,
    requested_by: String,
    budget_extension_usd: f64,
    dump_path: String,
    dump: JsonValue,
}

struct MaestroResumeOutcome {
    request_path: PathBuf,
    request: MaestroResumeRequest,
}

struct MaestroAbandonOutcome {
    dump_path: PathBuf,
    resume_request_removed: bool,
}

fn run_maestro(action: MaestroAction) -> ExitCode {
    let result = match action {
        MaestroAction::Resume {
            session_id,
            budget_extension,
        } => resume_maestro_session(&session_id, budget_extension).map(|outcome| {
            println!("session_id: {}", outcome.request.session_id);
            println!(
                "budget_extension_usd: {:.2}",
                outcome.request.budget_extension_usd
            );
            println!("resume_request: {}", outcome.request_path.display());
        }),
        MaestroAction::Abandon { session_id } => {
            abandon_maestro_session(&session_id).map(|outcome| {
                println!("session_id: {session_id}");
                println!("removed_abort_dump: {}", outcome.dump_path.display());
                println!("removed_resume_request: {}", outcome.resume_request_removed);
            })
        }
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("jam maestro failed: {err}");
            ExitCode::from(1)
        }
    }
}

fn resume_maestro_session(
    session_id: &str,
    budget_extension: f64,
) -> Result<MaestroResumeOutcome, String> {
    resume_maestro_session_in(&jam_home(), session_id, budget_extension)
}

fn resume_maestro_session_in(
    root: &Path,
    session_id: &str,
    budget_extension: f64,
) -> Result<MaestroResumeOutcome, String> {
    validate_maestro_session_id(session_id)?;
    if !budget_extension.is_finite() || budget_extension <= 0.0 {
        return Err("budget extension must be a positive finite USD amount".into());
    }

    let dump_path = aborted_maestro_session_path_in(root, session_id)?;
    let dump = read_json_file(&dump_path)?;
    validate_abort_dump_session(&dump, session_id, &dump_path)?;

    let request = MaestroResumeRequest {
        schema_version: 1,
        session_id: session_id.to_owned(),
        requested_at: Utc::now(),
        requested_by: format!("human:{}", current_user()),
        budget_extension_usd: budget_extension,
        dump_path: dump_path.display().to_string(),
        dump,
    };
    let request_path = maestro_resume_request_path_in(root, session_id)?;
    write_json_atomic(&request_path, &request)?;
    Ok(MaestroResumeOutcome {
        request_path,
        request,
    })
}

fn abandon_maestro_session(session_id: &str) -> Result<MaestroAbandonOutcome, String> {
    abandon_maestro_session_in(&jam_home(), session_id)
}

fn abandon_maestro_session_in(
    root: &Path,
    session_id: &str,
) -> Result<MaestroAbandonOutcome, String> {
    validate_maestro_session_id(session_id)?;
    let dump_path = aborted_maestro_session_path_in(root, session_id)?;
    if !dump_path.exists() {
        return Err(format!(
            "aborted session dump not found: {}",
            dump_path.display()
        ));
    }
    fs::remove_file(&dump_path).map_err(|err| format!("remove {}: {err}", dump_path.display()))?;

    let request_path = maestro_resume_request_path_in(root, session_id)?;
    let resume_request_removed = if request_path.exists() {
        fs::remove_file(&request_path)
            .map_err(|err| format!("remove {}: {err}", request_path.display()))?;
        true
    } else {
        false
    };

    Ok(MaestroAbandonOutcome {
        dump_path,
        resume_request_removed,
    })
}

const DISPATCH_STATE_BUCKET: &str = "dispatch-state";
const DISPATCH_PAUSED_KEY: &str = "dispatch-paused";
const DISPATCH_STATE_KEY: &str = "state";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct DispatchPauseRecord {
    dispatch_paused: bool,
    reason: Option<String>,
    changed_at: chrono::DateTime<Utc>,
    changed_by: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct HealthPingResult {
    subject: String,
    service: String,
    version: String,
    status: String,
}

fn run_patch(action: PatchAction) -> ExitCode {
    let result = match action {
        PatchAction::Apply {
            service,
            version,
            staging_path,
            nats_url,
            timeout_secs,
        } => run_patch_apply(service, version, staging_path, nats_url, timeout_secs),
        PatchAction::Rollback {
            service,
            reason,
            nats_url,
            timeout_secs,
        } => run_patch_rollback(service, reason, nats_url, timeout_secs),
    };

    match result {
        Ok(report) => {
            println!("outcome: {}", report.outcome.as_str());
            println!("service: {}", report.service);
            if !report.detail.is_empty() {
                println!("detail: {}", report.detail);
            }
            if report.outcome.is_success() {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(err) => {
            eprintln!("jam patch failed: {err}");
            ExitCode::from(1)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PatchOutcome {
    Confirmed,
    Failed,
    RolledBack,
    RolledBackSuccessfully,
}

impl PatchOutcome {
    fn as_str(self) -> &'static str {
        match self {
            PatchOutcome::Confirmed => "confirmed",
            PatchOutcome::Failed => "failed",
            PatchOutcome::RolledBack => "rolled-back",
            PatchOutcome::RolledBackSuccessfully => "rolled-back-successfully",
        }
    }

    fn is_success(self) -> bool {
        !matches!(self, PatchOutcome::Failed)
    }
}

#[derive(Debug, Clone)]
struct PatchTerminalReport {
    outcome: PatchOutcome,
    service: String,
    detail: String,
}

fn run_health(action: HealthAction) -> ExitCode {
    let result = match action {
        HealthAction::Ping {
            service,
            subject,
            nats_url,
            timeout_secs,
        } => run_health_ping(service, subject, nats_url, timeout_secs),
    };

    match result {
        Ok(result) => {
            println!("subject: {}", result.subject);
            println!("service: {}", result.service);
            println!("version: {}", result.version);
            println!("status: {}", result.status);
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("jam health ping failed: {err}");
            ExitCode::from(1)
        }
    }
}

fn run_health_ping(
    service: String,
    subject: Option<String>,
    nats_url: Option<String>,
    timeout_secs: u64,
) -> Result<HealthPingResult, String> {
    validate_service_arg(&service)?;
    if timeout_secs == 0 {
        return Err("--timeout-secs must be greater than zero".into());
    }
    let subject = subject.unwrap_or_else(|| health_ping_subject(&service));
    validate_nats_subject(&subject)?;
    let nats_url = nats_url.unwrap_or_else(default_nats_url);
    let nats_token = resolve_nats_token();
    let trace_ctx = TraceCtx::new_root(
        "cli.health.ping",
        format!("health ping {service} via {subject}"),
    );
    run_async_value(health_ping_request(
        nats_url,
        nats_token,
        trace_ctx,
        service,
        subject,
        Duration::from_secs(timeout_secs),
    ))
}

async fn health_ping_request(
    nats_url: String,
    nats_token: Option<String>,
    trace_ctx: TraceCtx,
    service: String,
    subject: String,
    timeout: Duration,
) -> Result<HealthPingResult, String> {
    let nats = JamNats::connect(&nats_url, nats_token)
        .await
        .map_err(|err| format!("connect {nats_url}: {err}"))?;
    let response: serde_json::Value = nats
        .request_traced(&subject, &serde_json::json!({}), &trace_ctx, timeout)
        .await
        .map_err(|err| format!("request {subject}: {err}"))?;
    parse_health_ping_response(&service, &subject, &response)
}

fn parse_health_ping_response(
    requested_service: &str,
    subject: &str,
    response: &serde_json::Value,
) -> Result<HealthPingResult, String> {
    let status = response
        .get("status")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| format!("health response on {subject} is missing string status"))?;
    if status != "ok" {
        return Err(format!(
            "health response on {subject} returned non-ok status: {status}"
        ));
    }
    let service = response
        .get("service")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| format!("health response on {subject} is missing string service"))?;
    let expected = health_service_name(requested_service);
    if service != expected {
        return Err(format!(
            "health response on {subject} came from {service}, expected {expected}"
        ));
    }
    let version = response
        .get("version")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| format!("health response on {subject} is missing string version"))?;
    Ok(HealthPingResult {
        subject: subject.into(),
        service: service.into(),
        version: version.into(),
        status: status.into(),
    })
}

struct PatchPublishContext {
    nats_url: String,
    nats_token: Option<String>,
    trace_ctx: TraceCtx,
    actor: String,
    service: String,
    timeout: Duration,
}

fn run_patch_apply(
    service: String,
    version: String,
    staging_path: Option<PathBuf>,
    nats_url: Option<String>,
    timeout_secs: u64,
) -> Result<PatchTerminalReport, String> {
    validate_service_arg(&service)?;
    validate_version_arg(&version)?;
    if timeout_secs == 0 {
        return Err("--timeout-secs must be greater than zero".into());
    }
    let staging_path = staging_path.unwrap_or_else(|| {
        let basename = jam_tools_core::deploy_targets::binary_name(&service)
            .map(str::to_owned)
            .unwrap_or_else(|| format!("jam-svc-{service}"));
        jam_home()
            .join("staging")
            .join(format!("{basename}-{version}"))
    });
    let ctx = PatchPublishContext {
        nats_url: nats_url.unwrap_or_else(default_nats_url),
        nats_token: resolve_nats_token(),
        trace_ctx: TraceCtx::new_root(
            "cli.patch.apply",
            format!("apply {service} tool service version {version}"),
        ),
        actor: format!("human:{}", current_user()),
        service,
        timeout: Duration::from_secs(timeout_secs),
    };
    run_async_value(publish_apply_and_wait(ctx, version, staging_path))
}

/// Publish patch.staged for a PythonApp deploy. The "staged" entity is a
/// source directory, not a binary file — so we skip the executable+sha256
/// checks and pass an empty sha. Patch-agent's PythonApp handler rsyncs the
/// directory and runs `uv pip install`.
fn run_patch_apply_python_app(
    service: String,
    version: String,
    source_dir: PathBuf,
    nats_url: Option<String>,
    timeout_secs: u64,
) -> Result<PatchTerminalReport, String> {
    validate_service_arg(&service)?;
    validate_version_arg(&version)?;
    let ctx = PatchPublishContext {
        nats_url: nats_url.unwrap_or_else(default_nats_url),
        nats_token: resolve_nats_token(),
        trace_ctx: TraceCtx::new_root(
            "cli.patch.apply",
            format!("apply {service} python app version {version}"),
        ),
        actor: format!("human:{}", current_user()),
        service,
        timeout: Duration::from_secs(timeout_secs),
    };
    run_async_value(publish_python_apply_and_wait(ctx, version, source_dir))
}

async fn publish_python_apply_and_wait(
    ctx: PatchPublishContext,
    version: String,
    source_dir: PathBuf,
) -> Result<PatchTerminalReport, String> {
    use jam_events::generated::{
        PatchConfirmed, PatchFailed, PatchRolledBackSuccessfully, PatchStaged,
    };
    if !source_dir.is_dir() {
        return Err(format!(
            "source path is not a dir: {}",
            source_dir.display()
        ));
    }
    let nats = JamNats::connect(&ctx.nats_url, ctx.nats_token.clone())
        .await
        .map_err(|err| format!("connect {}: {err}", ctx.nats_url))?;
    let (confirmed_sub, failed_sub, rb_success_sub) = tokio::try_join!(
        async {
            nats.client()
                .subscribe(PatchConfirmed::EVENT_TYPE)
                .await
                .map_err(|err| format!("subscribe patch.confirmed: {err}"))
        },
        async {
            nats.client()
                .subscribe(PatchFailed::EVENT_TYPE)
                .await
                .map_err(|err| format!("subscribe patch.failed: {err}"))
        },
        async {
            nats.client()
                .subscribe(PatchRolledBackSuccessfully::EVENT_TYPE)
                .await
                .map_err(|err| format!("subscribe patch.rolled-back-successfully: {err}"))
        },
    )?;
    let staged = PatchStaged {
        service: ctx.service.clone(),
        version: version.clone(),
        staging_path: source_dir.display().to_string(),
        // No binary to hash. Patch-agent's PythonApp handler doesn't check this
        // field, but the schema requires it — send a sentinel rather than risk
        // a future binary-flow consumer treating an empty string as zero-byte.
        binary_sha256: "python-app-no-binary-hash".to_owned(),
        requested_by: ctx.actor.clone(),
        ts: Utc::now(),
    };
    let envelope = EventEnvelope::new(
        PatchStaged::EVENT_TYPE,
        PatchStaged::EVENT_SUBTYPE_VERSION,
        0,
        ctx.trace_ctx.trace_id.to_string(),
        ctx.actor.clone(),
        staged,
    );
    nats.publish_traced(PatchStaged::EVENT_TYPE, &envelope, &ctx.trace_ctx)
        .await
        .map_err(|err| format!("publish patch.staged: {err}"))?;
    eprintln!(
        "patch.staged published; waiting up to {}s for the agent to finish",
        ctx.timeout.as_secs()
    );
    wait_for_apply_terminal(
        confirmed_sub,
        failed_sub,
        rb_success_sub,
        &ctx.trace_ctx.trace_id.to_string(),
        &ctx.service,
        ctx.timeout,
    )
    .await
}

fn run_patch_rollback(
    service: String,
    reason: String,
    nats_url: Option<String>,
    timeout_secs: u64,
) -> Result<PatchTerminalReport, String> {
    validate_service_arg(&service)?;
    if reason.trim().is_empty() {
        return Err("--reason must not be empty".into());
    }
    if timeout_secs == 0 {
        return Err("--timeout-secs must be greater than zero".into());
    }
    let ctx = PatchPublishContext {
        nats_url: nats_url.unwrap_or_else(default_nats_url),
        nats_token: resolve_nats_token(),
        trace_ctx: TraceCtx::new_root(
            "cli.patch.rollback",
            format!("roll back {service} tool service: {reason}"),
        ),
        actor: format!("human:{}", current_user()),
        service,
        timeout: Duration::from_secs(timeout_secs),
    };
    run_async_value(publish_rollback_and_wait(ctx, reason))
}

async fn publish_apply_and_wait(
    ctx: PatchPublishContext,
    version: String,
    staging_path: PathBuf,
) -> Result<PatchTerminalReport, String> {
    use jam_events::generated::{
        PatchConfirmed, PatchFailed, PatchRolledBackSuccessfully, PatchStaged,
    };
    if !staging_path.is_file() {
        return Err(format!(
            "staged binary missing: {}\nFix: build the service binary and place it at this path, or pass --staging-path",
            staging_path.display()
        ));
    }
    validate_executable_for_publish(&staging_path)?;
    let binary_sha256 = sha256_file_hex(&staging_path)?;

    let nats = JamNats::connect(&ctx.nats_url, ctx.nats_token.clone())
        .await
        .map_err(|err| format!("connect {}: {err}", ctx.nats_url))?;

    // Subscribe before publishing so we don't miss the terminal event.
    // patch.rolled-back is intermediate during apply: the agent emits it
    // during mechanical rollback, then post-rollback verify decides between
    // rolled-back-successfully and failed.
    let (confirmed_sub, failed_sub, rb_success_sub) = tokio::try_join!(
        async {
            nats.client()
                .subscribe(PatchConfirmed::EVENT_TYPE)
                .await
                .map_err(|err| format!("subscribe patch.confirmed: {err}"))
        },
        async {
            nats.client()
                .subscribe(PatchFailed::EVENT_TYPE)
                .await
                .map_err(|err| format!("subscribe patch.failed: {err}"))
        },
        async {
            nats.client()
                .subscribe(PatchRolledBackSuccessfully::EVENT_TYPE)
                .await
                .map_err(|err| format!("subscribe patch.rolled-back-successfully: {err}"))
        },
    )?;

    let staged = PatchStaged {
        service: ctx.service.clone(),
        version,
        staging_path: staging_path.display().to_string(),
        binary_sha256,
        requested_by: ctx.actor.clone(),
        ts: Utc::now(),
    };
    let envelope = EventEnvelope::new(
        PatchStaged::EVENT_TYPE,
        PatchStaged::EVENT_SUBTYPE_VERSION,
        0,
        ctx.trace_ctx.trace_id.to_string(),
        ctx.actor.clone(),
        staged,
    );
    nats.publish_traced(PatchStaged::EVENT_TYPE, &envelope, &ctx.trace_ctx)
        .await
        .map_err(|err| format!("publish patch.staged: {err}"))?;
    eprintln!(
        "patch.staged published; waiting up to {}s for the agent to finish",
        ctx.timeout.as_secs()
    );

    wait_for_apply_terminal(
        confirmed_sub,
        failed_sub,
        rb_success_sub,
        &ctx.trace_ctx.trace_id.to_string(),
        &ctx.service,
        ctx.timeout,
    )
    .await
}

async fn publish_rollback_and_wait(
    ctx: PatchPublishContext,
    reason: String,
) -> Result<PatchTerminalReport, String> {
    use jam_events::generated::{PatchFailed, PatchRollbackRequested, PatchRolledBack};

    let nats = JamNats::connect(&ctx.nats_url, ctx.nats_token.clone())
        .await
        .map_err(|err| format!("connect {}: {err}", ctx.nats_url))?;

    // Explicit rollback skips the post-rollback verify, so patch.rolled-back
    // is terminal here (unlike the apply path).
    let (failed_sub, rolled_back_sub) = tokio::try_join!(
        async {
            nats.client()
                .subscribe(PatchFailed::EVENT_TYPE)
                .await
                .map_err(|err| format!("subscribe patch.failed: {err}"))
        },
        async {
            nats.client()
                .subscribe(PatchRolledBack::EVENT_TYPE)
                .await
                .map_err(|err| format!("subscribe patch.rolled-back: {err}"))
        },
    )?;

    let payload = PatchRollbackRequested {
        service: ctx.service.clone(),
        reason,
        requested_by: ctx.actor.clone(),
        ts: Utc::now(),
    };
    let envelope = EventEnvelope::new(
        PatchRollbackRequested::EVENT_TYPE,
        PatchRollbackRequested::EVENT_SUBTYPE_VERSION,
        0,
        ctx.trace_ctx.trace_id.to_string(),
        ctx.actor.clone(),
        payload,
    );
    nats.publish_traced(
        PatchRollbackRequested::EVENT_TYPE,
        &envelope,
        &ctx.trace_ctx,
    )
    .await
    .map_err(|err| format!("publish patch.rollback-requested: {err}"))?;
    eprintln!(
        "patch.rollback-requested published; waiting up to {}s for the agent",
        ctx.timeout.as_secs()
    );

    wait_for_rollback_terminal(
        rolled_back_sub,
        failed_sub,
        &ctx.trace_ctx.trace_id.to_string(),
        &ctx.service,
        ctx.timeout,
    )
    .await
}

async fn wait_for_apply_terminal(
    mut confirmed_sub: jam_nats::async_nats::Subscriber,
    mut failed_sub: jam_nats::async_nats::Subscriber,
    mut rb_success_sub: jam_nats::async_nats::Subscriber,
    trace_id: &str,
    service: &str,
    timeout: Duration,
) -> Result<PatchTerminalReport, String> {
    use futures::StreamExt as _;
    use jam_events::generated::{PatchConfirmed, PatchFailed, PatchRolledBackSuccessfully};

    let deadline = tokio::time::sleep(timeout);
    tokio::pin!(deadline);

    loop {
        tokio::select! {
            () = &mut deadline => {
                return Err(format!(
                    "no terminal patch event for trace {trace_id} within {}s",
                    timeout.as_secs()
                ));
            }
            msg = confirmed_sub.next() => {
                let Some(msg) = msg else {
                    return Err("patch.confirmed subscription closed unexpectedly".into());
                };
                if let Some(env) = decode_envelope::<PatchConfirmed>(&msg.payload, trace_id, service, |p| &p.service) {
                    let detail = if env.payload.checks_run == 0 {
                        // patch-agent emits checks_run=0 when the requested
                        // version was already current (idempotent re-deploy).
                        format!("version {} already current (no-op)", env.payload.version)
                    } else {
                        format!(
                            "version {} confirmed by {} checks",
                            env.payload.version, env.payload.checks_run
                        )
                    };
                    return Ok(PatchTerminalReport {
                        outcome: PatchOutcome::Confirmed,
                        service: env.payload.service,
                        detail,
                    });
                }
            }
            msg = failed_sub.next() => {
                let Some(msg) = msg else {
                    return Err("patch.failed subscription closed unexpectedly".into());
                };
                if let Some(env) = decode_envelope::<PatchFailed>(&msg.payload, trace_id, service, |p| &p.service) {
                    return Ok(PatchTerminalReport {
                        outcome: PatchOutcome::Failed,
                        service: env.payload.service,
                        detail: format!("incident {} at {}: {}", env.payload.incident_id, env.payload.incident_dir, env.payload.summary),
                    });
                }
            }
            msg = rb_success_sub.next() => {
                let Some(msg) = msg else {
                    return Err("patch.rolled-back-successfully subscription closed unexpectedly".into());
                };
                if let Some(env) = decode_envelope::<PatchRolledBackSuccessfully>(&msg.payload, trace_id, service, |p| &p.service) {
                    return Ok(PatchTerminalReport {
                        outcome: PatchOutcome::RolledBackSuccessfully,
                        service: env.payload.service,
                        detail: format!("restored to version {}", env.payload.version),
                    });
                }
            }
        }
    }
}

async fn wait_for_rollback_terminal(
    mut rolled_back_sub: jam_nats::async_nats::Subscriber,
    mut failed_sub: jam_nats::async_nats::Subscriber,
    trace_id: &str,
    service: &str,
    timeout: Duration,
) -> Result<PatchTerminalReport, String> {
    use futures::StreamExt as _;
    use jam_events::generated::{PatchFailed, PatchRolledBack};

    let deadline = tokio::time::sleep(timeout);
    tokio::pin!(deadline);

    loop {
        tokio::select! {
            () = &mut deadline => {
                return Err(format!(
                    "no terminal patch event for trace {trace_id} within {}s",
                    timeout.as_secs()
                ));
            }
            msg = rolled_back_sub.next() => {
                let Some(msg) = msg else {
                    return Err("patch.rolled-back subscription closed unexpectedly".into());
                };
                if let Some(env) = decode_envelope::<PatchRolledBack>(&msg.payload, trace_id, service, |p| &p.service) {
                    return Ok(PatchTerminalReport {
                        outcome: PatchOutcome::RolledBack,
                        service: env.payload.service,
                        detail: format!(
                            "{} -> {}: {}",
                            env.payload.from_version, env.payload.to_version, env.payload.reason
                        ),
                    });
                }
            }
            msg = failed_sub.next() => {
                let Some(msg) = msg else {
                    return Err("patch.failed subscription closed unexpectedly".into());
                };
                if let Some(env) = decode_envelope::<PatchFailed>(&msg.payload, trace_id, service, |p| &p.service) {
                    return Ok(PatchTerminalReport {
                        outcome: PatchOutcome::Failed,
                        service: env.payload.service,
                        detail: format!("incident {} at {}: {}", env.payload.incident_id, env.payload.incident_dir, env.payload.summary),
                    });
                }
            }
        }
    }
}

fn decode_envelope<P>(
    payload: &[u8],
    trace_id: &str,
    service: &str,
    service_of: impl Fn(&P) -> &str,
) -> Option<EventEnvelope<P>>
where
    P: jam_events::generated::Event + serde::de::DeserializeOwned,
{
    let envelope: EventEnvelope<P> = serde_json::from_slice(payload).ok()?;
    if envelope.trace_id != trace_id || service_of(&envelope.payload) != service {
        return None;
    }
    Some(envelope)
}

fn validate_executable_for_publish(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        let mode = fs::metadata(path)
            .map_err(|err| format!("stat {}: {err}", path.display()))?
            .permissions()
            .mode();
        if mode & 0o111 == 0 {
            return Err(format!(
                "staged binary is not executable: {}\nFix: chmod +x the staged file before publishing patch.staged.",
                path.display()
            ));
        }
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

use jam_tools_core::hashing::sha256_file_hex;

fn validate_service_arg(service: &str) -> Result<(), String> {
    validate_segment(service, "service", |ch| {
        ch.is_ascii_alphanumeric() || ch == '-'
    })
}

fn validate_version_arg(version: &str) -> Result<(), String> {
    validate_segment(version, "version", |ch| {
        ch.is_ascii_alphanumeric() || ch == '.' || ch == '_' || ch == '-'
    })
}

fn health_ping_subject(service: &str) -> String {
    format!("tool.{service}.ping")
}

fn health_service_name(service: &str) -> String {
    jam_tools_core::deploy_targets::service_id(service)
        .map(str::to_owned)
        .unwrap_or_else(|| format!("jam-svc-{service}"))
}

fn validate_nats_subject(subject: &str) -> Result<(), String> {
    validate_segment(subject, "subject", |ch| {
        ch.is_ascii_alphanumeric() || ch == '.' || ch == '_' || ch == '-'
    })
}

fn validate_segment(
    value: &str,
    label: &str,
    allowed: impl Fn(char) -> bool,
) -> Result<(), String> {
    if value.is_empty() {
        return Err(format!("{label} must not be empty"));
    }
    if value.chars().all(allowed) {
        Ok(())
    } else {
        Err(format!(
            "{label} contains invalid characters; use ASCII letters, numbers, dots, underscores, or hyphens as appropriate"
        ))
    }
}

fn run_pause_dispatch(reason: String, nats_url: Option<String>) -> ExitCode {
    if reason.trim().is_empty() {
        eprintln!("jam pause-dispatch failed: --reason must not be empty");
        return ExitCode::from(1);
    }
    run_dispatch_toggle(true, Some(reason), nats_url)
}

fn run_resume_dispatch(nats_url: Option<String>) -> ExitCode {
    run_dispatch_toggle(false, None, nats_url)
}

fn run_dispatch_toggle(
    dispatch_paused: bool,
    reason: Option<String>,
    nats_url: Option<String>,
) -> ExitCode {
    let nats_url = nats_url.unwrap_or_else(default_nats_url);
    let changed_by = format!("human:{}", current_user());
    let result = run_async_value(set_dispatch_pause_state(
        nats_url.clone(),
        resolve_nats_token(),
        dispatch_paused,
        reason,
        changed_by,
    ));

    match result {
        Ok(record) => {
            println!("dispatch_paused: {}", record.dispatch_paused);
            if let Some(reason) = record.reason {
                println!("reason: {reason}");
            }
            println!(
                "changed_at: {}",
                record.changed_at.to_rfc3339_opts(SecondsFormat::Secs, true)
            );
            println!("changed_by: {}", record.changed_by);
            println!("nats_url: {nats_url}");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("jam dispatch state update failed: {err}");
            ExitCode::from(1)
        }
    }
}

async fn set_dispatch_pause_state(
    nats_url: String,
    nats_token: Option<String>,
    dispatch_paused: bool,
    reason: Option<String>,
    changed_by: String,
) -> Result<DispatchPauseRecord, String> {
    let nats = JamNats::connect(&nats_url, nats_token)
        .await
        .map_err(|err| format!("connect {nats_url}: {err}"))?;
    ensure_dispatch_state_bucket(&nats).await?;
    let record = DispatchPauseRecord {
        dispatch_paused,
        reason,
        changed_at: Utc::now(),
        changed_by,
    };
    write_dispatch_pause_record(&nats, &record).await?;
    Ok(record)
}

async fn ensure_dispatch_state_bucket(nats: &JamNats) -> Result<(), String> {
    jam_nats::ensure_kv_buckets(nats.jetstream(), &jam_nats::default_kv_buckets())
        .await
        .map_err(|err| format!("ensure {DISPATCH_STATE_BUCKET} KV bucket: {err}"))
}

async fn write_dispatch_pause_record(
    nats: &JamNats,
    record: &DispatchPauseRecord,
) -> Result<(), String> {
    let kv = nats
        .jetstream()
        .get_key_value(DISPATCH_STATE_BUCKET)
        .await
        .map_err(|err| format!("open {DISPATCH_STATE_BUCKET} KV bucket: {err}"))?;
    let paused = if record.dispatch_paused {
        "true"
    } else {
        "false"
    };
    kv.put(DISPATCH_PAUSED_KEY, paused.into())
        .await
        .map_err(|err| format!("write {DISPATCH_PAUSED_KEY}: {err}"))?;
    let state = serde_json::to_vec(record).map_err(|err| format!("serialize state: {err}"))?;
    kv.put(DISPATCH_STATE_KEY, state.into())
        .await
        .map_err(|err| format!("write {DISPATCH_STATE_KEY}: {err}"))?;
    Ok(())
}

async fn dispatch_pause_status(nats: &JamNats) -> Result<Option<DispatchPauseRecord>, String> {
    let kv = nats
        .jetstream()
        .get_key_value(DISPATCH_STATE_BUCKET)
        .await
        .map_err(|err| format!("open {DISPATCH_STATE_BUCKET} KV bucket: {err}"))?;
    let Some(paused) = kv
        .get(DISPATCH_PAUSED_KEY)
        .await
        .map_err(|err| format!("read {DISPATCH_PAUSED_KEY}: {err}"))?
    else {
        return Ok(None);
    };
    if parse_dispatch_paused(&paused)? {
        return Ok(Some(
            match kv
                .get(DISPATCH_STATE_KEY)
                .await
                .map_err(|err| format!("read {DISPATCH_STATE_KEY}: {err}"))?
            {
                Some(state) => serde_json::from_slice(&state)
                    .map_err(|err| format!("parse {DISPATCH_STATE_KEY}: {err}"))?,
                None => DispatchPauseRecord {
                    dispatch_paused: true,
                    reason: None,
                    changed_at: Utc::now(),
                    changed_by: "unknown".into(),
                },
            },
        ));
    }
    Ok(None)
}

fn parse_dispatch_paused(raw: &[u8]) -> Result<bool, String> {
    match std::str::from_utf8(raw).map(str::trim) {
        Ok("true") => Ok(true),
        Ok("false") => Ok(false),
        Ok(value) => Err(format!(
            "{DISPATCH_PAUSED_KEY} contains invalid bool: {value}"
        )),
        Err(err) => Err(format!("{DISPATCH_PAUSED_KEY} is not UTF-8: {err}")),
    }
}

fn run_task_list() -> Result<(), String> {
    let mut records = read_task_records()?;
    records.sort_by(|a, b| a.task_id.cmp(&b.task_id));
    if records.is_empty() {
        println!("no tasks found in {}", journal_root().display());
        return Ok(());
    }
    println!("task_id\tproject\ttask_class\tpriority\trequested_by\tdescription");
    for record in records {
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}",
            record.task_id,
            record.project,
            record.task_class,
            record.priority,
            record.requested_by,
            record.description
        );
    }
    Ok(())
}

fn run_task_show(task_id: &str) -> Result<(), String> {
    let records = read_task_records()?;
    let Some(record) = records.into_iter().find(|record| record.task_id == task_id) else {
        return Err(format!("task not found in journal: {task_id}"));
    };

    println!("task_id: {}", record.task_id);
    println!("trace_id: {}", record.trace_id);
    println!("timestamp: {}", record.timestamp);
    println!("project: {}", record.project);
    println!("task_class: {}", record.task_class);
    println!("priority: {}", record.priority);
    println!("requested_by: {}", record.requested_by);
    println!("description: {}", record.description);
    Ok(())
}

fn run_trace(action: TraceAction) -> ExitCode {
    let result = match action {
        TraceAction::Replay {
            trace_id,
            max_depth,
        } => run_trace_replay(&trace_id, max_depth),
        TraceAction::Find { filter, limit } => run_trace_find(&filter, limit),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("jam trace failed: {err}");
            ExitCode::from(1)
        }
    }
}

fn run_quota(action: QuotaAction) -> ExitCode {
    match action {
        QuotaAction::Show {
            harness_id,
            nats_url,
            timeout_secs,
        } => match run_quota_show(harness_id, nats_url, timeout_secs) {
            Ok(windows) => {
                print_quota_windows(&windows);
                ExitCode::SUCCESS
            }
            Err(err) => {
                eprintln!("jam quota show failed: {err}");
                ExitCode::from(1)
            }
        },
        QuotaAction::Recalibrate {
            harness,
            window_kind,
            status,
            remaining,
            resets_at,
            nats_url,
        } => match run_quota_recalibrate(
            &harness,
            &window_kind,
            status,
            remaining,
            resets_at,
            nats_url,
        ) {
            Ok(record) => {
                println!("subject: {}", record.subject);
                println!("event_type: {}", record.event_type);
                println!("harness: {}", record.harness);
                println!("window_kind: {}", record.window_kind);
                println!("trace_id: {}", record.trace_id);
                ExitCode::SUCCESS
            }
            Err(err) => {
                eprintln!("jam quota recalibrate failed: {err}");
                ExitCode::from(1)
            }
        },
    }
}

fn run_quota_show(
    harness_id: Option<String>,
    nats_url: Option<String>,
    timeout_secs: u64,
) -> Result<Vec<QuotaWindow>, String> {
    if timeout_secs == 0 {
        return Err("--timeout-secs must be greater than zero".into());
    }
    let harness_id = normalize_quota_filter(harness_id)?;
    let nats_url = nats_url.unwrap_or_else(default_nats_url);
    let nats_token = resolve_nats_token();
    let label = harness_id.as_deref().unwrap_or("all harnesses");
    let trace_ctx = TraceCtx::new_root("cli.quota.show", format!("quota show {label}"));
    run_async_value(query_quota_request(
        nats_url,
        nats_token,
        trace_ctx,
        harness_id,
        Duration::from_secs(timeout_secs),
    ))
}

fn run_quota_recalibrate(
    harness: &str,
    window_kind: &str,
    status: QuotaRecalibrateStatus,
    remaining: Option<f64>,
    resets_at: Option<String>,
    nats_url: Option<String>,
) -> Result<QuotaRecalibrationRecord, String> {
    let harness = normalize_required_quota_arg(harness, "--harness")?;
    let window_kind = normalize_required_quota_arg(window_kind, "--window-kind")?;
    let resets_at = parse_optional_datetime_arg(resets_at, "--resets-at")?;
    let payload = build_quota_recalibration_payload(
        harness.clone(),
        window_kind.clone(),
        status,
        remaining,
        resets_at,
        Utc::now(),
    )?;
    let nats_url = nats_url.unwrap_or_else(default_nats_url);
    let nats_token = resolve_nats_token();
    let actor = format!("human:{}", current_user());
    let trace_ctx = TraceCtx::new_root(
        "cli.quota.recalibrate",
        format!(
            "quota recalibrate {} {} as {}",
            harness,
            window_kind,
            payload.event_type()
        ),
    );
    run_async_value(publish_quota_recalibration(
        nats_url, nats_token, trace_ctx, actor, payload,
    ))
}

async fn query_quota_request(
    nats_url: String,
    nats_token: Option<String>,
    trace_ctx: TraceCtx,
    harness_id: Option<String>,
    timeout: Duration,
) -> Result<Vec<QuotaWindow>, String> {
    let nats = JamNats::connect(&nats_url, nats_token)
        .await
        .map_err(|err| format!("connect {nats_url}: {err}"))?;
    let subject = "tool.observe.query-quota";
    let payload = QueryQuotaRequest {
        harness_id: harness_id.clone(),
    };
    let response: JsonValue = nats
        .request_traced(subject, &payload, &trace_ctx, timeout)
        .await
        .map_err(|err| format!("request {subject}: {err}"))?;
    parse_query_quota_response(response, harness_id.as_deref(), subject)
}

async fn publish_quota_recalibration(
    nats_url: String,
    nats_token: Option<String>,
    trace_ctx: TraceCtx,
    actor: String,
    payload: QuotaRecalibrationPayload,
) -> Result<QuotaRecalibrationRecord, String> {
    match payload {
        QuotaRecalibrationPayload::Exhausted(payload) => {
            publish_quota_payload(nats_url, nats_token, trace_ctx, actor, payload).await
        }
        QuotaRecalibrationPayload::ExhaustedSoon(payload) => {
            publish_quota_payload(nats_url, nats_token, trace_ctx, actor, payload).await
        }
        QuotaRecalibrationPayload::Refilled(payload) => {
            publish_quota_payload(nats_url, nats_token, trace_ctx, actor, payload).await
        }
    }
}

async fn publish_quota_payload<P>(
    nats_url: String,
    nats_token: Option<String>,
    trace_ctx: TraceCtx,
    actor: String,
    payload: P,
) -> Result<QuotaRecalibrationRecord, String>
where
    P: Event + Serialize,
{
    let nats = JamNats::connect(&nats_url, nats_token)
        .await
        .map_err(|err| format!("connect {nats_url}: {err}"))?;
    let event_type = P::EVENT_TYPE;
    let subject = format!("journal.{event_type}");
    let trace_id = trace_ctx.trace_id.to_string();
    let harness = quota_payload_string(&payload, "harness")?;
    let window_kind = quota_payload_string(&payload, "window_kind")?;
    let envelope = EventEnvelope::new(
        event_type,
        P::EVENT_SUBTYPE_VERSION,
        0,
        trace_id.clone(),
        actor,
        payload,
    );
    nats.publish_traced(&subject, &envelope, &trace_ctx)
        .await
        .map_err(|err| format!("publish {subject}: {err}"))?;
    Ok(QuotaRecalibrationRecord {
        subject,
        event_type: event_type.into(),
        harness,
        window_kind,
        trace_id,
    })
}

fn quota_payload_string<P>(payload: &P, key: &str) -> Result<String, String>
where
    P: Serialize,
{
    serde_json::to_value(payload)
        .map_err(|err| format!("serialize quota payload: {err}"))?
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| format!("quota payload missing string {key}"))
}

fn build_quota_recalibration_payload(
    harness: String,
    window_kind: String,
    status: QuotaRecalibrateStatus,
    remaining: Option<f64>,
    resets_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> Result<QuotaRecalibrationPayload, String> {
    match status {
        QuotaRecalibrateStatus::Available => {
            if remaining.is_some() {
                return Err("--remaining is only valid with --status low".into());
            }
            if resets_at.is_some() {
                return Err("--resets-at is only valid with --status exhausted".into());
            }
            Ok(QuotaRecalibrationPayload::Refilled(QuotaRefilled {
                harness,
                window_kind,
                ts: now,
            }))
        }
        QuotaRecalibrateStatus::Exhausted => {
            if remaining.is_some() {
                return Err("--remaining is only valid with --status low".into());
            }
            Ok(QuotaRecalibrationPayload::Exhausted(QuotaExhausted {
                harness,
                window_kind,
                resets_at,
                detected_at: now,
            }))
        }
        QuotaRecalibrateStatus::Low => {
            if resets_at.is_some() {
                return Err("--resets-at is only valid with --status exhausted".into());
            }
            let remaining = remaining.ok_or_else(|| {
                "--remaining is required with --status low and must be a fraction from 0.0 to 1.0"
                    .to_owned()
            })?;
            if !remaining.is_finite() || !(0.0..=1.0).contains(&remaining) {
                return Err("--remaining must be a finite fraction from 0.0 to 1.0".into());
            }
            Ok(QuotaRecalibrationPayload::ExhaustedSoon(
                QuotaExhaustedSoon {
                    harness,
                    window_kind,
                    remaining,
                    ts: now,
                },
            ))
        }
    }
}

fn normalize_quota_filter(harness_id: Option<String>) -> Result<Option<String>, String> {
    harness_id
        .map(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                Err("--harness-id must not be empty".into())
            } else {
                Ok(trimmed.to_owned())
            }
        })
        .transpose()
}

fn normalize_required_quota_arg(value: &str, flag: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        Err(format!("{flag} must not be empty"))
    } else {
        Ok(trimmed.to_owned())
    }
}

fn parse_optional_datetime_arg(
    value: Option<String>,
    flag: &str,
) -> Result<Option<DateTime<Utc>>, String> {
    value
        .map(|value| {
            DateTime::parse_from_rfc3339(value.trim())
                .map(|dt| dt.with_timezone(&Utc))
                .map_err(|err| format!("{flag} must be RFC 3339: {err}"))
        })
        .transpose()
}

fn parse_query_quota_response(
    response: JsonValue,
    harness_id: Option<&str>,
    subject: &str,
) -> Result<Vec<QuotaWindow>, String> {
    if let Some(error) = response.get("error") {
        return Err(format_tool_error(subject, error));
    }

    if response
        .get("status")
        .and_then(serde_json::Value::as_str)
        .is_some()
    {
        let state = serde_json::from_value::<QuotaState>(response)
            .map_err(|err| format!("parse {subject} quota state response: {err}"))?;
        return Ok(vec![QuotaWindow {
            key: harness_id.unwrap_or("quota").to_owned(),
            state,
        }]);
    }

    let object = response
        .as_object()
        .ok_or_else(|| format!("{subject} returned non-object quota response"))?;
    let mut windows = Vec::with_capacity(object.len());
    for (key, value) in object {
        let state = serde_json::from_value::<QuotaState>(value.clone())
            .map_err(|err| format!("parse {subject} quota state {key}: {err}"))?;
        windows.push(QuotaWindow {
            key: key.clone(),
            state,
        });
    }
    windows.sort_by(|left, right| left.key.cmp(&right.key));
    Ok(windows)
}

fn format_tool_error(subject: &str, error: &JsonValue) -> String {
    let kind = error
        .get("kind")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown-error");
    let detail = error
        .get("detail")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("tool returned an error without detail");
    match error.get("tracked_by").and_then(serde_json::Value::as_str) {
        Some(tracked_by) => {
            format!("{subject} returned {kind}: {detail} (tracked by {tracked_by})")
        }
        None => format!("{subject} returned {kind}: {detail}"),
    }
}

fn print_quota_windows(windows: &[QuotaWindow]) {
    if windows.is_empty() {
        println!("no quota states found");
        return;
    }
    println!(
        "window\tkind\tstatus\tremaining\tresets_at\treset_cadence\tapi_budget\tusage\tprice_events\tobserved_at\tsource\tdetail"
    );
    for window in windows {
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            window.key,
            window.state.window_kind,
            window.state.status,
            format_remaining(window.state.remaining),
            format_optional_datetime(window.state.resets_at),
            format_reset_cadence(window.state.reset_cadence.as_ref()),
            format_api_budget(window.state.api_budget.as_ref()),
            format_usage(window.state.usage.as_ref()),
            format_price_events(&window.state.price_events),
            window
                .state
                .observed_at
                .to_rfc3339_opts(SecondsFormat::Secs, true),
            window.state.source,
            window.state.detail
        );
    }
}

fn format_remaining(remaining: Option<f64>) -> String {
    remaining.map_or_else(|| "-".into(), |value| format!("{:.1}%", value * 100.0))
}

fn format_reset_cadence(reset_cadence: Option<&QuotaResetCadence>) -> String {
    let Some(reset_cadence) = reset_cadence else {
        return "-".into();
    };
    let mut parts = vec![format!("{}s", reset_cadence.cadence_secs)];
    if let Some(next_reset_at) = reset_cadence.next_reset_at.as_ref() {
        parts.push(format!(
            "next={}",
            next_reset_at.to_rfc3339_opts(SecondsFormat::Secs, true)
        ));
    }
    if let Some(window_started_at) = reset_cadence.window_started_at.as_ref() {
        parts.push(format!(
            "started={}",
            window_started_at.to_rfc3339_opts(SecondsFormat::Secs, true)
        ));
    }
    if let Some(limit) = reset_cadence.limit_in_window {
        parts.push(format!("limit={limit}"));
    }
    if let Some(multiplier) = reset_cadence.multiplier {
        parts.push(format!("multiplier={multiplier:.2}"));
    }
    parts.join(",")
}

fn format_api_budget(api_budget: Option<&QuotaApiBudget>) -> String {
    let Some(api_budget) = api_budget else {
        return "-".into();
    };
    let rate_limit = api_budget.rate_limit_state.as_deref().map_or_else(
        || "rate-limit=unknown".to_owned(),
        |state| format!("rate-limit={state}"),
    );
    format!(
        "{}:{} ${:.2}/${:.2} in={:.4}/1M out={:.4}/1M {}",
        api_budget.provider,
        api_budget.model,
        api_budget.spent_this_month_usd,
        api_budget.monthly_cap_usd,
        api_budget.current_input_rate_per_1m,
        api_budget.current_output_rate_per_1m,
        rate_limit
    )
}

fn format_usage(usage: Option<&QuotaUsage>) -> String {
    let Some(usage) = usage else {
        return "-".into();
    };
    let mut parts = Vec::new();
    if let Some(provider) = &usage.provider {
        parts.push(format!("provider={provider}"));
    }
    if let Some(model) = &usage.model {
        parts.push(format!("model={model}"));
    }
    parts.push(format!("in={}", usage.input_tokens));
    parts.push(format!("out={}", usage.output_tokens));
    parts.push(format!("cost=${:.4}", usage.cost_usd));
    parts.push(format!("source={}", usage.last_source));
    parts.push(format!(
        "at={}",
        usage
            .last_observed_at
            .to_rfc3339_opts(SecondsFormat::Secs, true)
    ));
    parts.join(",")
}

fn format_price_events(price_events: &[QuotaPriceEvent]) -> String {
    if price_events.is_empty() {
        return "-".into();
    }
    price_events
        .iter()
        .map(format_price_event)
        .collect::<Vec<_>>()
        .join(";")
}

fn format_price_event(price_event: &QuotaPriceEvent) -> String {
    let mut parts = vec![price_event.name.clone()];
    if let Some(provider) = &price_event.provider {
        parts.push(format!("provider={provider}"));
    }
    if let Some(model) = &price_event.model {
        parts.push(format!("model={model}"));
    }
    if let Some(starts_at) = price_event.starts_at.as_ref() {
        parts.push(format!(
            "starts={}",
            starts_at.to_rfc3339_opts(SecondsFormat::Secs, true)
        ));
    }
    if let Some(ends_at) = price_event.ends_at.as_ref() {
        parts.push(format!(
            "ends={}",
            ends_at.to_rfc3339_opts(SecondsFormat::Secs, true)
        ));
    }
    if let Some(input_rate) = price_event.input_rate_per_1m {
        parts.push(format!("in={input_rate:.4}/1M"));
    }
    if let Some(output_rate) = price_event.output_rate_per_1m {
        parts.push(format!("out={output_rate:.4}/1M"));
    }
    if let Some(description) = &price_event.description {
        parts.push(format!("note={}", description.replace(['\t', '\n'], " ")));
    }
    parts.join(",")
}

fn format_optional_datetime(value: Option<chrono::DateTime<Utc>>) -> String {
    value.map_or_else(
        || "-".into(),
        |value| value.to_rfc3339_opts(SecondsFormat::Secs, true),
    )
}

#[derive(Debug, Serialize)]
struct QueryQuotaRequest {
    harness_id: Option<String>,
}

#[derive(Debug, Clone)]
struct QuotaWindow {
    key: String,
    state: QuotaState,
}

#[derive(Debug, Clone, Deserialize)]
struct QuotaState {
    status: String,
    detail: String,
    window_kind: String,
    source: String,
    remaining: Option<f64>,
    resets_at: Option<DateTime<Utc>>,
    reset_cadence: Option<QuotaResetCadence>,
    api_budget: Option<QuotaApiBudget>,
    usage: Option<QuotaUsage>,
    #[serde(default)]
    price_events: Vec<QuotaPriceEvent>,
    observed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
struct QuotaResetCadence {
    cadence_secs: u64,
    window_started_at: Option<DateTime<Utc>>,
    next_reset_at: Option<DateTime<Utc>>,
    limit_in_window: Option<u32>,
    multiplier: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
struct QuotaApiBudget {
    provider: String,
    model: String,
    monthly_cap_usd: f64,
    spent_this_month_usd: f64,
    current_input_rate_per_1m: f64,
    current_output_rate_per_1m: f64,
    rate_limit_state: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct QuotaUsage {
    provider: Option<String>,
    model: Option<String>,
    input_tokens: u64,
    output_tokens: u64,
    cost_usd: f64,
    last_source: String,
    last_observed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
struct QuotaPriceEvent {
    name: String,
    provider: Option<String>,
    model: Option<String>,
    description: Option<String>,
    starts_at: Option<DateTime<Utc>>,
    ends_at: Option<DateTime<Utc>>,
    input_rate_per_1m: Option<f64>,
    output_rate_per_1m: Option<f64>,
}

#[derive(Debug)]
enum QuotaRecalibrationPayload {
    Exhausted(QuotaExhausted),
    ExhaustedSoon(QuotaExhaustedSoon),
    Refilled(QuotaRefilled),
}

impl QuotaRecalibrationPayload {
    fn event_type(&self) -> &'static str {
        match self {
            Self::Exhausted(_) => QuotaExhausted::EVENT_TYPE,
            Self::ExhaustedSoon(_) => QuotaExhaustedSoon::EVENT_TYPE,
            Self::Refilled(_) => QuotaRefilled::EVENT_TYPE,
        }
    }
}

#[derive(Debug)]
struct QuotaRecalibrationRecord {
    subject: String,
    event_type: String,
    harness: String,
    window_kind: String,
    trace_id: String,
}

fn run_trace_replay(trace_id: &str, max_depth: u32) -> Result<(), String> {
    let replay = trace_replay_from_journal(&journal_root(), trace_id, max_depth)?;
    print_trace_replay(&replay);
    Ok(())
}

fn run_trace_find(filter: &str, limit: usize) -> Result<(), String> {
    let result =
        find_traces_in_journal(&journal_root(), filter, limit).map_err(|err| err.to_string())?;
    print_trace_find(&result);
    Ok(())
}

fn print_trace_replay(replay: &TraceReplay) {
    println!("trace_id: {}", replay.requested_trace_id);
    println!("max_depth: {}", replay.max_depth);
    println!("chain: {}", replay.chain.join(" <- "));
    println!("entries: {}", replay.entries.len());
    for entry in &replay.entries {
        let parent = entry.envelope.parent_trace_id.as_deref().unwrap_or("-");
        println!(
            "{} seq={} trace={} parent={} event={} actor={} source={}:{}{}",
            entry
                .envelope
                .timestamp
                .to_rfc3339_opts(SecondsFormat::Nanos, true),
            entry.envelope.journal_seq,
            entry.envelope.trace_id,
            parent,
            entry.envelope.event_type,
            entry.envelope.actor,
            entry.path.display(),
            entry.line_number,
            payload_context(&entry.envelope.payload)
        );
    }
}

fn print_trace_find(result: &TraceFindResult) {
    println!("filter: {}", result.filter);
    println!("limit: {}", result.limit);
    println!("matches: {}", result.matches.len());
    for trace in &result.matches {
        println!(
            "{} last_seen={} events={} event_count={}{}",
            trace.trace_id,
            trace.last_seen.to_rfc3339_opts(SecondsFormat::Nanos, true),
            trace.events.join(","),
            trace.event_count,
            trace_find_context(trace),
        );
    }
}

fn trace_find_context(trace: &jam_ui_server::trace_replay::TraceFindMatch) -> String {
    let mut parts = Vec::new();
    if let Some(parent_trace_id) = &trace.parent_trace_id {
        parts.push(format!("parent={parent_trace_id}"));
    }
    for (key, values) in [
        ("task_id", &trace.task_ids),
        ("session_id", &trace.session_ids),
        ("pr_ref", &trace.pr_refs),
        ("harness", &trace.harnesses),
        ("outcome", &trace.outcomes),
    ] {
        if !values.is_empty() {
            parts.push(format!("{key}={}", values.join(",")));
        }
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!(" {}", parts.join(" "))
    }
}

fn payload_context(payload: &serde_json::Value) -> String {
    let mut parts = Vec::new();
    for key in ["task_id", "session_id", "pr_ref", "worktree_path"] {
        if let Some(value) = payload.get(key).and_then(serde_json::Value::as_str) {
            parts.push(format!("{key}={value}"));
        }
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!(" {}", parts.join(" "))
    }
}

#[derive(Debug)]
struct TraceReplay {
    requested_trace_id: String,
    max_depth: u32,
    chain: Vec<String>,
    entries: Vec<TraceReplayEntry>,
}

#[derive(Debug)]
struct TraceReplayEntry {
    path: PathBuf,
    line_number: usize,
    envelope: GenericJournalEnvelope,
}

#[derive(Debug, Clone, Deserialize)]
struct GenericJournalEnvelope {
    event_type: String,
    timestamp: chrono::DateTime<Utc>,
    journal_seq: u64,
    trace_id: String,
    parent_trace_id: Option<String>,
    actor: String,
    payload: serde_json::Value,
}

fn trace_replay_from_journal(
    journal_root: &Path,
    trace_id: &str,
    max_depth: u32,
) -> Result<TraceReplay, String> {
    trace_id
        .parse::<TraceId>()
        .map_err(|err| format!("invalid trace id {trace_id}: {err}"))?;

    let mut all_entries = read_trace_journal_entries(journal_root)?;
    let chain = trace_parent_chain(&all_entries, trace_id, max_depth);
    let selected: HashSet<&str> = chain.iter().map(String::as_str).collect();
    all_entries.retain(|entry| selected.contains(entry.envelope.trace_id.as_str()));
    all_entries.sort_by(|left, right| {
        left.envelope
            .timestamp
            .cmp(&right.envelope.timestamp)
            .then(left.envelope.journal_seq.cmp(&right.envelope.journal_seq))
            .then(left.path.cmp(&right.path))
            .then(left.line_number.cmp(&right.line_number))
    });

    if all_entries.is_empty() {
        return Err(format!(
            "no journal entries found for trace {trace_id} in {}",
            journal_root.display()
        ));
    }

    Ok(TraceReplay {
        requested_trace_id: trace_id.into(),
        max_depth,
        chain,
        entries: all_entries,
    })
}

fn trace_parent_chain(entries: &[TraceReplayEntry], trace_id: &str, max_depth: u32) -> Vec<String> {
    let mut parent_by_trace = HashMap::new();
    for entry in entries {
        if let Some(parent) = &entry.envelope.parent_trace_id {
            parent_by_trace
                .entry(entry.envelope.trace_id.as_str())
                .or_insert(parent.as_str());
        }
    }

    let mut chain = vec![trace_id.to_owned()];
    let mut current = trace_id;
    for _ in 0..max_depth {
        let Some(parent) = parent_by_trace.get(current) else {
            break;
        };
        if chain.iter().any(|seen| seen == parent) {
            break;
        }
        chain.push((*parent).to_owned());
        current = parent;
    }
    chain
}

fn read_trace_journal_entries(journal_root: &Path) -> Result<Vec<TraceReplayEntry>, String> {
    let mut entries = Vec::new();
    for path in journal_jsonl_paths(journal_root)? {
        let file = File::open(&path).map_err(|err| format!("open {}: {err}", path.display()))?;
        for (idx, line) in BufReader::new(file).lines().enumerate() {
            let line =
                line.map_err(|err| format!("read {} line {}: {err}", path.display(), idx + 1))?;
            let envelope = serde_json::from_str::<GenericJournalEnvelope>(&line)
                .map_err(|err| format!("parse {} line {}: {err}", path.display(), idx + 1))?;
            entries.push(TraceReplayEntry {
                path: path.clone(),
                line_number: idx + 1,
                envelope,
            });
        }
    }
    Ok(entries)
}

fn journal_jsonl_paths(root: &Path) -> Result<Vec<PathBuf>, String> {
    if !root.exists() {
        return Err(format!("journal root does not exist: {}", root.display()));
    }
    let mut paths = Vec::new();
    let entries = fs::read_dir(root).map_err(|err| format!("read {}: {err}", root.display()))?;
    for entry in entries {
        let entry = entry.map_err(|err| format!("read {}: {err}", root.display()))?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        for file in fs::read_dir(&path).map_err(|err| format!("read {}: {err}", path.display()))? {
            let file = file.map_err(|err| format!("read {}: {err}", path.display()))?;
            let candidate = file.path();
            if candidate.extension().and_then(|ext| ext.to_str()) == Some("jsonl") {
                paths.push(candidate);
            }
        }
    }
    paths.sort();
    Ok(paths)
}

#[derive(Debug)]
struct TaskRecord {
    task_id: String,
    trace_id: String,
    timestamp: String,
    description: String,
    project: String,
    task_class: String,
    priority: String,
    requested_by: String,
}

fn read_task_records() -> Result<Vec<TaskRecord>, String> {
    let mut records = Vec::new();
    for path in task_journal_paths()? {
        let file = File::open(&path).map_err(|err| format!("open {}: {err}", path.display()))?;
        for (idx, line) in BufReader::new(file).lines().enumerate() {
            let line =
                line.map_err(|err| format!("read {} line {}: {err}", path.display(), idx + 1))?;
            let envelope = serde_json::from_str::<EventEnvelope<TaskRequested>>(&line)
                .map_err(|err| format!("parse {} line {}: {err}", path.display(), idx + 1))?;
            records.push(TaskRecord::from(envelope));
        }
    }
    Ok(records)
}

fn task_journal_paths() -> Result<Vec<PathBuf>, String> {
    let root = journal_root();
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut paths = Vec::new();
    let entries =
        std::fs::read_dir(&root).map_err(|err| format!("read {}: {err}", root.display()))?;
    for entry in entries {
        let entry = entry.map_err(|err| format!("read {}: {err}", root.display()))?;
        let candidate = entry.path().join("journal.task.jsonl");
        if candidate.is_file() {
            paths.push(candidate);
        }
    }
    Ok(paths)
}

impl From<EventEnvelope<TaskRequested>> for TaskRecord {
    fn from(envelope: EventEnvelope<TaskRequested>) -> Self {
        let payload = envelope.payload;
        Self {
            task_id: payload.task_id,
            trace_id: envelope.trace_id,
            timestamp: envelope
                .timestamp
                .to_rfc3339_opts(SecondsFormat::Nanos, true),
            description: payload.description,
            project: payload.project,
            task_class: payload.task_class,
            priority: payload.priority,
            requested_by: payload.requested_by,
        }
    }
}

fn task_id_for(description: &str, trace_ctx: &TraceCtx) -> String {
    let date = Utc::now().format("%Y-%m-%d");
    let slug = slugify(description);
    let trace = trace_ctx.trace_id.to_string().to_ascii_lowercase();
    let suffix = &trace[trace.len() - 6..];
    format!("{date}-{slug}-{suffix}")
}

fn slugify(input: &str) -> String {
    let mut slug = String::new();
    let mut last_was_dash = false;
    for char in input.chars().flat_map(char::to_lowercase) {
        if char.is_ascii_alphanumeric() {
            slug.push(char);
            last_was_dash = false;
        } else if !last_was_dash && !slug.is_empty() {
            slug.push('-');
            last_was_dash = true;
        }
        if slug.len() >= 48 {
            break;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        "task".into()
    } else {
        slug
    }
}

fn run_async<F>(future: F) -> Result<(), String>
where
    F: std::future::Future<Output = Result<(), String>>,
{
    run_async_value(future)
}

fn run_async_value<F, T>(future: F) -> Result<T, String>
where
    F: std::future::Future<Output = Result<T, String>>,
{
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|err| format!("create tokio runtime: {err}"))?;
    runtime.block_on(future)
}

fn run_ui(action: UiAction) -> ExitCode {
    if let Some(exit) = delegate_ui_to_maestro_if_needed() {
        return exit;
    }
    let store = TokenStore::from_jam_home(jam_home());
    match action {
        UiAction::Token { user_id } => match store.issue(&user_id) {
            Ok(issued) => {
                println!("id: {}", issued.id);
                println!("user_id: {}", issued.user_id);
                println!("token: {}", issued.token);
                ExitCode::SUCCESS
            }
            Err(err) => {
                eprintln!("jam ui token failed: {err}");
                ExitCode::from(1)
            }
        },
        UiAction::TokenRevoke { id } => match store.revoke(&id) {
            Ok(true) => {
                println!("revoked: {id}");
                ExitCode::SUCCESS
            }
            Ok(false) => {
                eprintln!("token not found: {id}");
                ExitCode::from(1)
            }
            Err(err) => {
                eprintln!("jam ui token-revoke failed: {err}");
                ExitCode::from(1)
            }
        },
        UiAction::TokenRevokeAll => match store.revoke_all() {
            Ok(count) => {
                println!("revoked: {count}");
                ExitCode::SUCCESS
            }
            Err(err) => {
                eprintln!("jam ui token-revoke-all failed: {err}");
                ExitCode::from(1)
            }
        },
    }
}

// UI session tokens are server-side state owned by `maestro` (the ui-server
// reads from /home/maestro/.jam/ui/session-tokens.json). When a non-maestro
// Manager runs `jam ui …`, re-exec under maestro so writes land in the store
// the ui-server actually reads, matching the §6.2 pattern used for the NATS
// token. Honors explicit JAM_HOME for tests/dev that want to target a
// specific store directly.
fn delegate_ui_to_maestro_if_needed() -> Option<ExitCode> {
    let username = std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_default();
    if username == jam_tools_core::paths::MAESTRO_USER {
        return None;
    }
    if std::env::var_os("JAM_HOME").is_some() {
        return None;
    }
    let forwarded: Vec<String> = std::env::args().skip(1).collect();
    let status = ProcessCommand::new("sudo")
        .args([
            "-n",
            "-u",
            jam_tools_core::paths::MAESTRO_USER,
            "-i",
            "/opt/jam/bin/jam",
        ])
        .args(&forwarded)
        .status();
    Some(match status {
        Ok(status) => status
            .code()
            .and_then(|code| u8::try_from(code).ok())
            .map_or(ExitCode::from(1), ExitCode::from),
        Err(err) => {
            eprintln!(
                "jam ui: re-exec under {} failed: {err}",
                jam_tools_core::paths::MAESTRO_USER
            );
            ExitCode::from(1)
        }
    })
}

fn run_tempyr(action: TempyrAction) -> ExitCode {
    let result = match action {
        TempyrAction::CanonicalWorktree { action } => match action {
            CanonicalWorktreeAction::Recreate => CanonicalWorktreeConfig::from_env()
                .and_then(|config| recreate_canonical_worktree(&config)),
            CanonicalWorktreeAction::ReplayTasks => CanonicalWorktreeConfig::from_env()
                .and_then(|config| replay_canonical_task_state(&config)),
        },
    };

    match result {
        Ok(outcome) => {
            println!("{}", outcome.summary());
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("jam tempyr failed: {err}");
            ExitCode::from(1)
        }
    }
}

#[derive(Debug, Clone)]
struct CanonicalWorktreeConfig {
    repo_path: PathBuf,
    worktree_path: PathBuf,
    branch: String,
    base_ref: Option<String>,
    graph_relpath: PathBuf,
    journal_root: PathBuf,
}

impl CanonicalWorktreeConfig {
    fn from_env() -> Result<Self, String> {
        let repo_path = std::env::var_os("JAM_PROJECT_REPO")
            .or_else(|| std::env::var_os("JAM_BLUEBERRY_REPO"))
            .map_or_else(|| PathBuf::from("/home/caleb/blueberry"), PathBuf::from);
        let worktree_path = std::env::var_os("JAM_CANONICAL_TEMPYR_WORKTREE")
            .or_else(|| std::env::var_os("JAM_TEMPYR_WORKTREE"))
            .map_or_else(|| PathBuf::from("/home/caleb/blueberry-jam"), PathBuf::from);
        let branch = std::env::var("JAM_TEMPYR_BRANCH").unwrap_or_else(|_| "tempyr-live".into());
        validate_git_ref("JAM_TEMPYR_BRANCH", &branch)?;
        if branch == "HEAD" || branch.starts_with("origin/") || branch.starts_with("refs/") {
            return Err(format!(
                "JAM_TEMPYR_BRANCH must be a local branch name, got {branch}"
            ));
        }
        let base_ref = std::env::var("JAM_TEMPYR_BASE_REF")
            .ok()
            .filter(|value| !value.trim().is_empty());
        if let Some(base_ref) = base_ref.as_deref() {
            validate_git_ref("JAM_TEMPYR_BASE_REF", base_ref)?;
        }
        let graph_relpath = std::env::var_os("JAM_GRAPH_RELPATH")
            .map_or_else(|| PathBuf::from("graph"), PathBuf::from);
        validate_graph_relpath(&graph_relpath)?;
        let journal_root =
            std::env::var_os("JAM_JOURNAL_ROOT").map_or_else(journal_root, PathBuf::from);
        Ok(Self {
            repo_path,
            worktree_path,
            branch,
            base_ref,
            graph_relpath,
            journal_root,
        })
    }

    fn task_dir(&self) -> PathBuf {
        self.worktree_path.join(&self.graph_relpath).join("tasks")
    }
}

#[derive(Debug, Clone)]
struct CanonicalOutcome {
    worktree_path: PathBuf,
    created: bool,
    replayed_events: usize,
}

impl CanonicalOutcome {
    fn summary(&self) -> String {
        let action = if self.created {
            "created"
        } else {
            "already exists"
        };
        if self.replayed_events == 0 {
            format!(
                "canonical Tempyr worktree {action}: {}",
                self.worktree_path.display()
            )
        } else {
            format!(
                "canonical Tempyr worktree {action}: {}; replayed {} journal event(s)",
                self.worktree_path.display(),
                self.replayed_events
            )
        }
    }
}

fn ensure_canonical_worktree(config: &CanonicalWorktreeConfig) -> Result<CanonicalOutcome, String> {
    validate_repo(&config.repo_path)?;
    if config.worktree_path.exists() {
        ensure_task_dir(config)?;
        return Ok(CanonicalOutcome {
            worktree_path: config.worktree_path.clone(),
            created: false,
            replayed_events: 0,
        });
    }
    add_canonical_worktree(config)?;
    set_shared_dir_mode(&config.worktree_path)?;
    ensure_task_dir(config)?;
    Ok(CanonicalOutcome {
        worktree_path: config.worktree_path.clone(),
        created: true,
        replayed_events: 0,
    })
}

fn recreate_canonical_worktree(
    config: &CanonicalWorktreeConfig,
) -> Result<CanonicalOutcome, String> {
    validate_repo(&config.repo_path)?;
    let replacing_existing = config.worktree_path.exists();
    if replacing_existing {
        git(
            &config.repo_path,
            &[
                "worktree",
                "remove",
                "--force",
                path_arg(&config.worktree_path)?,
            ],
        )?;
    }
    add_canonical_worktree(config)?;
    set_shared_dir_mode(&config.worktree_path)?;
    let task_dir = config.task_dir();
    if replacing_existing {
        if task_dir.exists() {
            fs::remove_dir_all(&task_dir)
                .map_err(|err| format!("remove {}: {err}", task_dir.display()))?;
        }
        fs::create_dir_all(&task_dir)
            .map_err(|err| format!("create {}: {err}", task_dir.display()))?;
    } else {
        ensure_task_dir(config)?;
    }
    let replayed_events = replay_task_journal(config)?;
    Ok(CanonicalOutcome {
        worktree_path: config.worktree_path.clone(),
        created: true,
        replayed_events,
    })
}

fn replay_canonical_task_state(
    config: &CanonicalWorktreeConfig,
) -> Result<CanonicalOutcome, String> {
    if !config.worktree_path.is_dir() {
        return Err(format!(
            "canonical Tempyr worktree does not exist: {}",
            config.worktree_path.display()
        ));
    }
    ensure_task_dir(config)?;
    let replayed_events = replay_task_journal(config)?;
    Ok(CanonicalOutcome {
        worktree_path: config.worktree_path.clone(),
        created: false,
        replayed_events,
    })
}

fn add_canonical_worktree(config: &CanonicalWorktreeConfig) -> Result<(), String> {
    let worktree_path = path_arg(&config.worktree_path)?;
    let branch_ref = format!("refs/heads/{}", config.branch);
    if git_ref_exists(&config.repo_path, &branch_ref)? {
        return git(
            &config.repo_path,
            &["worktree", "add", worktree_path, &config.branch],
        );
    }
    let base_ref = resolve_canonical_base_ref(config)?;
    git(
        &config.repo_path,
        &[
            "worktree",
            "add",
            "-b",
            &config.branch,
            worktree_path,
            &base_ref,
        ],
    )
}

fn resolve_canonical_base_ref(config: &CanonicalWorktreeConfig) -> Result<String, String> {
    if let Some(base_ref) = &config.base_ref {
        return Ok(base_ref.clone());
    }
    let remote_branch = format!("origin/{}", config.branch);
    let candidates = [
        remote_branch.as_str(),
        "origin/master",
        "origin/main",
        "master",
        "main",
    ];
    for candidate in candidates {
        if git_rev_exists(&config.repo_path, candidate)? {
            return Ok(candidate.to_owned());
        }
    }
    Err(format!(
        "local branch {} does not exist and no base ref was found; set JAM_TEMPYR_BASE_REF",
        config.branch
    ))
}

fn validate_repo(path: &Path) -> Result<(), String> {
    if !path.is_dir() {
        return Err(format!(
            "Blueberry repo does not exist or is not a directory: {}. \
             Set JAM_PROJECT_REPO to the pristine Blueberry checkout.",
            path.display()
        ));
    }
    let git_dir = path.join(".git");
    if !git_dir.exists() {
        return Err(format!(
            "Blueberry repo is not a git checkout: {}",
            path.display()
        ));
    }
    Ok(())
}

fn ensure_task_dir(config: &CanonicalWorktreeConfig) -> Result<(), String> {
    let task_dir = config.task_dir();
    fs::create_dir_all(&task_dir).map_err(|err| format!("create {}: {err}", task_dir.display()))
}

fn validate_graph_relpath(path: &Path) -> Result<(), String> {
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::CurDir | Component::Prefix(_)
            )
        })
    {
        return Err(format!(
            "JAM_GRAPH_RELPATH must be a relative native path, got {}",
            path.display()
        ));
    }
    Ok(())
}

fn validate_git_ref(label: &str, value: &str) -> Result<(), String> {
    if value.is_empty() || value.starts_with('-') {
        return Err(format!("{label} must be a non-option git ref"));
    }
    if value.contains("..")
        || value.contains("//")
        || value.ends_with('/')
        || value.split('/').any(|part| {
            Path::new(part)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("lock"))
        })
    {
        return Err(format!("{label} is not a safe git ref: {value}"));
    }
    if value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'/' | b'_' | b'-'))
    {
        Ok(())
    } else {
        Err(format!(
            "{label} may only contain ASCII letters, numbers, `.`, `/`, `_`, and `-`"
        ))
    }
}

fn replay_task_journal(config: &CanonicalWorktreeConfig) -> Result<usize, String> {
    if !config.journal_root.exists() {
        return Ok(0);
    }
    let mut files = Vec::new();
    for day in fs::read_dir(&config.journal_root)
        .map_err(|err| format!("read {}: {err}", config.journal_root.display()))?
    {
        let day = day.map_err(|err| format!("read {}: {err}", config.journal_root.display()))?;
        if !day.path().is_dir() {
            continue;
        }
        for entry in fs::read_dir(day.path())
            .map_err(|err| format!("read journal day {}: {err}", day.path().display()))?
        {
            let entry =
                entry.map_err(|err| format!("read journal day {}: {err}", day.path().display()))?;
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "jsonl") {
                files.push(path);
            }
        }
    }
    files.sort();

    let mut envelopes = Vec::new();
    for path in files {
        let file = File::open(&path).map_err(|err| format!("open {}: {err}", path.display()))?;
        for (index, line) in BufReader::new(file).lines().enumerate() {
            let line =
                line.map_err(|err| format!("read {} line {}: {err}", path.display(), index + 1))?;
            if line.trim().is_empty() {
                continue;
            }
            let envelope = serde_json::from_str::<EventEnvelope<serde_json::Value>>(&line)
                .map_err(|err| format!("parse {} line {}: {err}", path.display(), index + 1))?;
            envelopes.push(envelope);
        }
    }
    envelopes.sort_by(|a, b| {
        (a.timestamp, a.journal_seq, transition_rank(&a.event_type)).cmp(&(
            b.timestamp,
            b.journal_seq,
            transition_rank(&b.event_type),
        ))
    });

    let mut applied = 0usize;
    for envelope in envelopes {
        if apply_replayed_event(config, &envelope)? {
            applied += 1;
        }
    }
    Ok(applied)
}

fn transition_rank(event_type: &str) -> u8 {
    match event_type {
        "task.requested" => 0,
        "picker.spawned" => 1,
        "picker.exited" => 2,
        "pr.opened" => 3,
        "pr.merged" => 4,
        "task.failed" => 5,
        "task.abandoned" => 5,
        _ => 9,
    }
}

fn apply_replayed_event(
    config: &CanonicalWorktreeConfig,
    envelope: &EventEnvelope<serde_json::Value>,
) -> Result<bool, String> {
    if !matches!(
        envelope.event_type.as_str(),
        "task.requested"
            | "picker.spawned"
            | "picker.exited"
            | "pr.opened"
            | "pr.merged"
            | "task.failed"
            | "task.abandoned"
    ) {
        return Ok(false);
    }
    let task_id = json_string(&envelope.payload, "task_id")
        .ok_or_else(|| format!("{} payload missing task_id", envelope.event_type))?;
    validate_task_id(&task_id)?;

    let task_path = config.task_dir().join(format!("{task_id}.md"));
    let mut node = TaskNode::load_or_new(&task_path, &task_id, envelope.timestamp)?;
    match envelope.event_type.as_str() {
        "task.requested" => {
            node.set_str("status", "backlog");
            copy_json_string(&mut node, &envelope.payload, "description", "description");
            copy_json_string(&mut node, &envelope.payload, "project", "project");
            copy_json_string(&mut node, &envelope.payload, "task_class", "task-class");
            copy_json_string(&mut node, &envelope.payload, "priority", "priority");
            copy_json_string(&mut node, &envelope.payload, "requested_by", "requested-by");
            node.set_str("trace-id", &envelope.trace_id);
        }
        "picker.spawned" => {
            node.set_str("status", "in-progress");
            copy_json_string(&mut node, &envelope.payload, "spawned_at", "spawned-at");
            copy_json_string(&mut node, &envelope.payload, "session_id", "session-id");
            copy_json_string(&mut node, &envelope.payload, "session_id", "picker-handle");
            copy_json_string(
                &mut node,
                &envelope.payload,
                "worktree_path",
                "worktree-path",
            );
            copy_json_string(&mut node, &envelope.payload, "harness", "harness");
            copy_json_string(
                &mut node,
                &envelope.payload,
                "picker_trace_id",
                "picker-trace-id",
            );
            copy_json_string(&mut node, &envelope.payload, "maestro_trace_id", "trace-id");
            if let Some(parent) = &envelope.parent_trace_id {
                node.set_str("parent-trace-id", parent);
            }
            node.set_str("journal-trace-id", &envelope.trace_id);
        }
        "picker.exited" => {
            copy_json_string(&mut node, &envelope.payload, "session_id", "session-id");
            copy_json_string(&mut node, &envelope.payload, "exit_code", "exit-code");
            copy_json_string(&mut node, &envelope.payload, "exited_at", "exited-at");
            copy_json_string(&mut node, &envelope.payload, "duration_ms", "duration-ms");

            if !matches!(
                node.string("status").as_deref(),
                Some("in-review" | "merged")
            ) {
                if envelope
                    .payload
                    .get("exit_code")
                    .and_then(serde_json::Value::as_u64)
                    == Some(0)
                {
                    node.set_str("status", "picker-completed");
                    node.set_str("outcome", "picker-exited-zero");
                } else {
                    node.set_str("status", "failed");
                    node.set_str("outcome", "picker-exited-nonzero");
                }
            }
        }
        "pr.opened" => {
            node.set_str("status", "in-review");
            copy_json_string(&mut node, &envelope.payload, "pr_ref", "pr-ref");
            copy_json_string(&mut node, &envelope.payload, "branch", "pr-branch");
            copy_json_string(&mut node, &envelope.payload, "title", "pr-title");
            copy_json_string(&mut node, &envelope.payload, "opened_at", "pr-opened-at");
            if let Some(value) = envelope
                .payload
                .get("draft")
                .and_then(serde_json::Value::as_bool)
            {
                node.set_bool("pr-draft", value);
            }
        }
        "pr.merged" => {
            node.set_str("status", "merged");
            node.set_str("outcome", "merged");
            copy_json_string(&mut node, &envelope.payload, "pr_ref", "pr-ref");
            copy_json_string(&mut node, &envelope.payload, "merged_sha", "merged-sha");
            copy_json_string(&mut node, &envelope.payload, "merged_by", "merged-by");
            copy_json_string(&mut node, &envelope.payload, "merged_at", "merged-at");
            copy_json_string(
                &mut node,
                &envelope.payload,
                "touched_paths",
                "touched-paths",
            );
        }
        "task.failed" => {
            node.set_str("status", "failed");
            copy_json_string(&mut node, &envelope.payload, "reason", "outcome");
            copy_json_string(&mut node, &envelope.payload, "reason", "failure-reason");
            copy_json_string(&mut node, &envelope.payload, "detail", "failure-detail");
            copy_json_string(&mut node, &envelope.payload, "failed_at", "failed-at");
            copy_json_string(
                &mut node,
                &envelope.payload,
                "source_event_type",
                "failure-source",
            );
        }
        "task.abandoned" => {
            node.set_str("status", "abandoned");
            copy_json_string(&mut node, &envelope.payload, "reason", "outcome");
            copy_json_string(&mut node, &envelope.payload, "reason", "abandoned-reason");
            copy_json_string(&mut node, &envelope.payload, "abandoned_at", "abandoned-at");
        }
        _ => return Ok(false),
    }
    node.set_str("updated", Utc::now().to_rfc3339());
    node.set_str("last-updated", Utc::now().to_rfc3339());
    node.write(&task_path)?;
    Ok(true)
}

struct TaskNode {
    frontmatter: Mapping,
    body: String,
}

impl TaskNode {
    fn load_or_new(
        path: &Path,
        task_id: &str,
        created_at: chrono::DateTime<Utc>,
    ) -> Result<Self, String> {
        if path.exists() {
            return Self::load(path);
        }
        let mut frontmatter = Mapping::new();
        yaml_insert_str(&mut frontmatter, "id", task_id);
        yaml_insert_str(&mut frontmatter, "type", "task");
        yaml_insert_str(&mut frontmatter, "status", "backlog");
        yaml_insert_str(&mut frontmatter, "created", created_at.to_rfc3339());
        yaml_insert_str(&mut frontmatter, "updated", created_at.to_rfc3339());
        frontmatter.insert(
            YamlValue::String("edges".into()),
            YamlValue::Sequence(Vec::new()),
        );
        Ok(Self {
            frontmatter,
            body: "Lifecycle task node rebuilt from orchestrator journal events.\n".into(),
        })
    }

    fn load(path: &Path) -> Result<Self, String> {
        let raw =
            fs::read_to_string(path).map_err(|err| format!("read {}: {err}", path.display()))?;
        let Some(rest) = raw.strip_prefix("---\n") else {
            return Err(format!(
                "task node lacks YAML frontmatter: {}",
                path.display()
            ));
        };
        let Some((yaml, body)) = rest.split_once("\n---\n") else {
            return Err(format!(
                "task node frontmatter is not closed: {}",
                path.display()
            ));
        };
        let frontmatter = serde_yaml::from_str::<Mapping>(yaml)
            .map_err(|err| format!("parse task frontmatter {}: {err}", path.display()))?;
        Ok(Self {
            frontmatter,
            body: body.to_owned(),
        })
    }

    fn write(&self, path: &Path) -> Result<(), String> {
        let yaml = serde_yaml::to_string(&self.frontmatter)
            .map_err(|err| format!("serialize task frontmatter: {err}"))?;
        fs::write(path, format!("---\n{}---\n{}", yaml, self.body))
            .map_err(|err| format!("write {}: {err}", path.display()))
    }

    fn set_str(&mut self, key: &str, value: impl AsRef<str>) {
        yaml_insert_str(&mut self.frontmatter, key, value.as_ref());
    }

    fn set_bool(&mut self, key: &str, value: bool) {
        self.frontmatter
            .insert(YamlValue::String(key.into()), YamlValue::Bool(value));
    }

    fn string(&self, key: &str) -> Option<String> {
        self.frontmatter
            .get(YamlValue::String(key.into()))?
            .as_str()
            .map(ToOwned::to_owned)
    }
}

fn yaml_insert_str(mapping: &mut Mapping, key: &str, value: impl AsRef<str>) {
    mapping.insert(
        YamlValue::String(key.into()),
        YamlValue::String(value.as_ref().into()),
    );
}

fn copy_json_string(node: &mut TaskNode, payload: &serde_json::Value, from: &str, to: &str) {
    if let Some(value) = json_string(payload, from) {
        node.set_str(to, value);
    }
}

fn json_string(payload: &serde_json::Value, field: &str) -> Option<String> {
    payload
        .get(field)?
        .as_str()
        .map(ToOwned::to_owned)
        .or_else(|| payload.get(field).map(ToString::to_string))
}

fn validate_task_id(task_id: &str) -> Result<(), String> {
    if task_id.is_empty() || task_id.len() > 128 {
        return Err("task_id must be 1-128 characters".into());
    }
    if task_id == "." || task_id == ".." || task_id.contains("..") {
        return Err(format!(
            "task_id may not contain parent-directory segments: {task_id}"
        ));
    }
    if !task_id
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(format!("task_id contains unsafe characters: {task_id}"));
    }
    Ok(())
}

fn git(repo: &Path, args: &[&str]) -> Result<(), String> {
    let output = ProcessCommand::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .map_err(|err| format!("run git: {err}"))?;
    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "git -C {} {} failed: {}",
        repo.display(),
        args.join(" "),
        String::from_utf8_lossy(&output.stderr).trim()
    ))
}

fn git_ref_exists(repo: &Path, refname: &str) -> Result<bool, String> {
    let output = ProcessCommand::new("git")
        .arg("-C")
        .arg(repo)
        .args(["show-ref", "--verify", "--quiet", refname])
        .output()
        .map_err(|err| format!("run git: {err}"))?;
    match output.status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => Err(format!(
            "git -C {} show-ref --verify --quiet {} failed: {}",
            repo.display(),
            refname,
            String::from_utf8_lossy(&output.stderr).trim()
        )),
    }
}

fn git_rev_exists(repo: &Path, rev: &str) -> Result<bool, String> {
    let commit_rev = format!("{rev}^{{commit}}");
    let output = ProcessCommand::new("git")
        .arg("-C")
        .arg(repo)
        .args(["rev-parse", "--verify", "--quiet", &commit_rev])
        .output()
        .map_err(|err| format!("run git: {err}"))?;
    match output.status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => Err(format!(
            "git -C {} rev-parse --verify --quiet {} failed: {}",
            repo.display(),
            commit_rev,
            String::from_utf8_lossy(&output.stderr).trim()
        )),
    }
}

fn path_arg(path: &Path) -> Result<&str, String> {
    path.to_str()
        .ok_or_else(|| format!("path is not valid UTF-8: {}", path.display()))
}

fn validate_maestro_session_id(session_id: &str) -> Result<(), String> {
    if session_id.is_empty() || session_id.starts_with('.') {
        return Err("session id must be non-empty and may not start with `.`".into());
    }
    if session_id
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        Ok(())
    } else {
        Err("session id may only contain ASCII letters, numbers, `-`, `_`, `.`, and `:`".into())
    }
}

fn aborted_maestro_session_path_in(root: &Path, session_id: &str) -> Result<PathBuf, String> {
    validate_maestro_session_id(session_id)?;
    Ok(root
        .join("maestro-aborted-sessions")
        .join(format!("{session_id}.json")))
}

fn maestro_resume_request_path_in(root: &Path, session_id: &str) -> Result<PathBuf, String> {
    validate_maestro_session_id(session_id)?;
    Ok(root
        .join("maestro-resume-requests")
        .join(format!("{session_id}.json")))
}

fn read_json_file(path: &Path) -> Result<JsonValue, String> {
    let raw = fs::read_to_string(path).map_err(|err| format!("read {}: {err}", path.display()))?;
    serde_json::from_str(&raw).map_err(|err| format!("parse {} as JSON: {err}", path.display()))
}

fn validate_abort_dump_session(
    dump: &JsonValue,
    session_id: &str,
    path: &Path,
) -> Result<(), String> {
    match dump.get("session_id").and_then(JsonValue::as_str) {
        Some(value) if value == session_id => Ok(()),
        Some(value) => Err(format!(
            "aborted session dump {} belongs to session {value}, not {session_id}",
            path.display()
        )),
        None => Err(format!(
            "aborted session dump {} is missing `session_id`",
            path.display()
        )),
    }
}

fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("path has no parent: {}", path.display()))?;
    fs::create_dir_all(parent).map_err(|err| format!("mkdir -p {}: {err}", parent.display()))?;

    let mut body = serde_json::to_vec_pretty(value)
        .map_err(|err| format!("serialize {}: {err}", path.display()))?;
    body.push(b'\n');

    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("path has no valid file name: {}", path.display()))?;
    let tmp = path.with_file_name(format!(".{filename}.tmp"));
    fs::write(&tmp, body).map_err(|err| format!("write {}: {err}", tmp.display()))?;
    set_private_file_mode(&tmp)?;
    fs::rename(&tmp, path)
        .map_err(|err| format!("rename {} to {}: {err}", tmp.display(), path.display()))
}

#[cfg(unix)]
fn set_private_file_mode(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|err| format!("chmod 600 {}: {err}", path.display()))
}

#[cfg(not(unix))]
fn set_private_file_mode(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(unix)]
fn set_shared_dir_mode(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o2770))
        .map_err(|err| format!("chmod 2770 {}: {err}", path.display()))
}

#[cfg(not(unix))]
fn set_shared_dir_mode(_path: &Path) -> Result<(), String> {
    Ok(())
}

fn jam_home() -> PathBuf {
    jam_tools_core::paths::jam_home()
}

fn journal_root() -> PathBuf {
    jam_home().join("journal")
}

fn default_nats_url() -> String {
    std::env::var("NATS_URL").unwrap_or_else(|_| "nats://127.0.0.1:4222".into())
}

const MAESTRO_NATS_TOKEN_KEY: &str = "jam/nats/token";

fn resolve_nats_token() -> Option<String> {
    std::env::var("NATS_TOKEN")
        .ok()
        .filter(|token| !token.trim().is_empty())
        .or_else(|| read_maestro_pass_secret(MAESTRO_NATS_TOKEN_KEY).ok())
}

fn read_maestro_pass_secret(key: &str) -> Result<String, String> {
    read_maestro_pass_secret_with_sudo(Path::new("sudo"), key)
}

fn read_maestro_pass_secret_with_sudo(sudo_bin: &Path, key: &str) -> Result<String, String> {
    validate_pass_key(key)?;
    let output = ProcessCommand::new(sudo_bin)
        .args(["-n", "-u", "maestro", "-i", "pass", "show", key])
        .output()
        .map_err(|err| format!("run sudo pass bridge for {key}: {err}"))?;

    if !output.status.success() {
        return Err(format!(
            "sudo pass bridge for {key} exited with status {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    let value = String::from_utf8(output.stdout)
        .map_err(|err| format!("sudo pass bridge for {key} returned invalid utf-8: {err}"))?;
    let value = value.strip_suffix('\n').unwrap_or(&value).to_owned();
    if value.is_empty() {
        return Err(format!(
            "sudo pass bridge for {key} returned an empty value"
        ));
    }
    Ok(value)
}

fn validate_pass_key(key: &str) -> Result<(), String> {
    if key.is_empty() || key.starts_with('-') || key.starts_with('/') {
        return Err("pass key must be a relative key path".into());
    }
    if key.contains("..") || key.contains("//") {
        return Err("pass key may not contain parent-directory or empty path segments".into());
    }
    if key
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'.' | b'_' | b'-'))
    {
        Ok(())
    } else {
        Err("pass key contains invalid characters".into())
    }
}

fn current_user() -> String {
    std::env::var("USER").unwrap_or_else(|_| "unknown".into())
}

fn run_deploy(
    services: Vec<String>,
    dirty: bool,
    version: Option<String>,
    from: Option<PathBuf>,
    nats_url: Option<String>,
) -> ExitCode {
    let resolved = match resolve_deploy_targets(services, dirty) {
        Ok(value) => value,
        Err(err) => {
            eprintln!("jam deploy failed: {err}");
            return ExitCode::from(1);
        }
    };

    if resolved.is_empty() {
        eprintln!("jam deploy: nothing to do (working tree is clean)");
        return ExitCode::SUCCESS;
    }

    if resolved.len() > 1 && (version.is_some() || from.is_some()) {
        eprintln!("jam deploy: --version and --from are only valid for a single service");
        return ExitCode::from(1);
    }

    if resolved.len() > 1 {
        eprintln!(
            "deploying {} services in order: {}",
            resolved.len(),
            resolved.join(", ")
        );
    }

    let mut any_failure = false;
    for (idx, service) in resolved.iter().enumerate() {
        if resolved.len() > 1 {
            eprintln!(
                "\n[{idx_plus}/{total}] {service}",
                idx_plus = idx + 1,
                total = resolved.len()
            );
        }
        let result = run_deploy_inner(
            service.clone(),
            version.clone(),
            from.clone(),
            nats_url.clone(),
        );
        match result {
            Ok(report) => {
                println!("outcome: {}", report.outcome.as_str());
                println!("service: {}", report.service);
                if !report.detail.is_empty() {
                    println!("detail: {}", report.detail);
                }
                if report.outcome != PatchOutcome::Confirmed {
                    any_failure = true;
                    eprintln!("aborting remaining deploys after non-confirmed outcome");
                    break;
                }
            }
            Err(err) => {
                eprintln!("jam deploy {service} failed: {err}");
                any_failure = true;
                break;
            }
        }
    }

    if any_failure {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

fn resolve_deploy_targets(services: Vec<String>, dirty: bool) -> Result<Vec<String>, String> {
    if dirty {
        if !services.is_empty() {
            return Err("--dirty cannot be combined with explicit service names".into());
        }
        let workspace_root = resolve_workspace_root()?;
        return discover_dirty_targets(&workspace_root);
    }
    if services.is_empty() {
        return Err(
            "specify one or more service names, or pass --dirty to infer from `git status`".into(),
        );
    }
    Ok(services)
}

/// Inspect `git status --porcelain` and return the deploy targets whose source
/// paths have uncommitted changes. Recognized prefixes:
/// - `crates/jam-svc-<name>/` → `<name>`
/// - `maestro/`               → `maestro`
/// Targets are returned in a stable order: rust services alphabetically, then
/// `maestro` last (the maestro restart is the most observable change).
fn discover_dirty_targets(workspace_root: &Path) -> Result<Vec<String>, String> {
    let output = ProcessCommand::new("git")
        .args(["status", "--porcelain", "-z"])
        .current_dir(workspace_root)
        .output()
        .map_err(|err| format!("run git status: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "git status --porcelain failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let raw = String::from_utf8(output.stdout)
        .map_err(|err| format!("git status returned invalid utf-8: {err}"))?;

    // Index every registered deploy target by its `crates/<crate_name>/` prefix
    // so the matcher is data-driven: adding a new target to the registry makes
    // `--dirty` pick it up automatically. BTreeMap ordering is stable but not
    // semantically meaningful; we re-sort at the end.
    let prefix_to_short: std::collections::BTreeMap<String, &'static str> =
        jam_tools_core::deploy_targets::DEPLOY_TARGETS
            .iter()
            .map(|t| (format!("crates/{}/", t.crate_name), t.short_name))
            .collect();

    let mut services = std::collections::BTreeSet::new();
    let mut include_maestro = false;
    for entry in raw.split('\0') {
        if entry.len() < 4 {
            continue;
        }
        // Porcelain format: "XY <path>" — XY is 2 status chars + space.
        let path = &entry[3..];
        let mut matched = false;
        for (prefix, short_name) in &prefix_to_short {
            if path.starts_with(prefix.as_str()) {
                services.insert((*short_name).to_owned());
                matched = true;
                break;
            }
        }
        if matched {
            continue;
        }
        if path.starts_with("maestro/") {
            // Only treat as deploy-affecting if it's under src/, pyproject.toml,
            // or uv.lock — test changes alone don't need a redeploy.
            let rest = &path["maestro/".len()..];
            if rest.starts_with("src/") || rest == "pyproject.toml" || rest == "uv.lock" {
                include_maestro = true;
            }
        }
    }

    let mut targets: Vec<String> = services.into_iter().collect();
    if include_maestro {
        targets.push("maestro".to_owned());
    }
    Ok(targets)
}

fn run_deploy_inner(
    service: String,
    version_override: Option<String>,
    from: Option<PathBuf>,
    nats_url: Option<String>,
) -> Result<PatchTerminalReport, String> {
    validate_service_arg(&service)?;
    let workspace_root = resolve_workspace_root()?;
    let target = jam_tools_core::deploy_targets::find(&service).ok_or_else(|| {
        format!(
            "unknown deploy target `{service}` — add it to \
             crates/jam-tools-core/src/deploy_targets.rs"
        )
    })?;

    // For PythonApp the "staged" thing is a source directory; for everything
    // else it's a cargo-built binary. Branch on what to stage.
    use jam_tools_core::deploy_targets::DeployStrategy as Strategy;
    match target.strategy {
        Strategy::AtomicSwap
        | Strategy::StopReplaceRestart { .. }
        | Strategy::CanonicalBinary { .. } => {
            let source = if let Some(path) = from {
                let path = if path.is_absolute() {
                    path
                } else {
                    std::env::current_dir()
                        .map_err(|err| format!("read cwd: {err}"))?
                        .join(path)
                };
                if !path.is_file() {
                    return Err(format!("--from path is not a file: {}", path.display()));
                }
                path
            } else {
                cargo_build_release_crate(&workspace_root, target.crate_name)?;
                workspace_root
                    .join("target")
                    .join("release")
                    .join(target.binary_name)
            };
            if !source.is_file() {
                return Err(format!(
                    "expected service binary at {} after build",
                    source.display()
                ));
            }
            let version = match version_override {
                Some(value) => {
                    validate_version_arg(&value)?;
                    value
                }
                None => compute_deploy_version(&workspace_root, Some(&source))?,
            };
            eprintln!(
                "publishing patch.staged for {service} {version} from {}",
                source.display()
            );
            run_patch_apply(service, version, Some(source), nats_url, 60)
        }
        Strategy::PythonApp { .. } => {
            if from.is_some() {
                return Err(
                    "--from is not supported for PythonApp targets (no prebuilt artifact)".into(),
                );
            }
            let source_dir = workspace_root.join("maestro");
            if !source_dir.join("pyproject.toml").is_file() {
                return Err(format!(
                    "expected Python app source at {} (missing pyproject.toml)",
                    source_dir.display()
                ));
            }
            let version = match version_override {
                Some(value) => {
                    validate_version_arg(&value)?;
                    value
                }
                None => compute_deploy_version(&workspace_root, None)?,
            };
            eprintln!(
                "publishing patch.staged for {service} {version} from {}",
                source_dir.display()
            );
            run_patch_apply_python_app(service, version, source_dir, nats_url, 90)
        }
    }
}

fn resolve_workspace_root() -> Result<PathBuf, String> {
    let output = ProcessCommand::new("cargo")
        .args(["locate-project", "--workspace", "--message-format", "plain"])
        .output()
        .map_err(|err| format!("run cargo locate-project: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "cargo locate-project failed (exit {}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let path = String::from_utf8(output.stdout)
        .map_err(|err| format!("cargo locate-project returned invalid utf-8: {err}"))?;
    let cargo_toml = PathBuf::from(path.trim());
    cargo_toml
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| format!("cargo locate-project returned a root path: {}", path.trim()))
}

fn cargo_build_release_crate(workspace_root: &Path, crate_name: &str) -> Result<(), String> {
    eprintln!("building {crate_name} (cargo build --release)");
    let status = ProcessCommand::new("cargo")
        .args(["build", "--release", "-p"])
        .arg(crate_name)
        .current_dir(workspace_root)
        .status()
        .map_err(|err| format!("run cargo build -p {crate_name}: {err}"))?;
    if !status.success() {
        return Err(format!("cargo build -p {crate_name} failed: {status}"));
    }
    Ok(())
}

// Note: maestro / jam-cli deploys used to run synchronously in the CLI via
// helpers like `run_deploy_maestro`, `run_as_maestro`, `wait_for_process_running`,
// `find_uv_bin`, and `parse_process_status`. That logic now lives in
// `jam-patch-agent`'s PythonApp / CanonicalBinary strategies, so the CLI just
// publishes patch.staged and waits like every other target — see
// `run_deploy_inner`'s strategy match.

fn compute_deploy_version(
    workspace_root: &Path,
    binary_path: Option<&Path>,
) -> Result<String, String> {
    let cargo_toml = workspace_root.join("Cargo.toml");
    let raw = fs::read_to_string(&cargo_toml)
        .map_err(|err| format!("read {}: {err}", cargo_toml.display()))?;
    let base = parse_workspace_version(&raw).ok_or_else(|| {
        format!(
            "[workspace.package].version not found in {}",
            cargo_toml.display()
        )
    })?;
    if git_workspace_clean(workspace_root)? {
        return Ok(base);
    }
    let sha = git_short_sha(workspace_root)?;
    // When the working tree is dirty, include a short hash of the binary's
    // contents so different builds get distinct versions. Without this, two
    // back-to-back deploys produce the same `0.1.0-<sha>-dirty` string and the
    // routing manifest rejects the second as "already current". The content
    // hash also makes idempotent rebuilds (cargo cached, no work done)
    // genuinely idempotent: same input → same version → no-op.
    let base_dirty = format!("{base}-{sha}-dirty");
    if let Some(path) = binary_path {
        let content_hash = sha256_file_hex(path)?;
        let short = &content_hash[..content_hash.len().min(7)];
        Ok(format!("{base_dirty}-{short}"))
    } else {
        Ok(base_dirty)
    }
}

fn parse_workspace_version(raw: &str) -> Option<String> {
    let mut in_workspace_package = false;
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_workspace_package = trimmed == "[workspace.package]";
            continue;
        }
        if in_workspace_package && trimmed.starts_with("version") {
            let after_eq = trimmed.split_once('=')?.1.trim();
            let stripped = after_eq.strip_prefix('"')?;
            let value = stripped.split('"').next()?;
            if !value.is_empty() {
                return Some(value.to_owned());
            }
        }
    }
    None
}

fn git_workspace_clean(workspace_root: &Path) -> Result<bool, String> {
    let output = ProcessCommand::new("git")
        .args(["status", "--porcelain"])
        .current_dir(workspace_root)
        .output()
        .map_err(|err| format!("run git status: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "git status --porcelain failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(output.stdout.iter().all(u8::is_ascii_whitespace))
}

fn git_short_sha(workspace_root: &Path) -> Result<String, String> {
    let output = ProcessCommand::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(workspace_root)
        .output()
        .map_err(|err| format!("run git rev-parse: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "git rev-parse --short HEAD failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    String::from_utf8(output.stdout)
        .map(|value| value.trim().to_owned())
        .map_err(|err| format!("git rev-parse returned invalid utf-8: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::DateTime;
    use tempfile::TempDir;

    fn write_dirty_repo(files: &[&str]) -> TempDir {
        let tmp = TempDir::new().unwrap();
        let repo = tmp.path();
        for (args, expect) in [
            (vec!["init", "-q", "-b", "main"], "git init"),
            (
                vec!["config", "user.email", "test@example.com"],
                "git config email",
            ),
            (vec!["config", "user.name", "test"], "git config name"),
            (
                vec!["config", "commit.gpgsign", "false"],
                "git config gpgsign",
            ),
        ] {
            let status = ProcessCommand::new("git")
                .args(&args)
                .current_dir(repo)
                .status()
                .unwrap();
            assert!(status.success(), "{expect} failed");
        }
        // Stage and commit a baseline so files show as " M" (modified) rather
        // than "??" (untracked) — discover_dirty_targets is intended for the
        // tracked-modification case.
        for relpath in files {
            let path = repo.join(relpath);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, b"base\n").unwrap();
        }
        ProcessCommand::new("git")
            .args(["add", "-A"])
            .current_dir(repo)
            .status()
            .unwrap();
        ProcessCommand::new("git")
            .args(["commit", "-q", "-m", "baseline"])
            .current_dir(repo)
            .status()
            .unwrap();
        for relpath in files {
            std::fs::write(repo.join(relpath), b"modified\n").unwrap();
        }
        tmp
    }

    #[test]
    fn discover_dirty_targets_collects_rust_services_and_maestro() {
        let tmp = write_dirty_repo(&[
            "crates/jam-svc-worktree/src/main.rs",
            "crates/jam-svc-session/src/main.rs",
            "maestro/src/jam_maestro/dispatch.py",
            "docs/proposal-v5.md", // ignored
        ]);
        let targets = discover_dirty_targets(tmp.path()).unwrap();
        assert_eq!(targets, vec!["session", "worktree", "maestro"]);
    }

    #[test]
    fn discover_dirty_targets_ignores_maestro_test_only_changes() {
        let tmp = write_dirty_repo(&["maestro/tests/unit/test_dispatch.py", "maestro/README.md"]);
        let targets = discover_dirty_targets(tmp.path()).unwrap();
        assert!(targets.is_empty(), "got {targets:?}");
    }

    #[test]
    fn discover_dirty_targets_includes_maestro_for_pyproject_or_uv_lock() {
        let tmp = write_dirty_repo(&["maestro/pyproject.toml"]);
        assert_eq!(discover_dirty_targets(tmp.path()).unwrap(), vec!["maestro"]);
        let tmp = write_dirty_repo(&["maestro/uv.lock"]);
        assert_eq!(discover_dirty_targets(tmp.path()).unwrap(), vec!["maestro"]);
    }

    #[test]
    fn resolve_deploy_targets_rejects_dirty_with_explicit_names() {
        let err = resolve_deploy_targets(vec!["worktree".into()], true).unwrap_err();
        assert!(err.contains("--dirty cannot be combined"));
    }

    #[test]
    fn resolve_deploy_targets_rejects_empty_without_dirty() {
        let err = resolve_deploy_targets(vec![], false).unwrap_err();
        assert!(err.contains("specify one or more service names"));
    }

    #[test]
    fn resolve_deploy_targets_passes_explicit_services_through() {
        let targets =
            resolve_deploy_targets(vec!["worktree".into(), "maestro".into()], false).unwrap();
        assert_eq!(targets, vec!["worktree", "maestro"]);
    }

    #[test]
    fn parse_workspace_version_reads_quoted_value() {
        let raw = r#"
[workspace]
members = ["crates/*"]

[workspace.package]
version = "0.4.7"
edition = "2021"
"#;
        assert_eq!(parse_workspace_version(raw), Some("0.4.7".into()));
    }

    #[test]
    fn parse_workspace_version_ignores_inherited_keys_in_other_sections() {
        let raw = r#"
[package]
version.workspace = true

[workspace.package]
version = "1.2.3"
"#;
        assert_eq!(parse_workspace_version(raw), Some("1.2.3".into()));
    }

    #[test]
    fn parse_workspace_version_returns_none_when_only_inherited() {
        let raw = r"
[package]
version.workspace = true
";
        assert_eq!(parse_workspace_version(raw), None);
    }

    #[test]
    fn parse_workspace_version_strips_trailing_comment() {
        let raw = r#"
[workspace.package]
version = "0.9.0"   # bumped 2026-05-09
"#;
        assert_eq!(parse_workspace_version(raw), Some("0.9.0".into()));
    }

    #[test]
    fn recreate_canonical_worktree_replays_task_journal() {
        let tmp = TempDir::new().unwrap();
        let repo = tmp.path().join("blueberry");
        let worktree = tmp.path().join("blueberry-jam");
        let journal_root = tmp.path().join("journal");
        init_repo(&repo);
        write_journal(&journal_root);
        let config = CanonicalWorktreeConfig {
            repo_path: repo,
            worktree_path: worktree.clone(),
            branch: "tempyr-live".into(),
            base_ref: None,
            graph_relpath: PathBuf::from("graph"),
            journal_root,
        };

        ensure_canonical_worktree(&config).unwrap();
        fs::write(worktree.join("graph/tasks/stale.md"), "stale").unwrap();
        let outcome = recreate_canonical_worktree(&config).unwrap();

        let task = fs::read_to_string(worktree.join("graph/tasks/task-1.md")).unwrap();
        assert!(outcome.created);
        assert_eq!(outcome.replayed_events, 4);
        assert!(task.contains("status: merged"));
        assert!(task.contains("description: Test task"));
        assert!(task.contains("exit-code: '0'"));
        assert!(task.contains("merged-sha: abc123"));
        assert!(!worktree.join("graph/tasks/stale.md").exists());
    }

    #[test]
    fn ensure_canonical_worktree_creates_missing_branch_from_base_ref() {
        let tmp = TempDir::new().unwrap();
        let repo = tmp.path().join("blueberry");
        let worktree = tmp.path().join("blueberry-jam");
        init_repo(&repo);
        git_test(&repo, &["branch", "-D", "tempyr-live"]);
        let config = CanonicalWorktreeConfig {
            repo_path: repo.clone(),
            worktree_path: worktree.clone(),
            branch: "tempyr-live".into(),
            base_ref: Some("HEAD".into()),
            graph_relpath: PathBuf::from("graph"),
            journal_root: tmp.path().join("journal"),
        };

        let outcome = ensure_canonical_worktree(&config).unwrap();

        assert!(outcome.created);
        assert!(worktree.join(".git").exists());
        assert!(worktree.join("graph/tasks").is_dir());
        git_test(&repo, &["show-ref", "--verify", "refs/heads/tempyr-live"]);
    }

    #[test]
    fn git_ref_validation_rejects_lockfile_shaped_segments() {
        assert!(validate_git_ref("TEST_REF", "origin/master").is_ok());
        assert!(validate_git_ref("TEST_REF", "feature/foo.lock").is_err());
        assert!(validate_git_ref("TEST_REF", "feature/FOO.LOCK/bar").is_err());
    }

    #[test]
    fn recreate_missing_canonical_worktree_preserves_checked_out_tasks() {
        let tmp = TempDir::new().unwrap();
        let repo = tmp.path().join("blueberry");
        let worktree = tmp.path().join("blueberry-jam");
        init_repo(&repo);
        let config = CanonicalWorktreeConfig {
            repo_path: repo,
            worktree_path: worktree.clone(),
            branch: "tempyr-live".into(),
            base_ref: None,
            graph_relpath: PathBuf::from("graph"),
            journal_root: tmp.path().join("journal"),
        };

        recreate_canonical_worktree(&config).unwrap();

        assert!(worktree.join("graph/tasks/.gitkeep").exists());
    }

    #[test]
    fn trace_replay_walks_parent_chain_from_journal() {
        let tmp = TempDir::new().unwrap();
        let journal_root = tmp.path().join("journal");
        let day = journal_root.join("2026-05-06");
        fs::create_dir_all(&day).unwrap();
        let root = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
        let child = "01BRZ3NDEKTSV4RRFFQ69G5FAV";
        let unrelated = "01CRZ3NDEKTSV4RRFFQ69G5FAV";
        fs::write(
            day.join("journal.maestro.jsonl"),
            format!(
                "{}\n{}\n",
                envelope_with_trace(
                    "maestro.session-started",
                    root,
                    None,
                    1,
                    serde_json::json!({"session_id": "maestro-1"})
                ),
                envelope_with_trace(
                    "maestro.session-started",
                    unrelated,
                    None,
                    1,
                    serde_json::json!({"session_id": "maestro-other"})
                )
            ),
        )
        .unwrap();
        fs::write(
            day.join("journal.picker.jsonl"),
            format!(
                "{}\n",
                envelope_with_trace(
                    "picker.spawned",
                    child,
                    Some(root),
                    2,
                    serde_json::json!({
                        "task_id": "task-1",
                        "session_id": "codex-cli:abc"
                    })
                )
            ),
        )
        .unwrap();

        let replay = trace_replay_from_journal(&journal_root, child, 5).unwrap();

        assert_eq!(replay.chain, vec![child.to_owned(), root.to_owned()]);
        assert_eq!(replay.entries.len(), 2);
        assert_eq!(replay.entries[0].envelope.trace_id, root);
        assert_eq!(replay.entries[1].envelope.trace_id, child);
        assert!(replay
            .entries
            .iter()
            .all(|entry| entry.envelope.trace_id != unrelated));
    }

    #[test]
    fn trace_replay_rejects_invalid_trace_ids() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join("journal")).unwrap();

        let error =
            trace_replay_from_journal(&tmp.path().join("journal"), "not-a-trace", 5).unwrap_err();

        assert!(error.contains("invalid trace id"));
    }

    #[test]
    fn dispatch_paused_bool_parser_is_strict() {
        assert!(parse_dispatch_paused(b"true").unwrap());
        assert!(!parse_dispatch_paused(b"false\n").unwrap());
        assert!(parse_dispatch_paused(b"yes").is_err());
    }

    #[test]
    fn patch_args_reject_path_segments() {
        assert!(validate_service_arg("observe").is_ok());
        assert!(validate_service_arg("bad.service").is_err());
        assert!(validate_version_arg("0.4.7").is_ok());
        assert!(validate_version_arg("../bad").is_err());
    }

    #[test]
    fn health_ping_response_requires_ok_status_and_expected_service() {
        let response = serde_json::json!({
            "status": "ok",
            "service": "jam-svc-observe",
            "version": "0.1.0",
        });

        let parsed = parse_health_ping_response("observe", "tool.observe.ping", &response).unwrap();

        assert_eq!(parsed.status, "ok");
        assert_eq!(parsed.service, "jam-svc-observe");
        assert_eq!(parsed.version, "0.1.0");
    }

    #[test]
    fn health_ping_response_rejects_wrong_service() {
        let response = serde_json::json!({
            "status": "ok",
            "service": "jam-svc-session",
            "version": "0.1.0",
        });

        let err =
            parse_health_ping_response("observe", "tool.observe.ping", &response).unwrap_err();

        assert!(err.contains("expected jam-svc-observe"));
    }

    #[test]
    fn quota_query_response_parses_map_sorted_by_window() {
        let response = serde_json::json!({
            "opencode-deepseek/api-budget": {
                "status": "available",
                "detail": "opencode-deepseek api-budget quota refilled or limit cleared",
                "window_kind": "api-budget",
                "source": "journal.quota.refilled",
                "remaining": 0.75,
                "api_budget": {
                    "provider": "deepseek",
                    "model": "deepseek-v4-pro",
                    "monthly_cap_usd": 20.0,
                    "spent_this_month_usd": 5.0,
                    "current_input_rate_per_1m": 0.14,
                    "current_output_rate_per_1m": 0.28,
                    "rate_limit_state": "available"
                },
                "usage": {
                    "provider": "deepseek",
                    "model": "deepseek-v4-pro",
                    "input_tokens": 1000,
                    "output_tokens": 250,
                    "cost_usd": 0.50,
                    "last_source": "opencode-json",
                    "last_observed_at": "2026-05-06T10:04:00Z"
                },
                "price_events": [{
                    "name": "deepseek-sale",
                    "provider": "deepseek",
                    "model": "deepseek-v4-pro",
                    "ends_at": "2026-05-31T15:59:00Z",
                    "input_rate_per_1m": 0.14,
                    "output_rate_per_1m": 0.28
                }],
                "observed_at": "2026-05-06T10:05:00Z"
            },
            "codex-cli/local-messages": {
                "status": "exhausted",
                "detail": "codex-cli local-messages quota exhausted",
                "window_kind": "local-messages",
                "source": "journal.quota.exhausted",
                "remaining": 0.0,
                "resets_at": "2026-05-06T15:00:00Z",
                "reset_cadence": {
                    "cadence_secs": 18000,
                    "next_reset_at": "2026-05-06T15:00:00Z",
                    "limit_in_window": 300,
                    "multiplier": 1.0
                },
                "observed_at": "2026-05-06T10:00:00Z"
            }
        });

        let windows =
            parse_query_quota_response(response, None, "tool.observe.query-quota").unwrap();

        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0].key, "codex-cli/local-messages");
        assert_eq!(windows[0].state.status, "exhausted");
        assert_eq!(windows[0].state.window_kind, "local-messages");
        assert_eq!(format_remaining(windows[0].state.remaining), "0.0%");
        assert_eq!(
            format_optional_datetime(windows[0].state.resets_at),
            "2026-05-06T15:00:00Z"
        );
        assert_eq!(
            format_reset_cadence(windows[0].state.reset_cadence.as_ref()),
            "18000s,next=2026-05-06T15:00:00Z,limit=300,multiplier=1.00"
        );
        assert_eq!(windows[1].key, "opencode-deepseek/api-budget");
        assert_eq!(
            format_api_budget(windows[1].state.api_budget.as_ref()),
            "deepseek:deepseek-v4-pro $5.00/$20.00 in=0.1400/1M out=0.2800/1M rate-limit=available"
        );
        assert_eq!(
            format_usage(windows[1].state.usage.as_ref()),
            "provider=deepseek,model=deepseek-v4-pro,in=1000,out=250,cost=$0.5000,source=opencode-json,at=2026-05-06T10:04:00Z"
        );
        assert_eq!(
            format_price_events(&windows[1].state.price_events),
            "deepseek-sale,provider=deepseek,model=deepseek-v4-pro,ends=2026-05-31T15:59:00Z,in=0.1400/1M,out=0.2800/1M"
        );
    }

    #[test]
    fn quota_query_response_parses_single_window() {
        let response = serde_json::json!({
            "status": "low",
            "detail": "codex-cli local-messages quota at 8.0% remaining",
            "window_kind": "local-messages",
            "source": "journal.quota.exhausted-soon",
            "remaining": 0.08,
            "observed_at": "2026-05-06T10:00:00Z"
        });

        let windows = parse_query_quota_response(
            response,
            Some("codex-cli/local-messages"),
            "tool.observe.query-quota",
        )
        .unwrap();

        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].key, "codex-cli/local-messages");
        assert_eq!(windows[0].state.status, "low");
        assert_eq!(format_remaining(windows[0].state.remaining), "8.0%");
    }

    #[test]
    fn quota_query_response_surfaces_tool_errors() {
        let response = serde_json::json!({
            "error": {
                "kind": "quota-not-found",
                "detail": "no quota state found for harness codex-cli",
                "tracked_by": "task-quota-tracker-three-shapes"
            }
        });

        let err =
            parse_query_quota_response(response, Some("codex-cli"), "tool.observe.query-quota")
                .unwrap_err();

        assert!(err.contains("quota-not-found"));
        assert!(err.contains("task-quota-tracker-three-shapes"));
    }

    #[test]
    fn quota_filter_trims_and_rejects_empty_values() {
        assert_eq!(
            normalize_quota_filter(Some(" codex-cli ".into())).unwrap(),
            Some("codex-cli".into())
        );
        assert!(normalize_quota_filter(Some("  ".into())).is_err());
    }

    #[test]
    fn quota_recalibration_builds_refilled_event() {
        let now = DateTime::parse_from_rfc3339("2026-05-06T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        let payload = build_quota_recalibration_payload(
            "opencode-deepseek".into(),
            "api-budget".into(),
            QuotaRecalibrateStatus::Available,
            None,
            None,
            now,
        )
        .unwrap();

        assert_eq!(payload.event_type(), QuotaRefilled::EVENT_TYPE);
        match payload {
            QuotaRecalibrationPayload::Refilled(payload) => {
                assert_eq!(payload.harness, "opencode-deepseek");
                assert_eq!(payload.window_kind, "api-budget");
                assert_eq!(payload.ts, now);
            }
            _ => panic!("expected quota.refilled payload"),
        }
    }

    #[test]
    fn quota_recalibration_validates_low_remaining_fraction() {
        let now = DateTime::parse_from_rfc3339("2026-05-06T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        assert!(build_quota_recalibration_payload(
            "codex-cli".into(),
            "local-messages".into(),
            QuotaRecalibrateStatus::Low,
            None,
            None,
            now,
        )
        .is_err());
        assert!(build_quota_recalibration_payload(
            "codex-cli".into(),
            "local-messages".into(),
            QuotaRecalibrateStatus::Low,
            Some(1.5),
            None,
            now,
        )
        .is_err());

        let payload = build_quota_recalibration_payload(
            "codex-cli".into(),
            "local-messages".into(),
            QuotaRecalibrateStatus::Low,
            Some(0.08),
            None,
            now,
        )
        .unwrap();

        assert_eq!(payload.event_type(), QuotaExhaustedSoon::EVENT_TYPE);
        match payload {
            QuotaRecalibrationPayload::ExhaustedSoon(payload) => {
                assert!((payload.remaining - 0.08).abs() < f64::EPSILON);
            }
            _ => panic!("expected quota.exhausted-soon payload"),
        }
    }

    #[test]
    fn quota_recalibration_exhausted_accepts_reset_time() {
        let now = DateTime::parse_from_rfc3339("2026-05-06T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let reset = DateTime::parse_from_rfc3339("2026-05-06T15:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        let payload = build_quota_recalibration_payload(
            "codex-cli".into(),
            "local-messages".into(),
            QuotaRecalibrateStatus::Exhausted,
            None,
            Some(reset),
            now,
        )
        .unwrap();

        assert_eq!(payload.event_type(), QuotaExhausted::EVENT_TYPE);
        match payload {
            QuotaRecalibrationPayload::Exhausted(payload) => {
                assert_eq!(payload.resets_at, Some(reset));
                assert_eq!(payload.detected_at, now);
            }
            _ => panic!("expected quota.exhausted payload"),
        }
    }

    #[test]
    fn quota_recalibration_rejects_misplaced_fields() {
        let now = DateTime::parse_from_rfc3339("2026-05-06T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let reset = DateTime::parse_from_rfc3339("2026-05-06T15:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        assert!(build_quota_recalibration_payload(
            "codex-cli".into(),
            "local-messages".into(),
            QuotaRecalibrateStatus::Available,
            None,
            Some(reset),
            now,
        )
        .is_err());
        assert!(build_quota_recalibration_payload(
            "codex-cli".into(),
            "local-messages".into(),
            QuotaRecalibrateStatus::Exhausted,
            Some(0.0),
            None,
            now,
        )
        .is_err());
    }

    #[cfg(unix)]
    #[test]
    fn maestro_pass_bridge_uses_noninteractive_sudo() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = TempDir::new().unwrap();
        let sudo = tmp.path().join("sudo");
        fs::write(
            &sudo,
            r#"#!/bin/sh
dir=$(dirname "$0")
printf '%s\n' "$@" > "$dir/args"
printf 'nats-token\n'
"#,
        )
        .unwrap();
        let mut permissions = fs::metadata(&sudo).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&sudo, permissions).unwrap();

        let token = read_maestro_pass_secret_with_sudo(&sudo, MAESTRO_NATS_TOKEN_KEY).unwrap();

        assert_eq!(token, "nats-token");
        let args = fs::read_to_string(tmp.path().join("args")).unwrap();
        assert_eq!(
            args.lines().collect::<Vec<_>>(),
            vec![
                "-n",
                "-u",
                "maestro",
                "-i",
                "pass",
                "show",
                "jam/nats/token"
            ]
        );
    }

    #[test]
    fn pass_key_validation_rejects_unsafe_paths() {
        assert!(validate_pass_key("jam/nats/token").is_ok());
        assert!(validate_pass_key("../jam/nats/token").is_err());
        assert!(validate_pass_key("/jam/nats/token").is_err());
        assert!(validate_pass_key("-bad").is_err());
        assert!(validate_pass_key("jam//nats/token").is_err());
        assert!(validate_pass_key("jam/nats/token;cat").is_err());
    }

    #[test]
    fn maestro_resume_writes_request_from_abort_dump() {
        let tmp = TempDir::new().unwrap();
        let session_id = "maestro-2026-05-06-test";
        let dump_path = aborted_maestro_session_path_in(tmp.path(), session_id).unwrap();
        fs::create_dir_all(dump_path.parent().unwrap()).unwrap();
        fs::write(
            &dump_path,
            serde_json::to_string_pretty(&serde_json::json!({
                "schema_version": 1,
                "session_id": session_id,
                "trace_id": "01HXKJ00000000000000000000",
                "reason": "per-session-usd-exceeded-125pct",
                "spent_usd": 6.27,
                "budget_usd": 5.0,
                "messages_in_session": [],
            }))
            .unwrap(),
        )
        .unwrap();

        let outcome = resume_maestro_session_in(tmp.path(), session_id, 5.0).unwrap();

        assert_eq!(
            outcome.request_path,
            tmp.path()
                .join("maestro-resume-requests")
                .join(format!("{session_id}.json"))
        );
        assert_eq!(outcome.request.session_id, session_id);
        assert!((outcome.request.budget_extension_usd - 5.0).abs() < f64::EPSILON);
        assert_eq!(
            outcome.request.dump["reason"],
            "per-session-usd-exceeded-125pct"
        );
        assert!(dump_path.exists());

        let written = read_json_file(&outcome.request_path).unwrap();
        assert_eq!(written["session_id"], session_id);
        assert_eq!(written["dump"]["session_id"], session_id);
    }

    #[test]
    fn maestro_resume_rejects_bad_session_id_and_budget() {
        let tmp = TempDir::new().unwrap();

        assert!(resume_maestro_session_in(tmp.path(), "../bad", 1.0).is_err());
        assert!(resume_maestro_session_in(tmp.path(), "maestro-good", 0.0).is_err());
        assert!(resume_maestro_session_in(tmp.path(), "maestro-good", f64::NAN).is_err());
    }

    #[test]
    fn maestro_abandon_removes_dump_and_pending_resume_request() {
        let tmp = TempDir::new().unwrap();
        let session_id = "maestro-2026-05-06-test";
        let dump_path = aborted_maestro_session_path_in(tmp.path(), session_id).unwrap();
        let request_path = maestro_resume_request_path_in(tmp.path(), session_id).unwrap();
        fs::create_dir_all(dump_path.parent().unwrap()).unwrap();
        fs::create_dir_all(request_path.parent().unwrap()).unwrap();
        fs::write(&dump_path, "{}").unwrap();
        fs::write(&request_path, "{}").unwrap();

        let outcome = abandon_maestro_session_in(tmp.path(), session_id).unwrap();

        assert_eq!(outcome.dump_path, dump_path);
        assert!(outcome.resume_request_removed);
        assert!(!dump_path.exists());
        assert!(!request_path.exists());
    }

    fn init_repo(path: &Path) {
        fs::create_dir_all(path.join("graph/tasks")).unwrap();
        fs::write(path.join("graph/tasks/.gitkeep"), "").unwrap();
        git_test(path, &["init"]);
        git_test(path, &["config", "user.email", "test@example.invalid"]);
        git_test(path, &["config", "user.name", "Jam Test"]);
        git_test(path, &["add", "."]);
        git_test(path, &["commit", "-m", "init"]);
        git_test(path, &["branch", "tempyr-live"]);
    }

    fn write_journal(root: &Path) {
        let day = root.join("2026-05-06");
        fs::create_dir_all(&day).unwrap();
        fs::write(
            day.join("journal.task.jsonl"),
            format!(
                "{}\n",
                envelope(
                    "task.requested",
                    serde_json::json!({
                        "task_id": "task-1",
                        "description": "Test task",
                        "project": "blueberry",
                        "task_class": "light-edit",
                        "priority": "normal",
                        "requested_by": "human:caleb"
                    })
                )
            ),
        )
        .unwrap();
        fs::write(
            day.join("journal.picker.jsonl"),
            format!(
                "{}\n{}\n",
                envelope(
                    "picker.spawned",
                    serde_json::json!({
                        "task_id": "task-1",
                        "session_id": "codex-cli:abc",
                        "worktree_path": "/tmp/task-1",
                        "spawned_at": "2026-05-06T05:01:00Z"
                    })
                ),
                envelope(
                    "picker.exited",
                    serde_json::json!({
                        "task_id": "task-1",
                        "session_id": "codex-cli:abc",
                        "exit_code": 0,
                        "exited_at": "2026-05-06T05:02:00Z",
                        "duration_ms": 60000
                    })
                )
            ),
        )
        .unwrap();
        fs::write(
            day.join("journal.pr.jsonl"),
            format!(
                "{}\n",
                envelope(
                    "pr.merged",
                    serde_json::json!({
                        "task_id": "task-1",
                        "pr_ref": "cleak/blueberry#42",
                        "merged_sha": "abc123",
                        "merged_by": "caleb",
                        "merged_at": "2026-05-06T05:02:00Z"
                    })
                )
            ),
        )
        .unwrap();
    }

    fn envelope(event_type: &str, payload: serde_json::Value) -> String {
        envelope_with_trace(event_type, "01ARZ3NDEKTSV4RRFFQ69G5FAV", None, 1, payload)
    }

    fn envelope_with_trace(
        event_type: &str,
        trace_id: &str,
        parent_trace_id: Option<&str>,
        journal_seq: u64,
        payload: serde_json::Value,
    ) -> String {
        let mut envelope =
            EventEnvelope::new(event_type, 1, journal_seq, trace_id, "test", payload);
        if let Some(parent_trace_id) = parent_trace_id {
            envelope = envelope.with_parent_trace(parent_trace_id);
        }
        envelope.timestamp = DateTime::parse_from_rfc3339("2026-05-06T05:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        serde_json::to_string(&envelope).unwrap()
    }

    fn git_test(repo: &Path, args: &[&str]) {
        let output = ProcessCommand::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git -C {} {} failed: {}",
            repo.display(),
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[cfg(unix)]
    fn set_executable(path: &Path) {
        let mut permissions = fs::metadata(path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).unwrap();
    }

    #[cfg(not(unix))]
    fn set_executable(_path: &Path) {}
}
