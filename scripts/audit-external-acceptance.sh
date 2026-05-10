#!/usr/bin/env bash
#
# Rootless audit of the production/external acceptance checks that cannot be
# proven by the deterministic local smoke suite.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
INSTALL_DIR="${INSTALL_DIR:-/opt/jam/bin}"
UI_DIST_DIR="${UI_DIST_DIR:-/home/maestro/.jam/ui/dist}"
UI_TOKEN_DIR="$(dirname "$UI_DIST_DIR")"
NATS_URL="${NATS_URL:-nats://127.0.0.1:4222}"
ACCEPTANCE_EVIDENCE_DIR="${ACCEPTANCE_EVIDENCE_DIR:-/home/maestro/.jam/acceptance}"
FAILED=0
MANUAL=0
AUDIT_DIR=""
SUMMARY_JSON=""
EVENTS_JSONL=""

PASS_GLYPH='\033[32m✓\033[0m'
FAIL_GLYPH='\033[31m✗\033[0m'
WARN_GLYPH='\033[33m!\033[0m'
INFO_GLYPH='\033[34mi\033[0m'

record_event() {
    local status="$1"
    shift
    if [[ -z "$EVENTS_JSONL" ]]; then
        return
    fi
    command -v python3 >/dev/null 2>&1 || return
    python3 - "$EVENTS_JSONL" "$status" "$*" <<'PY'
import json
import sys
from datetime import datetime, timezone
from pathlib import Path

path = Path(sys.argv[1])
event = {
    "ts": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
    "status": sys.argv[2],
    "detail": sys.argv[3],
}
with path.open("a", encoding="utf-8") as handle:
    handle.write(json.dumps(event, separators=(",", ":")) + "\n")
PY
    return 0
}
pass() {
    printf "  ${PASS_GLYPH} %s\n" "$*"
    record_event "passed" "$*"
}
fail() {
    printf "  ${FAIL_GLYPH} %s\n" "$*" >&2
    FAILED=$((FAILED + 1))
    record_event "failed" "$*"
}
warn() {
    printf "  ${WARN_GLYPH} %s\n" "$*" >&2
    MANUAL=$((MANUAL + 1))
    record_event "manual" "$*"
}
info() { printf "  ${INFO_GLYPH} %s\n" "$*"; }

