#!/usr/bin/env bash
#
# Rootless completion audit for the active Jamboree implementation objective.
# This does not replace real production/provider/phone acceptance; it gathers
# the current graph state plus a fresh external acceptance audit into one
# machine-readable handoff artifact.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
AUDIT_DIR=""
SUMMARY_JSON=""
RUN_LOCAL_CORE=0

PASS_GLYPH='\033[32m✓\033[0m'
FAIL_GLYPH='\033[31m✗\033[0m'
INFO_GLYPH='\033[34mi\033[0m'

pass() { printf "  ${PASS_GLYPH} %s\n" "$*"; }
fail() { printf "  ${FAIL_GLYPH} %s\n" "$*" >&2; }
info() { printf "  ${INFO_GLYPH} %s\n" "$*"; }

usage() {
    cat <<'EOF'
Usage: scripts/audit-completion-state.sh [--run-local-core]

Options:
  --run-local-core  Run scripts/smoke-local-acceptance.sh --core first and use
                    that fresh local acceptance summary.
  -h, --help        Show this help.
EOF
}

parse_args() {
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --run-local-core)
                RUN_LOCAL_CORE=1
                ;;
            -h|--help)
                usage
                exit 0
                ;;
            *)
                fail "unknown argument: $1"
                usage >&2
                exit 1
                ;;
        esac
        shift
    done
}

run_tempyr_list() {
    local status="$1"
    local output="$AUDIT_DIR/tempyr-${status}.txt"
    local rc_file="$AUDIT_DIR/tempyr-${status}.status"
    local rc=0
    if tempyr list --type task --status "$status" >"$output" 2>&1; then
        rc=0
    else
        rc=$?
    fi
    printf '%s\n' "$rc" >"$rc_file"
}

latest_local_acceptance_summary() {
    find /tmp -maxdepth 2 -path '/tmp/jam-local-acceptance.*/summary.json' \
        -printf '%T@ %p\n' 2>/dev/null \
        | sort -rn \
        | awk 'NR == 1 { print $2 }'
}

run_local_core_acceptance() {
    local log="$AUDIT_DIR/local-acceptance-core.log"
    local rc=0
    if scripts/smoke-local-acceptance.sh --core >"$log" 2>&1; then
        rc=0
    else
        rc=$?
    fi
    printf '%s\n' "$rc" >"$AUDIT_DIR/local-acceptance.status"
    awk '/summary: / { path=$NF } END { print path }' "$log" >"$AUDIT_DIR/local-summary.path"
}

run_external_audit() {
    local log="$AUDIT_DIR/external-acceptance.log"
    local rc=0
    if scripts/audit-external-acceptance.sh >"$log" 2>&1; then
        rc=0
    else
        rc=$?
    fi
    printf '%s\n' "$rc" >"$AUDIT_DIR/external-acceptance.status"
    awk '/summary: / { path=$NF } END { print path }' "$log" >"$AUDIT_DIR/external-summary.path"
}

