//! Post-picker coordination: on `picker.exited`, decide whether to call
//! `tool.repo.open-pr` or emit `picker.continuation-needed`.
//!
//! See `graph/decisions/dec-post-picker-coordination.md`. Principle §2.1
//! (more observable, not more deterministic): the act of leaving a clean
//! worktree with `.jam/pr-*` metadata + commits ahead of trunk is the
//! signal that a Picker is ready to ship — not a Picker self-declaration.

use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use chrono::Utc;
use jam_events::generated::PickerContinuationNeeded;
use jam_events::Event;
use jam_nats::JamNats;
use jam_trace::TraceCtx;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use crate::JournalEnvelope;

const OPEN_PR_SUBJECT: &str = "tool.repo.open-pr";
const RESUME_PICKER_SUBJECT: &str = "tool.session.resume-picker";
const CONTINUATION_SUBJECT: &str = "journal.picker.continuation-needed";
const OPEN_PR_TIMEOUT: Duration = Duration::from_secs(60);
const RESUME_PICKER_TIMEOUT: Duration = Duration::from_secs(30);
const CONTINUATION_ATTEMPT_CAP: u32 = 3;
const PICKER_USER_ENV: &str = "JAM_PICKER_USER";
const DEFAULT_PICKER_USER: &str = "picker";
const DEFAULT_BASE_ENV: &str = "JAM_TRUNK_BRANCH";
const DEFAULT_BASE: &str = "master";
const SUDO_BIN_ENV: &str = "JAM_SUDO_BIN";
const DEFAULT_SUDO_BIN: &str = "/usr/bin/sudo";

#[derive(Debug, Clone)]
struct ExitedEvent {
    task_id: String,
    session_id: String,
    exit_code: u32,
    worktree_path: PathBuf,
    branch: String,
}

enum CheckOutcome {
    Ready { title: String, body: String },
    NeedsContinuation { reason: &'static str, detail: String },
}

#[derive(Debug, Serialize)]
struct OpenPrRequest<'a> {
    task_id: &'a str,
    branch: &'a str,
    title: &'a str,
    body: &'a str,
    draft: bool,
    base: &'a str,
    worktree_path: &'a str,
    push: bool,
}

#[derive(Debug, Deserialize)]
struct OpenPrResponse {
    #[serde(default)]
    pr_ref: Option<String>,
    #[serde(default)]
    error: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
struct ResumePickerRequest<'a> {
    task_id: &'a str,
    prompt: &'a str,
    parent_session_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    task_class: Option<&'a str>,
}

#[derive(Debug, Deserialize)]
struct ResumePickerResponse {
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    error: Option<serde_json::Value>,
}

/// Handle a `pr.review-received` event: emit `picker.continuation-needed`
/// so the Picker addresses the new review activity in its worktree. The
/// follow-up commits flow back to the same PR branch.
pub async fn handle_pr_review_received(
    nats: &JamNats,
    envelope: &JournalEnvelope,
    ctx: &TraceCtx,
) {
    let task_id = envelope
        .payload
        .get("task_id")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let pr_ref = envelope
        .payload
        .get("pr_ref")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let reviewer = envelope
        .payload
        .get("reviewer")
        .and_then(|v| v.as_str())
        .unwrap_or("a reviewer");
    if task_id.is_empty() || pr_ref.is_empty() {
        return;
    }
    let event = synth_exited_event(task_id);
    let detail = format!("PR {pr_ref} received new review activity from {reviewer}.");
    let prompt = format!(
        "PR `{pr_ref}` for task `{task_id}` received new review comments from `{reviewer}`.\n\
         \n\
         Use the orchestrator's `read-pr-comments` tool to list the new comments, then address each one in your worktree:\n\
         - For substantive feedback: change the code, add tests, or update docs as appropriate.\n\
         - For each comment you address: leave a brief reply via `reply-to-comment` and call `mark-review-artifact-handled` with status=Addressed.\n\
         - For comments you decline to act on: reply explaining why and mark them Acknowledged/Dismissed.\n\
         \n\
         Make follow-up commits on the existing branch (do NOT amend or force-push). When your worktree is clean and `.jam/pr-*` reflects the cumulative change, exit.",
    );
    publish_continuation(nats, &event, "review-received", &detail, &prompt, ctx).await;
}