write_summary() {
    local status="passed"
    if [[ "$FAILED" -ne 0 ]]; then
        status="failed"
    elif [[ "$MANUAL" -ne 0 ]]; then
        status="manual-follow-up"
    fi
    if ! command -v python3 >/dev/null 2>&1; then
        cat >"$SUMMARY_JSON" <<EOF
{
  "finished_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "status": "$status",
  "failures": $FAILED,
  "manual_followups": $MANUAL,
  "events_jsonl": "$EVENTS_JSONL"
}
EOF
        return
    fi

    python3 - "$SUMMARY_JSON" "$EVENTS_JSONL" "$status" "$FAILED" "$MANUAL" <<'PY'
import json
import sys
from datetime import datetime, timezone
from pathlib import Path

summary_path = Path(sys.argv[1])
events_path = Path(sys.argv[2])
status = sys.argv[3]
failures = int(sys.argv[4])
manual_followups = int(sys.argv[5])

events = []
if events_path.is_file():
    for line in events_path.read_text(encoding="utf-8").splitlines():
        if line.strip():
            events.append(json.loads(line))

details = [
    str(event.get("detail", ""))
    for event in events
    if event.get("status") in {"failed", "manual"}
]
remediations = []
seen_remediations = set()


def add_remediation(remediation_id, title, commands=None, evidence_files=None, docs=None):
    if remediation_id in seen_remediations:
        return
    seen_remediations.add(remediation_id)
    item = {"id": remediation_id, "title": title}
    if commands:
        item["commands"] = commands
    if evidence_files:
        item["evidence_files"] = evidence_files
    if docs:
        item["docs"] = docs
    remediations.append(item)


production_install_bins = (
    "/opt/jam/bin/nats-server",
    "/opt/jam/bin/process-compose",
    "/opt/jam/bin/jam",
    "/opt/jam/bin/jam-nats-bridge",
    "/opt/jam/bin/jam-svc-message",
    "/opt/jam/bin/jam-svc-supervise",
    "/opt/jam/bin/jam-ui-server",
)


def is_production_install_issue(detail):
    if detail == "/home/maestro/.jam/ui/dist/index.html missing":
        return True
    if any(detail.startswith(f"{path} missing or not executable") for path in production_install_bins):
        return True
    if detail in {
        "/home/maestro/.jam/ui missing",
        "/home/maestro/.jam/ui not writable by maestro for UI session tokens",
    }:
        return True
    if detail.startswith(("nats-server version ", "process-compose version ")):
        return "version command failed" in detail or "version drift" in detail
    return False


if any(is_production_install_issue(detail) for detail in details):
    add_remediation(
        "install-production-substrate",
        "Run the interactive/root substrate install so /opt/jam/bin and the UI bundle exist.",
        commands=[
            "sudo ./scripts/install-substrate.sh",
            "/opt/jam/bin/nats-server --version",
            "/opt/jam/bin/process-compose version",
        ],
        docs=["docs/runbooks/external-acceptance.md#install-and-start-production-substrate"],
    )

if any("NATS not reachable" in detail for detail in details):
    add_remediation(
        "start-production-substrate",
        "Start the production process-compose substrate and verify the existing NATS-to-JSONL path.",
        commands=[
            "sudo /opt/jam/bin/process-compose up -f /home/caleb/jamboree/process-compose.yaml -t=false",
            "scripts/smoke-substrate-journal.sh --existing",
        ],
        docs=["docs/runbooks/external-acceptance.md#install-and-start-production-substrate"],
    )

if any("github-app-installation-id" in detail for detail in details):
    add_remediation(
        "seed-github-app-installation-id",
        "Install the GitHub App on Blueberry and seed the runtime installation ID.",
        commands=[
            "sudo -u maestro -H pass insert jam/pickers/github-app-installation-id",
            "/opt/jam/bin/jam doctor",
        ],
        evidence_files=["/home/maestro/.jam/acceptance/github-app-live-pr.json"],
        docs=["docs/runbooks/external-acceptance.md#seed-github-app-installation"],
    )

if any("Tailscale host CGNAT" in detail or "Tailscale phone UI evidence" in detail for detail in details):
    add_remediation(
        "verify-tailscale-phone-ui",
        "Connect the host and phone to Tailscale, verify UI auth/WebSocket from the phone, and write evidence.",
        evidence_files=["/home/maestro/.jam/acceptance/tailscale-phone-ui.json"],
        docs=[
            "docs/runbooks/mobile-tailscale-ui.md",
            "docs/runbooks/external-acceptance.md#tailscale-phone-ui",
        ],
    )

if any("ntfy phone delivery evidence" in detail for detail in details):
    add_remediation(
        "verify-ntfy-phone-delivery",
        "Enable/verify ntfy phone subscription, publish a high-urgency notify-human request, and write evidence.",
        evidence_files=["/home/maestro/.jam/acceptance/ntfy-phone-delivery.json"],
        docs=["docs/runbooks/external-acceptance.md#ntfy-phone-delivery"],
    )

if any("real provider/quota evidence" in detail for detail in details):
    add_remediation(
        "run-real-provider-quota-window",
        "Run real provider and quota acceptance during an approved quota window, then write before/after evidence.",
        evidence_files=["/home/maestro/.jam/acceptance/provider-quota-window.json"],
        docs=["docs/runbooks/external-acceptance.md#real-provider-and-quota-window"],
    )

if any("GitHub App live PR/push/comment evidence" in detail for detail in details):
    add_remediation(
        "verify-github-app-live-pr",
        "Use the GitHub App installation token for a real Blueberry PR push/comment flow, then write evidence.",
        evidence_files=["/home/maestro/.jam/acceptance/github-app-live-pr.json"],
        docs=["docs/runbooks/external-acceptance.md#github-app-live-pr"],
    )

if any("7-day stability evidence" in detail for detail in details):
    add_remediation(
        "run-seven-day-stability-soak",
        "Run the production orchestrator for at least seven days, complete at least 50 tasks, and write soak evidence.",
        evidence_files=["/home/maestro/.jam/acceptance/seven-day-stability.json"],
        docs=["docs/runbooks/external-acceptance.md#seven-day-stability"],
    )

if any("doctor" in detail for detail in details):
    add_remediation(
        "rerun-doctor-after-remediation",
        "Rerun jam doctor after the install, substrate, GitHub App, and evidence blockers are addressed.",
        commands=["/opt/jam/bin/jam doctor"],
        docs=["docs/runbooks/external-acceptance.md#final-acceptance"],
    )

summary = {
    "finished_at": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
    "status": status,
    "failures": failures,
    "manual_followups": manual_followups,
    "events_jsonl": str(events_path),
    "failed_checks": [
        {"ts": event.get("ts"), "detail": event.get("detail")}
        for event in events
        if event.get("status") == "failed"
    ],
    "manual_followup_checks": [
        {"ts": event.get("ts"), "detail": event.get("detail")}
        for event in events
        if event.get("status") == "manual"
    ],
    "remediations": remediations,
}
summary_path.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY
}

