#!/usr/bin/env bash
#
# Prove the §20.5 patch-agent recovery loop with a live NATS server:
# 1. a patch that passes ping but fails the observe smoke check is mechanically
#    rolled back and confirmed healthy;
# 2. a patch whose rollback route is also unhealthy runs the LLM diagnosis hook,
#    writes an incident dump, publishes patch.failed + notify.human, pauses
#    dispatch, and exits non-zero.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
NATS_PORT="${NATS_PORT:-42230}"
NATS_URL="nats://127.0.0.1:${NATS_PORT}"
CONTAINER="jam-smoke-nats-patch-agent-$$"
SMOKE_HOME=""
AGENT_PID=""
EVENT_PID=""

cleanup() {
    local status=$?
    if [[ $status -ne 0 && -n "$SMOKE_HOME" ]]; then
        for log in "$SMOKE_HOME"/patch-agent-*.log "$SMOKE_HOME"/logs/*.stderr.log "$SMOKE_HOME"/logs/*.stdout.log; do
            if [[ -f "$log" ]]; then
                printf '\n--- %s ---\n' "$log" >&2
                sed -n '1,220p' "$log" >&2
            fi
        done
    fi
    if [[ -n "$AGENT_PID" ]]; then
        kill "$AGENT_PID" 2>/dev/null || true
        wait "$AGENT_PID" 2>/dev/null || true
    fi
    if [[ -n "$EVENT_PID" ]]; then
        kill "$EVENT_PID" 2>/dev/null || true
        wait "$EVENT_PID" 2>/dev/null || true
    fi
    if [[ -n "$SMOKE_HOME" ]]; then
        pkill -f "$SMOKE_HOME/bin/jam-svc-observe" 2>/dev/null || true
        rm -rf "$SMOKE_HOME"
    fi
    docker stop "$CONTAINER" >/dev/null 2>&1 || true
}
trap cleanup EXIT

need() {
    command -v "$1" >/dev/null 2>&1 || {
        printf 'missing required command: %s\n' "$1" >&2
        exit 1
    }
}

wait_for_port() {
    python3 - "$NATS_PORT" <<'PY'
import socket
import sys
import time

port = int(sys.argv[1])
deadline = time.time() + 10
while time.time() < deadline:
    with socket.socket() as sock:
        sock.settimeout(0.2)
        if sock.connect_ex(("127.0.0.1", port)) == 0:
            raise SystemExit(0)
    time.sleep(0.1)
raise SystemExit("NATS did not become reachable")
PY
}

capture_events() {
    local ready="$1"
    local output="$2"
    shift 2
    python3 - "$NATS_URL" "$ready" "$output" "$@" <<'PY' &
import asyncio
import json
import sys
from pathlib import Path
from urllib.parse import urlparse


async def read_info(reader: asyncio.StreamReader) -> None:
    line = await asyncio.wait_for(reader.readline(), timeout=5)
    if not line.startswith(b"INFO "):
        raise RuntimeError(f"expected INFO, got {line!r}")


async def read_message(reader: asyncio.StreamReader) -> tuple[str, object]:
    while True:
        line = await asyncio.wait_for(reader.readline(), timeout=30)
        if not line:
            raise RuntimeError("NATS connection closed")
        parts = line.decode().strip().split()
        if not parts or parts[0] in {"PING", "PONG", "+OK"}:
            continue
        if parts[0] == "HMSG":
            subject = parts[1]
            if len(parts) == 5:
                header_len = int(parts[3])
                total_len = int(parts[4])
            else:
                header_len = int(parts[4])
                total_len = int(parts[5])
            data = await reader.readexactly(total_len + 2)
            payload = data[header_len:total_len]
            return subject, json.loads(payload)
        if parts[0] == "MSG":
            subject = parts[1]
            total_len = int(parts[-1])
            data = await reader.readexactly(total_len + 2)
            return subject, json.loads(data[:total_len])
        raise RuntimeError(f"unexpected NATS protocol line: {line!r}")


async def main() -> None:
    nats_url, ready, output, *subjects = sys.argv[1:]
    parsed = urlparse(nats_url)
    reader, writer = await asyncio.open_connection(parsed.hostname, parsed.port)
    await read_info(reader)
    writer.write(b'CONNECT {"lang":"patch-agent-smoke","version":"0.1.0","protocol":1,"headers":true}\r\n')
    for index, subject in enumerate(subjects, start=1):
        writer.write(f"SUB {subject} {index}\r\n".encode())
    await writer.drain()
    Path(ready).write_text("ready\n")

    seen: dict[str, object] = {}
    deadline = asyncio.get_running_loop().time() + 30
    while set(seen) != set(subjects):
        timeout = max(0.1, deadline - asyncio.get_running_loop().time())
        subject, payload = await asyncio.wait_for(read_message(reader), timeout=timeout)
        if subject in subjects and subject not in seen:
            seen[subject] = payload
    Path(output).write_text(json.dumps(seen, indent=2, sort_keys=True) + "\n")
    writer.close()
    await writer.wait_closed()


asyncio.run(main())
PY
    EVENT_PID="$!"
    wait_for_file "$ready" "event capture did not subscribe"
}

wait_for_file() {
    local path="$1"
    local message="$2"
    for _ in {1..100}; do
        [[ -f "$path" ]] && return 0
        sleep 0.1
    done
    printf '%s\n' "$message" >&2
    exit 1
}

wait_health_ok() {
    local subject="$1"
    for _ in {1..100}; do
        if target/debug/jam health ping observe --subject "$subject" --nats-url "$NATS_URL" --timeout-secs 1 >/dev/null 2>&1; then
            return 0
        fi
        sleep 0.1
    done
    printf 'health ping did not become ok: %s\n' "$subject" >&2
    target/debug/jam health ping observe --subject "$subject" --nats-url "$NATS_URL" --timeout-secs 1 >&2 || true
    exit 1
}

start_patch_agent() {
    local log="$1"
    shift
    "$@" \
        target/debug/jam-patch-agent \
        --max-events 1 \
        --nats-url "$NATS_URL" \
        --jam-bin "$ROOT/target/debug/jam" \
        >"$log" 2>&1 &
    AGENT_PID="$!"
    for _ in {1..100}; do
        grep -q "subscribed" "$log" && return 0
        if ! kill -0 "$AGENT_PID" 2>/dev/null; then
            cat "$log" >&2
            printf 'patch agent exited before subscribing\n' >&2
            exit 1
        fi
        sleep 0.1
    done
    cat "$log" >&2
    printf 'patch agent did not subscribe\n' >&2
    exit 1
}

start_observe_route() {
    local version="$1"
    local prefix="$2"
    local broken="$3"
    local stdout="$SMOKE_HOME/logs/manual-observe-${version}.stdout.log"
    local stderr="$SMOKE_HOME/logs/manual-observe-${version}.stderr.log"
    JAM_OBSERVE_LIST_BLOCKERS_BROKEN="$broken" \
    JAM_OBSERVE_GITHUB_LOOKUP=false \
    JAM_OBSERVE_SUBJECT_PREFIX="$prefix" \
    "$SMOKE_HOME/bin/jam-svc-observe-${version}" >"$stdout" 2>"$stderr" &
    wait_health_ok "${prefix}.ping"
}

need docker
need python3

cd "$ROOT"
cargo build -p jam-cli -p jam-svc-observe -p jam-patch-agent

docker run --rm -d --name "$CONTAINER" -p "127.0.0.1:${NATS_PORT}:4222" nats:2.11.0-alpine -js >/dev/null
wait_for_port

SMOKE_HOME="$(mktemp -d /tmp/jam-patch-agent.XXXXXX)"
mkdir -p "$SMOKE_HOME/staging" "$SMOKE_HOME/logs"
cp target/debug/jam-svc-observe "$SMOKE_HOME/staging/jam-svc-observe-0.0.9"
cp target/debug/jam-svc-observe "$SMOKE_HOME/staging/jam-svc-observe-0.1.0"
cp target/debug/jam-svc-observe "$SMOKE_HOME/staging/jam-svc-observe-0.1.1"
chmod +x "$SMOKE_HOME/staging/jam-svc-observe-0.0.9" \
    "$SMOKE_HOME/staging/jam-svc-observe-0.1.0" \
    "$SMOKE_HOME/staging/jam-svc-observe-0.1.1"

export JAM_HOME="$SMOKE_HOME"
export NATS_URL
export JAM_OBSERVE_GITHUB_LOOKUP=false
export JAM_PATCH_HEALTH_TIMEOUT_SECS=10
export JAM_PATCH_DRAIN_TIMEOUT_SECS=10
export JAM_PATCH_AGENT_SKIP_DOCTOR=1
export JAM_PATCH_AGENT_REQUEST_TIMEOUT_SECS=2
export JAM_PATCH_AGENT_COMMAND_TIMEOUT_SECS=10

target/debug/jam patch apply observe 0.0.9 --nats-url "$NATS_URL"

SUCCESS_READY="$SMOKE_HOME/success-events-ready"
SUCCESS_EVENTS="$SMOKE_HOME/success-events.json"
capture_events "$SUCCESS_READY" "$SUCCESS_EVENTS" patch.rolled-back-successfully notify.human
start_patch_agent "$SMOKE_HOME/patch-agent-success.log" env -u JAM_OBSERVE_LIST_BLOCKERS_BROKEN -u JAM_PATCH_AGENT_LLM_CMD
JAM_OBSERVE_LIST_BLOCKERS_BROKEN=true \
    target/debug/jam patch apply observe 0.1.0 --nats-url "$NATS_URL"
wait "$AGENT_PID"
AGENT_PID=""
wait "$EVENT_PID"
EVENT_PID=""
wait_health_ok tool.observe.v009.ping

pkill -f "$SMOKE_HOME/bin/jam-svc-observe-0.0.9" 2>/dev/null || true
start_observe_route "0.0.9" "tool.observe.v009" "true"

FAIL_READY="$SMOKE_HOME/failure-events-ready"
FAIL_EVENTS="$SMOKE_HOME/failure-events.json"
capture_events "$FAIL_READY" "$FAIL_EVENTS" patch.failed notify.human
start_patch_agent "$SMOKE_HOME/patch-agent-failure.log" env JAM_OBSERVE_LIST_BLOCKERS_BROKEN=true JAM_PATCH_AGENT_LLM_CMD=/bin/false
set +e
JAM_OBSERVE_LIST_BLOCKERS_BROKEN=true \
    target/debug/jam patch apply observe 0.1.1 --nats-url "$NATS_URL"
APPLY_STATUS="$?"
wait "$AGENT_PID"
AGENT_STATUS="$?"
set -e
AGENT_PID=""
if [[ "$AGENT_STATUS" -eq 0 ]]; then
    cat "$SMOKE_HOME/patch-agent-failure.log" >&2
    printf 'patch agent was expected to exit non-zero after unrecoverable failure\n' >&2
    exit 1
fi
if [[ "$APPLY_STATUS" -ne 0 ]]; then
    printf 'second jam patch apply exited %s after emitting patch.applied; continuing smoke\n' "$APPLY_STATUS"
fi
wait "$EVENT_PID"
EVENT_PID=""

INCIDENT_DIR="$(python3 - "$FAIL_EVENTS" <<'PY'
import json
import sys

events = json.load(open(sys.argv[1], encoding="utf-8"))
print(events["patch.failed"]["payload"]["incident_dir"])
PY
)"
test -d "$INCIDENT_DIR"
python3 - "$INCIDENT_DIR/llm-diagnosis.json" <<'PY'
import json
import sys

data = json.load(open(sys.argv[1], encoding="utf-8"))
if data["attempted"] is not True:
    raise SystemExit("LLM diagnosis hook was not attempted")
if data["status"] != "exit-1":
    raise SystemExit(f"expected /bin/false to produce exit-1, got {data['status']!r}")
PY

cat "$SUCCESS_EVENTS"
cat "$FAIL_EVENTS"
printf 'patch-agent recovery smoke passed\n'