/// Handle a `pr.ci.status-changed` event for a failing CI run: emit
/// `picker.continuation-needed` so the Picker investigates the failure.
pub async fn handle_pr_ci_status_changed(
    nats: &JamNats,
    envelope: &JournalEnvelope,
    ctx: &TraceCtx,
) {
    let task_id = envelope
        .payload
        .get("task_id")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let pr_ref = envelope
        .payload
        .get("pr_ref")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let ci_status = envelope
        .payload
        .get("ci_status")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if task_id.is_empty() || pr_ref.is_empty() {
        return;
    }
    if !ci_status_is_failure(ci_status) {
        return;
    }
    let event = synth_exited_event(task_id);
    let detail = format!("PR {pr_ref} CI status is {ci_status}.");
    let prompt = format!(
        "CI on PR `{pr_ref}` for task `{task_id}` reported `{ci_status}`.\n\
         \n\
         From your worktree, run `gh pr checks` to see which check(s) failed, then inspect the failing job's logs (`gh run view --log-failed`). Reproduce the failure locally, fix it, and commit on the existing branch. Do NOT amend or force-push.\n\
         \n\
         When the local build/test gates pass again and the worktree is clean, exit. The orchestrator will push your follow-up commits to the existing PR.",
    );
    publish_continuation(nats, &event, "ci-failed", &detail, &prompt, ctx).await;
}

fn ci_status_is_failure(status: &str) -> bool {
    matches!(
        status.to_ascii_lowercase().as_str(),
        "failure" | "failed" | "error" | "cancelled" | "timed_out" | "action_required"
    )
}

/// Build an [`ExitedEvent`] shell for synthesizing a continuation when we
/// don't have the original picker.exited in scope (PR feedback paths). The
/// session_id is left as a placeholder; the resume tool only needs task_id
/// to find the worktree.
fn synth_exited_event(task_id: &str) -> ExitedEvent {
    let worktree_root = std::env::var_os("JAM_WORKTREE_ROOT")
        .map_or_else(|| PathBuf::from("/home/picker/workers"), PathBuf::from);
    ExitedEvent {
        task_id: task_id.to_owned(),
        session_id: format!("synthetic:{task_id}"),
        exit_code: 0,
        worktree_path: worktree_root.join(task_id),
        branch: format!("task/{task_id}"),
    }
}

/// Handle a `picker.continuation-needed` event by calling
/// `tool.session.resume-picker`. Guards against runaway continuation loops
/// by capping `attempt` at [`CONTINUATION_ATTEMPT_CAP`]; beyond the cap the
/// event is left in the journal for human triage.
pub async fn handle_continuation_needed(
    nats: &JamNats,
    envelope: &JournalEnvelope,
    ctx: &TraceCtx,
) {
    let task_id = envelope
        .payload
        .get("task_id")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let session_id = envelope
        .payload
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let reason = envelope
        .payload
        .get("reason")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let prompt = envelope
        .payload
        .get("prompt")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let attempt = envelope
        .payload
        .get("attempt")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;

    if task_id.is_empty() || session_id.is_empty() || prompt.is_empty() {
        warn!("picker.continuation-needed payload missing required fields; ignoring");
        return;
    }

    if attempt >= CONTINUATION_ATTEMPT_CAP {
        warn!(
            task = %task_id,
            reason = %reason,
            attempt = attempt,
            "continuation-needed exceeded attempt cap; leaving for human triage",
        );
        return;
    }

    let request = ResumePickerRequest {
        task_id,
        prompt,
        parent_session_id: session_id,
        task_class: None,
    };
    info!(
        task = %task_id,
        reason = %reason,
        attempt = attempt,
        "dispatching resume-picker",
    );
    let response: Result<ResumePickerResponse, _> = nats
        .request_traced(RESUME_PICKER_SUBJECT, &request, ctx, RESUME_PICKER_TIMEOUT)
        .await;
    match response {
        Ok(resp) if resp.error.is_none() => {
            info!(
                task = %task_id,
                new_session = %resp.session_id.unwrap_or_default(),
                parent = %session_id,
                "resume-picker succeeded",
            );
        }
        Ok(resp) => {
            warn!(
                task = %task_id,
                error = %resp.error.map(|e| e.to_string()).unwrap_or_default(),
                "resume-picker returned error envelope",
            );
        }
        Err(err) => {
            warn!(task = %task_id, error = %err, "resume-picker request failed");
        }
    }
}

