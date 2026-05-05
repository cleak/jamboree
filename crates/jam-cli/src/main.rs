//! The `jam` CLI binary — user-facing entry point.
//!
//! Per spec §11.4 + `comp-jam-cli-binary`. Phase 0 implements `setup` and
//! `doctor` against the [`jam-setup`] check set; remaining subcommands are
//! well-structured TODO stubs that name what they will do, who consumes
//! them, and which task in `graph/tasks/` tracks the work.

use clap::{Parser, Subcommand};
use jam_setup::{CheckSeverity, CheckStatus};
use std::process::ExitCode;

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
    Doctor,

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
    },

    /// Resume spawning after pause.
    ResumeDispatch,

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
    },
    /// List active tasks.
    List,
    /// Show task detail.
    Show {
        /// Task ID.
        task_id: String,
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
    },
}

#[derive(Subcommand)]
enum QuotaAction {
    /// Show current quota state across all harnesses.
    Show,
    /// Recalibrate from observed limit responses.
    Recalibrate,
}

#[derive(Subcommand)]
enum PatchAction {
    /// Apply a staged patch.
    Apply {
        /// Tool service name (e.g. observe, session).
        service: String,
        /// New version string.
        version: String,
    },
}

#[derive(Subcommand)]
enum UiAction {
    /// Issue a new session token.
    Token,
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
    /// Recreate the canonical Tempyr worktree from origin.
    CanonicalWorktreeRecreate,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Setup => run_setup(),
        Command::Doctor => run_doctor(),
        Command::Task { action } => stub(
            "jam task",
            task_action_label(&action),
            "task-cli-task-spawn-list-show",
        ),
        Command::Trace { action } => stub(
            "jam trace",
            trace_action_label(&action),
            "task-trace-replay-tool-prove",
        ),
        Command::Quota { action } => stub(
            "jam quota",
            quota_action_label(&action),
            "task-quota-tracker-three-shapes",
        ),
        Command::Patch { action } => stub(
            "jam patch",
            patch_action_label(&action),
            "task-routing-manifest-schema",
        ),
        Command::Ui { action } => stub(
            "jam ui",
            ui_action_label(&action),
            "task-session-token-auth-impl",
        ),
        Command::PauseDispatch { reason } => stub(
            "jam pause-dispatch",
            &format!("reason: {reason}"),
            "task-dispatch-pause-resume",
        ),
        Command::ResumeDispatch => stub("jam resume-dispatch", "", "task-dispatch-pause-resume"),
        Command::Maestro { action } => stub(
            "jam maestro",
            maestro_action_label(&action),
            "task-hard-abort-dump-and-resume",
        ),
        Command::Tempyr { action } => stub(
            "jam tempyr",
            tempyr_action_label(&action),
            "task-tempyr-canonical-worktree-bootstrap",
        ),
    }
}

fn run_setup() -> ExitCode {
    print_header("jam setup — preflight checks");
    print_run_outcomes(true)
}

fn run_doctor() -> ExitCode {
    print_header("jam doctor — environment health");
    print_run_outcomes(false)
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

fn stub(cmd: &str, action: &str, task_id: &str) -> ExitCode {
    eprintln!();
    eprintln!("\x1b[33m{cmd} {action}\x1b[0m: not yet implemented.");
    eprintln!("  Tracked by graph/tasks/{task_id}.md");
    eprintln!();
    ExitCode::from(2)
}

fn task_action_label(a: &TaskAction) -> &'static str {
    match a {
        TaskAction::Spawn { .. } => "spawn",
        TaskAction::List => "list",
        TaskAction::Show { .. } => "show",
        TaskAction::Cleanup => "cleanup",
    }
}

fn trace_action_label(a: &TraceAction) -> &'static str {
    match a {
        TraceAction::Replay { .. } => "replay",
        TraceAction::Find { .. } => "find",
    }
}

fn quota_action_label(a: &QuotaAction) -> &'static str {
    match a {
        QuotaAction::Show => "show",
        QuotaAction::Recalibrate => "recalibrate",
    }
}

fn patch_action_label(a: &PatchAction) -> &'static str {
    match a {
        PatchAction::Apply { .. } => "apply",
    }
}

fn ui_action_label(a: &UiAction) -> &'static str {
    match a {
        UiAction::Token => "token",
        UiAction::TokenRevoke { .. } => "token-revoke",
        UiAction::TokenRevokeAll => "token-revoke-all",
    }
}

fn maestro_action_label(a: &MaestroAction) -> &'static str {
    match a {
        MaestroAction::Resume { .. } => "resume",
        MaestroAction::Abandon { .. } => "abandon",
    }
}

fn tempyr_action_label(a: &TempyrAction) -> &'static str {
    match a {
        TempyrAction::CanonicalWorktreeRecreate => "canonical-worktree-recreate",
    }
}