need() {
    command -v "$1" >/dev/null 2>&1 || {
        fail "missing required command: $1"
        return 1
    }
}

check_exec() {
    local path="$1"
    if [[ -x "$path" ]]; then
        pass "$path executable"
    else
        fail "$path missing or not executable"
    fi
}

check_version_contains() {
    local label="$1"
    local command="$2"
    local expected="$3"
    local output=""
    if ! output="$(bash -lc "$command" 2>&1)"; then
        fail "$label version command failed: $output"
        return
    fi
    if [[ "$output" == *"$expected"* ]]; then
        pass "$label version includes $expected"
    else
        fail "$label version drift: expected $expected, got: $output"
    fi
}

pass_key_exists() {
    local key="$1"
    sudo -n -u maestro -H pass show "$key" >/dev/null 2>&1
}

check_pass_key() {
    local key="$1"
    if pass_key_exists "$key"; then
        pass "maestro pass has $key"
    else
        fail "maestro pass missing $key"
    fi
}

evidence_file_is_valid_json() {
    local path="$1"
    shift
    sudo -n -u maestro -H python3 - "$path" "$@" <<'PY'
import json
import sys
from datetime import datetime
from json import JSONDecodeError
from pathlib import Path

path = Path(sys.argv[1])
required_evidence_keys = sys.argv[2:]


def fail(message):
    raise SystemExit(message)


if not path.is_file():
    fail("file is missing or not a regular file")
try:
    raw = path.read_text(encoding="utf-8")
except OSError as exc:
    fail(f"cannot read file: {exc}")
try:
    data = json.loads(raw)
except JSONDecodeError as exc:
    fail(f"invalid JSON at line {exc.lineno}, column {exc.colno}: {exc.msg}")
if not isinstance(data, dict):
    fail("top-level JSON value must be an object")


def parse_ts(field, value):
    if not isinstance(value, str) or not value.strip():
        fail(f"{field} must be a non-empty timestamp string")

    try:
        return datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError:
        fail(f"{field} must be parseable as an ISO/RFC3339 timestamp")


def non_empty_string(value):
    return isinstance(value, str) and bool(value.strip())


def trueish(value):
    return value is True or value in {"true", "ok", "connected", "passed", "200"} or value == 200


parse_ts("verified_at", data.get("verified_at"))
if not non_empty_string(data.get("verifier")):
    fail("verifier must be a non-empty string")
evidence = data.get("evidence")
if not isinstance(evidence, dict):
    fail("evidence must be an object")
for key in required_evidence_keys:
    value = evidence.get(key)
    if value is None or value == "":
        fail(f"evidence.{key} is required")

name = path.name
if name == "tailscale-phone-ui.json":
    if not non_empty_string(evidence.get("ui_url")):
        fail("evidence.ui_url must be a non-empty string")
    if not evidence["ui_url"].startswith(("http://", "https://")):
        fail("evidence.ui_url must start with http:// or https://")
    if not trueish(evidence.get("auth_check")) or not trueish(evidence.get("websocket")):
        fail("evidence.auth_check and evidence.websocket must both be positive")
elif name == "ntfy-phone-delivery.json":
    if not non_empty_string(evidence.get("trace_id")) or len(evidence["trace_id"]) < 10:
        fail("evidence.trace_id must be a non-empty trace ID")
    parse_ts("evidence.phone_received_at", evidence.get("phone_received_at"))
    if evidence.get("urgency") not in {"high", "critical"}:
        fail("evidence.urgency must be high or critical")
elif name == "provider-quota-window.json":
    if not non_empty_string(evidence.get("trace_id")) or len(evidence["trace_id"]) < 10:
        fail("evidence.trace_id must be a non-empty trace ID")
    if not isinstance(evidence.get("harnesses"), list) or not evidence["harnesses"]:
        fail("evidence.harnesses must be a non-empty list")
    if not isinstance(evidence.get("quota_before"), dict) or not isinstance(evidence.get("quota_after"), dict):
        fail("evidence.quota_before and evidence.quota_after must both be objects")
elif name == "github-app-live-pr.json":
    installation_id = evidence.get("installation_id")
    if isinstance(installation_id, int):
        if installation_id <= 0:
            fail("evidence.installation_id must be positive")
    elif isinstance(installation_id, str):
        if not installation_id.isdigit() or int(installation_id) <= 0:
            fail("evidence.installation_id must be a positive integer or digit string")
    else:
        fail("evidence.installation_id must be a positive integer or digit string")
    if not non_empty_string(evidence.get("pr_ref")) or "#" not in evidence["pr_ref"]:
        fail("evidence.pr_ref must be a non-empty PR reference containing #")
    if evidence.get("push_verified") is not True or evidence.get("comment_api_verified") is not True:
        fail("evidence.push_verified and evidence.comment_api_verified must both be true")
elif name == "seven-day-stability.json":
    started = parse_ts("evidence.started_at", evidence.get("started_at"))
    finished = parse_ts("evidence.finished_at", evidence.get("finished_at"))
    if (finished - started).total_seconds() < 7 * 24 * 60 * 60:
        fail("evidence.started_at to evidence.finished_at must cover at least 7 days")
    try:
        completed_tasks = int(evidence.get("completed_tasks"))
    except (TypeError, ValueError):
        fail("evidence.completed_tasks must be an integer")
    if completed_tasks < 50:
        fail("evidence.completed_tasks must be at least 50")
    try:
        downtime_minutes = float(evidence.get("downtime_minutes"))
    except (TypeError, ValueError):
        fail("evidence.downtime_minutes must be a number")
    if downtime_minutes >= 5:
        fail("evidence.downtime_minutes must be less than 5")
PY
}