/// Entry point invoked from `handle_message` after the Tempyr-node update.
pub async fn handle_picker_exited(nats: &JamNats, envelope: &JournalEnvelope, ctx: &TraceCtx) {
    if envelope.event_type != "picker.exited" {
        return;
    }
    let Some(event) = parse_exited(envelope) else {
        warn!("picker.exited payload missing required fields; skipping post-picker coordination");
        return;
    };

    let outcome = if event.exit_code != 0 {
        CheckOutcome::NeedsContinuation {
            reason: "picker-failed",
            detail: format!("picker exited with non-zero code {}", event.exit_code),
        }
    } else {
        run_pre_checks(&event)
    };

    match outcome {
        CheckOutcome::Ready { title, body } => {
            info!(
                task = %event.task_id,
                worktree = %event.worktree_path.display(),
                "post-picker pre-checks passed; requesting open-pr",
            );
            match request_open_pr(nats, &event, &title, &body, ctx).await {
                Ok(pr_ref) => info!(task = %event.task_id, pr_ref = %pr_ref, "open-pr succeeded"),
                Err(err) => {
                    warn!(task = %event.task_id, error = %err, "open-pr request failed");
                    publish_continuation(
                        nats,
                        &event,
                        "open-pr-failed",
                        &err,
                        &draft_open_pr_failure_prompt(&event, &err),
                        ctx,
                    )
                    .await;
                }
            }
        }
        CheckOutcome::NeedsContinuation { reason, detail } => {
            let prompt = draft_continuation_prompt(reason, &detail, &event);
            info!(
                task = %event.task_id,
                reason = %reason,
                "post-picker pre-checks failed; requesting continuation",
            );
            publish_continuation(nats, &event, reason, &detail, &prompt, ctx).await;
        }
    }
}

fn parse_exited(envelope: &JournalEnvelope) -> Option<ExitedEvent> {
    let task_id = envelope.payload.get("task_id")?.as_str()?.to_owned();
    let session_id = envelope.payload.get("session_id")?.as_str()?.to_owned();
    let exit_code = envelope.payload.get("exit_code")?.as_u64()? as u32;
    // picker.exited doesn't carry worktree_path; derive from task_id using
    // the same convention as jam-svc-worktree.
    let worktree_root = std::env::var_os("JAM_WORKTREE_ROOT")
        .map_or_else(|| PathBuf::from("/home/picker/workers"), PathBuf::from);
    let worktree_path = worktree_root.join(&task_id);
    let branch = format!("task/{task_id}");
    Some(ExitedEvent {
        task_id,
        session_id,
        exit_code,
        worktree_path,
        branch,
    })
}

