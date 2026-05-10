#!/usr/bin/env bash
#
# Prove the external acceptance audit's manual-evidence parser with temporary
# readable evidence. This does not prove production provider, phone, GitHub App,
# or substrate acceptance; those remain checked by audit-external-acceptance.sh.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
EVIDENCE_DIR=""
AUDIT_LOG=""
SUMMARY_JSON=""

PASS_GLYPH='\033[32m✓\033[0m'
FAIL_GLYPH='\033[31m✗\033[0m'
INFO_GLYPH='\033[34mi\033[0m'

pass() { printf "  ${PASS_GLYPH} %s\n" "$*"; }
fail() { printf "  ${FAIL_GLYPH} %s\n" "$*" >&2; }
info() { printf "  ${INFO_GLYPH} %s\n" "$*"; }

cleanup() {
    remove_evidence_dir
    if [[ -n "$AUDIT_LOG" && -f "$AUDIT_LOG" ]]; then
        rm -f "$AUDIT_LOG"
    fi
}

remove_evidence_dir() {
    if [[ -n "$EVIDENCE_DIR" ]]; then
        rm -rf "$EVIDENCE_DIR"
        EVIDENCE_DIR=""
    fi
}

create_valid_evidence() {
    EVIDENCE_DIR="$(mktemp -d /tmp/jam-external-audit-evidence.XXXXXX)"
    chmod 755 "$EVIDENCE_DIR"

    python3 - "$EVIDENCE_DIR" <<'PY'
import json
import sys
from datetime import datetime, timedelta, timezone
from pathlib import Path

root = Path(sys.argv[1])
now = datetime.now(timezone.utc).replace(microsecond=0)
started = now - timedelta(days=8)


def ts(value):
    return value.isoformat().replace("+00:00", "Z")


def payload(evidence):
    return {
        "verified_at": ts(now),
        "verifier": "smoke:external-audit-evidence",
        "evidence": evidence,
    }


files = {
    "tailscale-phone-ui.json": payload(
        {
            "ui_url": "https://100.64.0.10:5173",
            "auth_check": True,
            "websocket": "connected",
        }
    ),
    "ntfy-phone-delivery.json": payload(
        {
            "trace_id": "01SMOKEAUDITEVIDENCE000000",
            "phone_received_at": ts(now),
            "urgency": "critical",
        }
    ),
    "provider-quota-window.json": payload(
        {
            "trace_id": "01SMOKEAUDITPROVIDER00000",
            "harnesses": ["codex", "claude"],
            "quota_before": {
                "codex": {"remaining": 9},
                "claude": {"remaining": 7},
            },
            "quota_after": {
                "codex": {"remaining": 8},
                "claude": {"remaining": 6},
            },
        }
    ),
    "github-app-live-pr.json": payload(
        {
            "installation_id": 123456,
            "pr_ref": "caleb/blueberry#42",
            "push_verified": True,
            "comment_api_verified": True,
        }
    ),
    "seven-day-stability.json": payload(
        {
            "started_at": ts(started),
            "finished_at": ts(now),
            "completed_tasks": 64,
            "downtime_minutes": 1.5,
        }
    ),
}

for name, data in files.items():
    path = root / name
    path.write_text(json.dumps(data, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    path.chmod(0o644)
PY
}

create_invalid_evidence() {
    EVIDENCE_DIR="$(mktemp -d /tmp/jam-external-audit-invalid-evidence.XXXXXX)"
    chmod 755 "$EVIDENCE_DIR"

    python3 - "$EVIDENCE_DIR" <<'PY'
import json
import sys
from datetime import datetime, timedelta, timezone
from pathlib import Path

root = Path(sys.argv[1])
now = datetime.now(timezone.utc).replace(microsecond=0)
started = now - timedelta(days=1)


def ts(value):
    return value.isoformat().replace("+00:00", "Z")


def payload(evidence):
    return {
        "verified_at": ts(now),
        "verifier": "smoke:external-audit-invalid-evidence",
        "evidence": evidence,
    }


(root / "tailscale-phone-ui.json").write_text('{"verified_at": ', encoding="utf-8")

files = {
    "ntfy-phone-delivery.json": payload(
        {
            "trace_id": "01SMOKEINVALIDNTFY0000000",
            "phone_received_at": ts(now),
            "urgency": "low",
        }
    ),
    "provider-quota-window.json": payload(
        {
            "trace_id": "01SMOKEINVALIDPROVIDER000",
            "harnesses": [],
            "quota_before": {},
            "quota_after": {},
        }
    ),
    "github-app-live-pr.json": payload(
        {
            "installation_id": 0,
            "pr_ref": "caleb/blueberry#42",
            "push_verified": True,
            "comment_api_verified": True,
        }
    ),
    "seven-day-stability.json": payload(
        {
            "started_at": ts(started),
            "finished_at": ts(now),
            "completed_tasks": 64,
            "downtime_minutes": 1.5,
        }
    ),
}

for name, data in files.items():
    path = root / name
    path.write_text(json.dumps(data, indent=2, sort_keys=True) + "\n", encoding="utf-8")
for path in root.iterdir():
    path.chmod(0o644)
PY
}

run_audit() {
    AUDIT_LOG="$(mktemp /tmp/jam-external-audit-evidence-audit.XXXXXX.log)"

    local status=0
    if ACCEPTANCE_EVIDENCE_DIR="$EVIDENCE_DIR" scripts/audit-external-acceptance.sh >"$AUDIT_LOG" 2>&1; then
        status=0
    else
        status=$?
        if [[ "$status" -ne 1 && "$status" -ne 2 ]]; then
            fail "external audit exited unexpectedly with status $status; log: $AUDIT_LOG"
            sed -n '1,240p' "$AUDIT_LOG" >&2 || true
            return 1
        fi
    fi

    SUMMARY_JSON="$(awk '/summary: / { path=$NF } END { print path }' "$AUDIT_LOG")"
    if [[ -z "$SUMMARY_JSON" || ! -f "$SUMMARY_JSON" ]]; then
        fail "external audit did not produce a summary path; log: $AUDIT_LOG"
        sed -n '1,240p' "$AUDIT_LOG" >&2 || true
        return 1
    fi

    cp "$AUDIT_LOG" "$(dirname "$SUMMARY_JSON")/smoke-audit-output.log"
    rm -f "$AUDIT_LOG"
    AUDIT_LOG=""
    info "audit exited with status $status; summary: $SUMMARY_JSON"
}

verify_evidence_events() {
    python3 - "$SUMMARY_JSON" "$EVIDENCE_DIR" <<'PY'
import json
import sys
from pathlib import Path

summary_path = Path(sys.argv[1])
evidence_dir = sys.argv[2]

summary = json.loads(summary_path.read_text(encoding="utf-8"))
events_path = Path(summary["events_jsonl"])
if not events_path.is_file():
    raise SystemExit(f"events_jsonl missing: {events_path}")

events = []
for line_no, line in enumerate(events_path.read_text(encoding="utf-8").splitlines(), start=1):
    if not line.strip():
        continue
    try:
        events.append(json.loads(line))
    except json.JSONDecodeError as exc:
        raise SystemExit(f"invalid summary.jsonl line {line_no}: {exc}") from exc

expected = [
    "Tailscale phone UI evidence valid",
    "ntfy phone delivery evidence valid",
    "real provider/quota evidence valid",
    "GitHub App live PR/push/comment evidence valid",
    "7-day stability evidence valid",
]

missing = []
for fragment in expected:
    if not any(
        event.get("status") == "passed"
        and fragment in event.get("detail", "")
        and evidence_dir in event.get("detail", "")
        for event in events
    ):
        missing.append(fragment)

invalid = [
    event.get("detail", "")
    for event in events
    if "evidence missing or invalid" in event.get("detail", "")
    and evidence_dir in event.get("detail", "")
]

if missing:
    raise SystemExit("missing passed evidence events: " + ", ".join(missing))
if invalid:
    raise SystemExit("unexpected invalid evidence events: " + "; ".join(invalid))

remediations = summary.get("remediations")
if not isinstance(remediations, list):
    raise SystemExit("summary.remediations must be a list")
if summary.get("failures", 0) and not remediations:
    raise SystemExit("expected remediation entries while audit has failures")

for item in remediations:
    if not isinstance(item, dict):
        raise SystemExit("summary.remediations entries must be objects")
    if not item.get("id") or not item.get("title"):
        raise SystemExit("summary.remediations entries must include id and title")

remediation_ids = {item.get("id") for item in remediations}
unexpected_evidence_remediations = remediation_ids & {
    "verify-ntfy-phone-delivery",
    "run-real-provider-quota-window",
    "verify-github-app-live-pr",
    "run-seven-day-stability-soak",
}
if unexpected_evidence_remediations:
    raise SystemExit(
        "valid fake evidence unexpectedly produced remediation ids: "
        + ", ".join(sorted(unexpected_evidence_remediations))
    )
PY
}

verify_invalid_evidence_events() {
    python3 - "$SUMMARY_JSON" "$EVIDENCE_DIR" <<'PY'
import json
import sys
from pathlib import Path

summary_path = Path(sys.argv[1])
evidence_dir = sys.argv[2]

summary = json.loads(summary_path.read_text(encoding="utf-8"))
manual_details = [
    item.get("detail", "")
    for item in summary.get("manual_followup_checks", [])
    if evidence_dir in item.get("detail", "")
]

expected_fragments = [
    "Tailscale phone UI evidence missing or invalid",
    "invalid JSON at line",
    "ntfy phone delivery evidence missing or invalid",
    "evidence.urgency must be high or critical",
    "real provider/quota evidence missing or invalid",
    "evidence.harnesses must be a non-empty list",
    "GitHub App live PR/push/comment evidence missing or invalid",
    "evidence.installation_id must be positive",
    "7-day stability evidence missing or invalid",
    "must cover at least 7 days",
]

missing = [
    fragment
    for fragment in expected_fragments
    if not any(fragment in detail for detail in manual_details)
]
if missing:
    raise SystemExit("missing invalid-evidence reason fragments: " + ", ".join(missing))

remediation_ids = {item.get("id") for item in summary.get("remediations", [])}
expected_remediations = {
    "verify-ntfy-phone-delivery",
    "run-real-provider-quota-window",
    "verify-github-app-live-pr",
    "run-seven-day-stability-soak",
}
missing_remediations = expected_remediations - remediation_ids
if missing_remediations:
    raise SystemExit(
        "invalid fake evidence did not produce remediation ids: "
        + ", ".join(sorted(missing_remediations))
    )
PY
}

trap cleanup EXIT
cd "$ROOT"

command -v python3 >/dev/null 2>&1 || {
    fail "missing required command: python3"
    exit 1
}

create_valid_evidence
run_audit
verify_evidence_events
remove_evidence_dir
create_invalid_evidence
run_audit
verify_invalid_evidence_events
pass "external audit evidence smoke passed"