check_evidence_file() {
    local filename="$1"
    local description="$2"
    shift 2
    local path="$ACCEPTANCE_EVIDENCE_DIR/$filename"
    local error=""
    if error="$(evidence_file_is_valid_json "$path" "$@" 2>&1)"; then
        pass "$description evidence valid at $path"
    else
        error="${error//$'\n'/; }"
        if [[ -n "$error" ]]; then
            warn "$description evidence missing or invalid at $path: $error"
        else
            warn "$description evidence missing or invalid at $path"
        fi
    fi
}

nats_reachable() {
    python3 - "$NATS_URL" <<'PY'
import socket
import sys
from urllib.parse import urlparse

url = urlparse(sys.argv[1])
host = url.hostname or "127.0.0.1"
port = url.port or 4222
try:
    with socket.create_connection((host, port), timeout=2) as sock:
        sock.settimeout(2)
        line = sock.recv(1024)
except OSError:
    raise SystemExit(1)
if not line.startswith(b"INFO "):
    raise SystemExit(1)
PY
}

header() {
    printf "\n\033[1m%s\033[0m\n" "$*"
}

cd "$ROOT"

AUDIT_DIR="$(mktemp -d /tmp/jam-external-acceptance.XXXXXX)"
SUMMARY_JSON="$AUDIT_DIR/summary.json"
EVENTS_JSONL="$AUDIT_DIR/summary.jsonl"

header "External Acceptance Audit"
info "This audit intentionally excludes deterministic local smokes; run scripts/smoke-local-acceptance.sh for those."
info "summary: $SUMMARY_JSON"

header "Production Substrate Install"
check_exec "$INSTALL_DIR/nats-server"
check_exec "$INSTALL_DIR/process-compose"
for bin in jam jam-nats-bridge jam-svc-message jam-svc-supervise jam-ui-server; do
    check_exec "$INSTALL_DIR/$bin"
done
if [[ -f "$UI_DIST_DIR/index.html" ]]; then
    pass "$UI_DIST_DIR/index.html present"