fn run_pre_checks(event: &ExitedEvent) -> CheckOutcome {
    // 1. Worktree exists.
    let wt = event.worktree_path.to_string_lossy().to_string();
    let exists = sudo_picker_check(&format!("test -d {} && echo yes", shell_quote(&wt)));
    if !exists.trim().eq_ignore_ascii_case("yes") {
        return CheckOutcome::NeedsContinuation {
            reason: "worktree-missing",
            detail: format!("expected worktree at {wt} but it was not readable as picker"),
        };
    }

    // 2. Commits ahead of trunk.
    let base = std::env::var(DEFAULT_BASE_ENV).unwrap_or_else(|_| DEFAULT_BASE.into());
    let ahead_cmd = format!(
        "cd {wt} && git rev-list --count origin/{base}..HEAD 2>/dev/null",
        wt = shell_quote(&wt),
        base = shell_quote(&base),
    );
    let ahead = sudo_picker_check(&ahead_cmd);
    let ahead_count = ahead.trim().parse::<u32>().unwrap_or(0);
    if ahead_count == 0 {
        return CheckOutcome::NeedsContinuation {
            reason: "no-commits",
            detail: format!(
                "branch {} has no commits ahead of origin/{}",
                event.branch, base
            ),
        };
    }

    // 3. Working tree clean (ignoring .jam/, which is a runtime artifact dir).
    let status_cmd = format!(
        "cd {wt} && git status --porcelain --untracked-files=normal 2>/dev/null \
         | awk '$0 !~ /(^.. \\.jam\\/| \\.jam$)/' | head -c 4096",
        wt = shell_quote(&wt),
    );
    let dirty = sudo_picker_check(&status_cmd);
    let dirty_trimmed = dirty.trim();
    if !dirty_trimmed.is_empty() {
        return CheckOutcome::NeedsContinuation {
            reason: "dirty-tree",
            detail: format!(
                "worktree has uncommitted changes outside .jam/:\n{}",
                truncate(dirty_trimmed, 800)
            ),
        };
    }

    // 4. .jam/pr-title.txt + .jam/pr-body.md present and non-empty.
    let title = read_jam_file(&wt, ".jam/pr-title.txt");
    let body = read_jam_file(&wt, ".jam/pr-body.md");
    let Some(title) = title.filter(|s| !s.trim().is_empty()) else {
        return CheckOutcome::NeedsContinuation {
            reason: "missing-pr-metadata",
            detail: ".jam/pr-title.txt is missing or empty".into(),
        };
    };
    let Some(body) = body.filter(|s| !s.trim().is_empty()) else {
        return CheckOutcome::NeedsContinuation {
            reason: "missing-pr-metadata",
            detail: ".jam/pr-body.md is missing or empty".into(),
        };
    };

    CheckOutcome::Ready {
        title: title.trim().to_owned(),
        body,
    }
}

fn read_jam_file(worktree: &str, relpath: &str) -> Option<String> {
    let cmd = format!(
        "cat {} 2>/dev/null",
        shell_quote(&format!("{worktree}/{relpath}"))
    );
    let raw = sudo_picker_check(&cmd);
    if raw.is_empty() {
        None
    } else {
        Some(raw)
    }
}

fn sudo_picker_check(bash_cmd: &str) -> String {
    let sudo_bin = std::env::var(SUDO_BIN_ENV).unwrap_or_else(|_| DEFAULT_SUDO_BIN.into());
    let picker_user =
        std::env::var(PICKER_USER_ENV).unwrap_or_else(|_| DEFAULT_PICKER_USER.into());
    let mut command = Command::new(sudo_bin);
    command.args(["-n", "-u", &picker_user, "bash", "-c", bash_cmd]);
    match command.output() {
        Ok(output) => String::from_utf8_lossy(&output.stdout).into_owned(),
        Err(err) => {
            debug!("sudo picker check failed: {err}");
            String::new()
        }
    }
}

async fn request_open_pr(
    nats: &JamNats,
    event: &ExitedEvent,
    title: &str,
    body: &str,
    ctx: &TraceCtx,
) -> Result<String, String> {
    let base = std::env::var(DEFAULT_BASE_ENV).unwrap_or_else(|_| DEFAULT_BASE.into());
    let payload = OpenPrRequest {
        task_id: &event.task_id,
        branch: &event.branch,
        title,
        body,
        draft: false,
        base: &base,
        worktree_path: &event.worktree_path.to_string_lossy(),
        push: true,
    };
    let response: OpenPrResponse = nats
        .request_traced(OPEN_PR_SUBJECT, &payload, ctx, OPEN_PR_TIMEOUT)
        .await
        .map_err(|err| format!("nats request: {err}"))?;
    if let Some(error) = response.error {
        return Err(format!("open-pr error envelope: {error}"));
    }
    response
        .pr_ref
        .ok_or_else(|| "open-pr returned no pr_ref".into())
}

