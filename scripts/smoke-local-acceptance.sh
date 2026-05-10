#!/usr/bin/env bash
#
# Run the local, deterministic Jamboree acceptance smokes that do not require
# root-owned /opt installs, real provider quota, phone delivery, or GitHub App
# installation credentials.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODE="core"
LOG_DIR=""
LIST_ONLY=false
SUMMARY_JSON=""
SUMMARY_JSONL=""

PASS_GLYPH='\033[32m✓\033[0m'
FAIL_GLYPH='\033[31m✗\033[0m'
INFO_GLYPH='\033[34mi\033[0m'

pass() { printf "  ${PASS_GLYPH} %s\n" "$*"; }
fail() { printf "  ${FAIL_GLYPH} %s\n" "$*" >&2; }
info() { printf "  ${INFO_GLYPH} %s\n" "$*"; }

usage() {
    cat <<'EOF'
Usage: scripts/smoke-local-acceptance.sh [--core|--all|--heavy-only] [--list]

Modes:
  --core  Run deterministic local acceptance smokes that are broad but bounded.
          This is the default.
  --all   Run core plus heavier deterministic smokes.
  --heavy-only
          Run only the heavier deterministic smokes. Useful after --core passed.
  --list  Print the selected smoke commands instead of running them.

Excluded by design:
  - sudo ./scripts/install-substrate.sh production install
  - scripts/smoke-substrate-journal.sh --existing production substrate check
  - real provider quota burns, real GitHub App flows, phone/Tailscale checks
EOF
}

parse_args() {
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --core)
                MODE="core"
                ;;
            --all)
                MODE="all"
                ;;
            --heavy-only)
                MODE="heavy-only"
                ;;
            --list)
                LIST_ONLY=true
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

core_smokes() {
    cat <<'EOF'
install-substrate::scripts/smoke-install-substrate.sh
maestro-runtime-journal::MAESTRO_NATS_PORT=42242 scripts/smoke-substrate-journal.sh --maestro-runtime
message-modes::scripts/smoke-message-modes-delivery.sh
research-service::scripts/smoke-research-service.sh
search-service::scripts/smoke-search-service.sh
external-audit-evidence::scripts/smoke-external-audit-evidence.sh
evolve-coordinator::scripts/smoke-evolve-coordinator.sh
EOF
}

heavy_smokes() {
    cat <<'EOF'
atomic-swap::scripts/smoke-atomic-swap-mid-session.sh
docker-sandbox::scripts/smoke-docker-sandbox-backend.sh
cgroup-resource-limits::scripts/smoke-cgroup-resource-limits.sh
hermes-evolution-vendor::scripts/smoke-hermes-evolution-vendor.sh
patch-agent-recovery::scripts/smoke-patch-agent-recovery.sh
EOF
}

selected_smokes() {
    if [[ "$MODE" != "heavy-only" ]]; then
        core_smokes
    fi
    if [[ "$MODE" == "all" || "$MODE" == "heavy-only" ]]; then
        heavy_smokes
    fi
}

print_smokes() {
    selected_smokes | while IFS='::' read -r name _ command; do
        [[ -n "$name" ]] || continue
        printf '%-28s %s\n' "$name" "$command"
    done
}

run_smoke() {
    local name="$1"
    local command="$2"
    local log="$LOG_DIR/${name}.log"

    info "running $name"
    if bash -lc "$command" >"$log" 2>&1; then
        pass "$name passed"
        write_smoke_summary "$name" "passed" "$log"
        return 0
    fi

    fail "$name failed; log: $log"
    write_smoke_summary "$name" "failed" "$log"
    sed -n '1,240p' "$log" >&2 || true
    return 1
}

write_smoke_summary() {
    local name="$1"
    local status="$2"
    local log="$3"
    printf '{"ts":"%s","mode":"%s","name":"%s","status":"%s","log":"%s"}\n' \
        "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
        "$MODE" \
        "$name" \
        "$status" \
        "$log" >>"$SUMMARY_JSONL"
}

write_suite_summary() {
    local status="$1"
    printf '{\n' >"$SUMMARY_JSON"
    printf '  "finished_at": "%s",\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" >>"$SUMMARY_JSON"
    printf '  "mode": "%s",\n' "$MODE" >>"$SUMMARY_JSON"
    printf '  "status": "%s",\n' "$status" >>"$SUMMARY_JSON"
    printf '  "log_dir": "%s",\n' "$LOG_DIR" >>"$SUMMARY_JSON"
    printf '  "events_jsonl": "%s"\n' "$SUMMARY_JSONL" >>"$SUMMARY_JSON"
    printf '}\n' >>"$SUMMARY_JSON"
}

run_all() {
    LOG_DIR="$(mktemp -d /tmp/jam-local-acceptance.XXXXXX)"
    SUMMARY_JSON="$LOG_DIR/summary.json"
    SUMMARY_JSONL="$LOG_DIR/summary.jsonl"
    info "logs: $LOG_DIR"
    local failures=0
    while IFS='::' read -r name _ command; do
        [[ -n "$name" ]] || continue
        if ! run_smoke "$name" "$command"; then
            failures=$((failures + 1))
            break
        fi
    done < <(selected_smokes)

    if [[ "$failures" -ne 0 ]]; then
        write_suite_summary "failed"
        fail "local acceptance suite failed"
        printf '  summary: %s\n' "$SUMMARY_JSON" >&2
        return 1
    fi

    write_suite_summary "passed"
    pass "local acceptance suite passed"
    printf '  logs retained at %s\n' "$LOG_DIR"
    printf '  summary: %s\n' "$SUMMARY_JSON"
}

parse_args "$@"
cd "$ROOT"

if [[ "$LIST_ONLY" == true ]]; then
    print_smokes
    exit 0
fi

run_all