else
    fail "$UI_DIST_DIR/index.html missing"
fi
if [[ -d "$UI_TOKEN_DIR" ]]; then
    pass "$UI_TOKEN_DIR present"
    if sudo -n -u maestro -H test -w "$UI_TOKEN_DIR" >/dev/null 2>&1; then
        pass "$UI_TOKEN_DIR writable by maestro for UI session tokens"
    else
        fail "$UI_TOKEN_DIR not writable by maestro for UI session tokens"
    fi
else
    fail "$UI_TOKEN_DIR missing"
fi
if [[ -x "$INSTALL_DIR/nats-server" ]]; then
    check_version_contains "nats-server" "$INSTALL_DIR/nats-server --version" "2.11.0"
fi
if [[ -x "$INSTALL_DIR/process-compose" ]]; then
    check_version_contains "process-compose" "$INSTALL_DIR/process-compose version" "1.40.1"
fi

header "Production NATS And Journal"
need python3 || true
if command -v python3 >/dev/null 2>&1 && nats_reachable; then
    pass "NATS reachable at $NATS_URL"
    if scripts/smoke-substrate-journal.sh --existing >/tmp/jam-existing-substrate-smoke.log 2>&1; then
        pass "production --existing journal smoke passed"
    else
        fail "production --existing journal smoke failed; see /tmp/jam-existing-substrate-smoke.log"
    fi
else
    fail "NATS not reachable at $NATS_URL"
fi

header "GitHub App"
if sudo -n -u maestro -H true >/dev/null 2>&1; then
    check_pass_key "jam/pickers/github-app-id"
    check_pass_key "jam/pickers/github-app-installation-id"
    check_pass_key "jam/pickers/github-app-key"
else
    fail "cannot run noninteractive commands as maestro"
fi

header "Deployment-Specific Manual Acceptance"
if command -v tailscale >/dev/null 2>&1 && tailscale ip -4 2>/dev/null | rg -q '^100\.(6[4-9]|[7-9][0-9]|1[01][0-9]|12[0-7])\.'; then
    pass "Tailscale CGNAT IPv4 present"
else
    warn "Tailscale host CGNAT IPv4 missing"
fi
check_evidence_file "tailscale-phone-ui.json" "Tailscale phone UI" \
    ui_url auth_check websocket
check_evidence_file "ntfy-phone-delivery.json" "ntfy phone delivery" \
    trace_id phone_received_at urgency
check_evidence_file "provider-quota-window.json" "real provider/quota" \
    trace_id harnesses quota_before quota_after
check_evidence_file "github-app-live-pr.json" "GitHub App live PR/push/comment" \
    installation_id pr_ref push_verified comment_api_verified
check_evidence_file "seven-day-stability.json" "7-day stability" \
    started_at finished_at completed_tasks downtime_minutes

header "jam doctor"
doctor_bin=""
if [[ -x "$INSTALL_DIR/jam" ]]; then
    doctor_bin="$INSTALL_DIR/jam"
elif [[ -x target/debug/jam ]]; then
    doctor_bin="target/debug/jam"
    info "using development jam binary for doctor because $INSTALL_DIR/jam is not installed"
else
    fail "no jam binary available for doctor; install substrate or build jam-cli"
fi
if [[ -n "$doctor_bin" ]]; then
    doctor_detail="$doctor_bin doctor"
    if [[ "$doctor_bin" == "target/debug/jam" ]]; then
        doctor_detail="$doctor_bin doctor (development fallback because $INSTALL_DIR/jam is not installed)"
    fi
    if "$doctor_bin" doctor >/tmp/jam-doctor-audit.log 2>&1; then
        pass "$doctor_detail clean"
    else
        fail "$doctor_detail still failing; see /tmp/jam-doctor-audit.log"
    fi
fi

printf "\n\033[1mSummary\033[0m\n"
printf "  failures: %s\n" "$FAILED"
printf "  manual follow-ups: %s\n" "$MANUAL"
write_summary
printf "  summary: %s\n" "$SUMMARY_JSON"

if [[ "$FAILED" -ne 0 ]]; then
    exit 1
fi
if [[ "$MANUAL" -ne 0 ]]; then
    exit 2
fi
exit 0