async fn publish_continuation(
    nats: &JamNats,
    event: &ExitedEvent,
    reason: &str,
    detail: &str,
    prompt: &str,
    ctx: &TraceCtx,
) {
    let payload = PickerContinuationNeeded {
        task_id: event.task_id.clone(),
        session_id: event.session_id.clone(),
        worktree_path: event.worktree_path.to_string_lossy().into_owned(),
        reason: reason.to_owned(),
        detail: detail.to_owned(),
        prompt: prompt.to_owned(),
        attempt: 0, // coordinator will increment on subsequent emissions
        requested_at: Utc::now(),
    };
    let envelope = jam_events::EventEnvelope::new(
        PickerContinuationNeeded::EVENT_TYPE,
        PickerContinuationNeeded::EVENT_SUBTYPE_VERSION,
        0,
        ctx.trace_id.to_string(),
        "jam-task-lifecycle",
        payload,
    );
    if let Err(err) = nats.publish_traced(CONTINUATION_SUBJECT, &envelope, ctx).await {
        warn!(
            task = %event.task_id,
            reason = %reason,
            error = %err,
            "failed to publish picker.continuation-needed",
        );
    } else {
        info!(
            task = %event.task_id,
            reason = %reason,
            "picker.continuation-needed published",
        );
    }
}

fn draft_continuation_prompt(reason: &str, detail: &str, event: &ExitedEvent) -> String {
    let common = format!(
        "The post-picker coordinator could not open a PR for task `{}` because: {}.\n\n",
        event.task_id, detail
    );
    let action = match reason {
        "no-commits" => "Make the code changes the task requires, commit them on this branch, write `.jam/pr-title.txt` and `.jam/pr-body.md`, then exit.",
        "dirty-tree" => "Commit (or revert) the listed pending changes so the working tree is clean. Then exit.",
        "missing-pr-metadata" => "Write `.jam/pr-title.txt` (one-line conventional-commit title) and `.jam/pr-body.md` (Summary + Verification sections) describing the existing commit(s). Do NOT amend commits or push. Then exit.",
        "picker-failed" => "Diagnose why the previous session exited non-zero and fix the root cause. Commit, write `.jam/pr-*` metadata, exit.",
        "worktree-missing" => "Re-create the worktree (the orchestrator may need to intervene). Notify the human.",
        _ => "Address the issue above and exit cleanly.",
    };
    let constraints = "Constraints:\n- Do NOT run `git push` or `gh pr create`; the orchestrator opens the PR.\n- Leave the worktree clean (no uncommitted changes outside `.jam/`).\n- `.jam/pr-title.txt` and `.jam/pr-body.md` are required.\n";
    format!("{common}{action}\n\n{constraints}")
}

fn draft_open_pr_failure_prompt(event: &ExitedEvent, err: &str) -> String {
    format!(
        "Tried to open a PR for task `{}` and the open-pr tool returned an error: {}.\n\nInspect what changed; if the issue is in the metadata (title/body), correct `.jam/pr-title.txt` / `.jam/pr-body.md` and exit. If the issue is upstream (push rejected, branch out of sync, etc.), fix it and exit. Do NOT call `gh pr create` directly.",
        event.task_id, err,
    )
}

fn shell_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for c in s.chars() {
        if c == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(c);
        }
    }
    out.push('\'');
    out
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_owned()
    } else {
        format!("{}…[truncated, {} bytes total]", &s[..max], s.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_quote_escapes_single_quotes() {
        assert_eq!(shell_quote("a"), "'a'");
        assert_eq!(shell_quote("a'b"), "'a'\\''b'");
        assert_eq!(shell_quote("/tmp/x"), "'/tmp/x'");
    }

    #[test]
    fn truncate_respects_max() {
        assert_eq!(truncate("hello", 10), "hello");
        let t = truncate("0123456789ABCDEF", 5);
        assert!(t.starts_with("01234"));
        assert!(t.contains("truncated"));
    }

    #[test]
    fn draft_continuation_prompt_includes_task_and_constraints() {
        let event = ExitedEvent {
            task_id: "demo-task".into(),
            session_id: "codex-cli:ULID".into(),
            exit_code: 0,
            worktree_path: PathBuf::from("/home/picker/workers/demo-task"),
            branch: "task/demo-task".into(),
        };
        let prompt = draft_continuation_prompt("missing-pr-metadata", "missing files", &event);
        assert!(prompt.contains("demo-task"));
        assert!(prompt.contains(".jam/pr-title.txt"));
        assert!(prompt.contains("Do NOT run `git push`"));
    }
}