write_summary() {
    local local_summary_path="$1"
    local local_summary_source="$2"
    python3 - "$SUMMARY_JSON" "$AUDIT_DIR" "$local_summary_path" "$local_summary_source" <<'PY'
import json
import sys
from datetime import datetime, timezone
from pathlib import Path

summary_path = Path(sys.argv[1])
audit_dir = Path(sys.argv[2])
local_summary_path = Path(sys.argv[3]) if sys.argv[3] else None
local_summary_source = sys.argv[4]


def read_status(path):
    try:
        return int(Path(path).read_text(encoding="utf-8").strip())
    except (OSError, ValueError):
        return None


def read_lines(path):
    try:
        lines = Path(path).read_text(encoding="utf-8").splitlines()
    except OSError:
        return []
    if len(lines) == 1 and lines[0] == "No nodes match the given filters.":
        return []
    return [line for line in lines if line.strip()]


def read_json(path):
    if not path or not Path(path).is_file():
        return None
    return json.loads(Path(path).read_text(encoding="utf-8"))


graph = {}
for status in ("backlog", "in_progress", "blocked"):
    lines = read_lines(audit_dir / f"tempyr-{status}.txt")
    graph[status] = {
        "command_status": read_status(audit_dir / f"tempyr-{status}.status"),
        "count": len(lines),
        "nodes": lines,
    }

graph_validate = {
    "command_status": read_status(audit_dir / "tempyr-validate.status"),
    "log": str(audit_dir / "tempyr-validate.log"),
}

required_task_ids = [
    "task-litellm-backend-skeleton",
    "task-ui-shell-axum-and-solidjs",
    "task-nats-jetstream-up",
    "task-maestro-session-loop",
    "task-jam-svc-session-codex-cli-only",
    "task-jam-svc-observe-mvp",
]
required_tasks = {}
for task_id in required_task_ids:
    path = Path("graph/tasks") / f"{task_id}.md"
    status = None
    if path.is_file():
        for line in path.read_text(encoding="utf-8").splitlines():
            if line.startswith("status:"):
                status = line.split(":", 1)[1].strip()
                break
    required_tasks[task_id] = {
        "path": str(path),
        "status": status,
        "done": status == "done",
    }

local_summary = read_json(local_summary_path) if local_summary_path else None
external_summary_path_text = (audit_dir / "external-summary.path").read_text(encoding="utf-8").strip()
external_summary_path = Path(external_summary_path_text) if external_summary_path_text else None
external_summary = read_json(external_summary_path) if external_summary_path else None

local_status = local_summary.get("status") if isinstance(local_summary, dict) else None
required_core_smokes = [
    "install-substrate",
    "maestro-runtime-journal",
    "message-modes",
    "research-service",
    "search-service",
    "external-audit-evidence",
    "evolve-coordinator",
]
local_smokes = {}
if isinstance(local_summary, dict):
    events_jsonl = local_summary.get("events_jsonl")
    events_path = Path(events_jsonl) if events_jsonl else None
    if events_path and events_path.is_file():
        for line in events_path.read_text(encoding="utf-8").splitlines():
            if not line.strip():
                continue
            event = json.loads(line)
            name = event.get("name")
            if name:
                local_smokes[name] = {
                    "status": event.get("status"),
                    "log": event.get("log"),
                    "ts": event.get("ts"),
                }
local_required_smokes = {
    name: {
        "status": local_smokes.get(name, {}).get("status"),
        "log": local_smokes.get(name, {}).get("log"),
        "passed": local_smokes.get(name, {}).get("status") == "passed",
    }
    for name in required_core_smokes
}
local_required_smokes_all_passed = all(
    item["passed"] for item in local_required_smokes.values()
)
external_status = external_summary.get("status") if isinstance(external_summary, dict) else None
external_failures = external_summary.get("failures") if isinstance(external_summary, dict) else None
external_manual = external_summary.get("manual_followups") if isinstance(external_summary, dict) else None
required_tasks_all_done = all(task["done"] for task in required_tasks.values())

objective_complete = (
    graph_validate["command_status"] == 0
    and required_tasks_all_done
    and graph["backlog"]["command_status"] == 0
    and graph["in_progress"]["command_status"] == 0
    and graph["blocked"]["command_status"] == 0
    and graph["backlog"]["count"] == 0
    and graph["in_progress"]["count"] == 0
    and graph["blocked"]["count"] == 0
    and local_status == "passed"
    and local_required_smokes_all_passed
    and external_status == "passed"
    and external_failures == 0
    and external_manual == 0
)

summary = {
    "finished_at": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
    "status": "complete" if objective_complete else "incomplete",
    "objective_complete": objective_complete,
    "graph": graph,
    "graph_validate": graph_validate,
    "required_tasks": required_tasks,
    "required_tasks_all_done": required_tasks_all_done,
    "local_acceptance_summary": str(local_summary_path) if local_summary_path else None,
    "local_acceptance_summary_source": local_summary_source,
    "local_acceptance_command_status": read_status(audit_dir / "local-acceptance.status"),
    "local_acceptance_log": (
        str(audit_dir / "local-acceptance-core.log")
        if (audit_dir / "local-acceptance-core.log").is_file()
        else None
    ),
    "local_acceptance_status": local_status,
    "local_required_smokes": local_required_smokes,
    "local_required_smokes_all_passed": local_required_smokes_all_passed,
    "external_acceptance_summary": str(external_summary_path) if external_summary_path else None,
    "external_acceptance_status": external_status,
    "external_acceptance_failures": external_failures,
    "external_acceptance_manual_followups": external_manual,
    "external_acceptance_remediations": (
        len(external_summary.get("remediations", [])) if isinstance(external_summary, dict) else None
    ),
}
summary_path.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY
}

parse_args "$@"
cd "$ROOT"
AUDIT_DIR="$(mktemp -d /tmp/jam-completion-audit.XXXXXX)"
SUMMARY_JSON="$AUDIT_DIR/summary.json"

info "completion audit: $AUDIT_DIR"

run_tempyr_list backlog
run_tempyr_list in_progress
run_tempyr_list blocked

if tempyr validate >"$AUDIT_DIR/tempyr-validate.log" 2>&1; then
    printf '0\n' >"$AUDIT_DIR/tempyr-validate.status"
else
    printf '%s\n' "$?" >"$AUDIT_DIR/tempyr-validate.status"
fi

LOCAL_SUMMARY_SOURCE="latest-retained"
if [[ "$RUN_LOCAL_CORE" -eq 1 ]]; then
    info "running fresh local core acceptance"
    run_local_core_acceptance
    LOCAL_SUMMARY="$(cat "$AUDIT_DIR/local-summary.path")"
    LOCAL_SUMMARY_SOURCE="fresh-core-run"
else
    LOCAL_SUMMARY="$(latest_local_acceptance_summary || true)"
    printf '%s\n' "$LOCAL_SUMMARY" >"$AUDIT_DIR/local-summary.path"
fi
run_external_audit
write_summary "$LOCAL_SUMMARY" "$LOCAL_SUMMARY_SOURCE"

if python3 - "$SUMMARY_JSON" <<'PY'
import json
import sys
from pathlib import Path

summary = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
print(f"  summary: {sys.argv[1]}")
print(
    "  graph: "
    f"backlog={summary['graph']['backlog']['count']} "
    f"in_progress={summary['graph']['in_progress']['count']} "
    f"blocked={summary['graph']['blocked']['count']}"
)
print(
    "  local: "
    f"source={summary['local_acceptance_summary_source']} "
    f"status={summary['local_acceptance_status']} "
    f"required_smokes={summary['local_required_smokes_all_passed']}"
)
print(
    "  external: "
    f"status={summary['external_acceptance_status']} "
    f"failures={summary['external_acceptance_failures']} "
    f"manual_followups={summary['external_acceptance_manual_followups']}"
)
raise SystemExit(0 if summary["objective_complete"] else 1)
PY
then
    pass "completion audit passed"
    exit 0
fi

fail "completion audit incomplete"
exit 1
