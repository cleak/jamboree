#!/usr/bin/env bash
#
# Prove the Phase 0 NATS -> jam-nats-bridge -> JSONL journal path.
#
# Default mode uses a temporary process-compose project. Pass --existing to
# verify an already-running substrate writes to $JAM_HOME/journal. Pass
# --maestro-runtime to start cached NATS plus target/debug/jam-nats-bridge as
# the maestro user while writing to /home/maestro/.jam/journal.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
NATS_PORT="${NATS_PORT:-42241}"
MAESTRO_NATS_PORT="${MAESTRO_NATS_PORT:-4222}"
NATS_URL="${NATS_URL:-}"
TRACE_ID="${TRACE_ID:-01KQYS00000000000000000000}"
MODE="isolated"
SMOKE_DIR=""
JOURNAL_HOME=""
PC_PID=""
NATS_PID=""
NATS_CHILD_PID=""
BRIDGE_PID=""
BRIDGE_CHILD_PID=""
BRIDGE_LOG=""
NATS_LOG=""
NATS_STORE_DIR=""

PASS_GLYPH='\033[32m✓\033[0m'
FAIL_GLYPH='\033[31m✗\033[0m'
INFO_GLYPH='\033[34mi\033[0m'

pass() { printf "  ${PASS_GLYPH} %s\n" "$*"; }
fail() { printf "  ${FAIL_GLYPH} %s\n" "$*" >&2; }
info() { printf "  ${INFO_GLYPH} %s\n" "$*"; }

die() {
    fail "$1"
    if [[ -n "${2:-}" ]]; then
        printf "\n    Fix:\n%s\n\n" "$2" >&2
    fi
    exit 1
}

cleanup() {
    local status=$?
    if [[ -n "$PC_PID" ]]; then
        terminate_pid "$PC_PID"
    fi
    if [[ -n "$BRIDGE_PID" ]]; then
        terminate_pid "$BRIDGE_PID"
    fi
    if [[ -n "$BRIDGE_CHILD_PID" ]]; then
        terminate_pid "$BRIDGE_CHILD_PID"
    fi
    if [[ -n "$NATS_PID" ]]; then
        terminate_pid "$NATS_PID"
    fi
    if [[ -n "$NATS_CHILD_PID" ]]; then
        terminate_pid "$NATS_CHILD_PID"
    fi
    cleanup_maestro_runtime_processes
    if [[ -n "$NATS_STORE_DIR" ]]; then
        sudo -n -u maestro -H rm -rf "$NATS_STORE_DIR" 2>/dev/null || true
    fi
    if [[ $status -ne 0 && -n "$SMOKE_DIR" ]]; then
        [[ -f "$SMOKE_DIR/process-compose.stdout.log" ]] && sed -n '1,220p' "$SMOKE_DIR/process-compose.stdout.log" >&2
        [[ -f "$SMOKE_DIR/process-compose.log" ]] && sed -n '1,220p' "$SMOKE_DIR/process-compose.log" >&2
        [[ -n "$NATS_LOG" && -f "$NATS_LOG" ]] && sed -n '1,220p' "$NATS_LOG" >&2
        [[ -n "$BRIDGE_LOG" && -f "$BRIDGE_LOG" ]] && sed -n '1,220p' "$BRIDGE_LOG" >&2
    fi
    if [[ -n "$SMOKE_DIR" ]]; then
        rm -rf "$SMOKE_DIR"
    fi
}
trap cleanup EXIT

terminate_pid() {
    local pid="$1"
    local child_pids=""

    child_pids="$(pgrep -P "$pid" 2>/dev/null || true)"
    if [[ -n "$child_pids" ]]; then
        kill $child_pids 2>/dev/null || true
        sudo -n -u maestro -H kill $child_pids 2>/dev/null || true
    fi

    kill "$pid" 2>/dev/null || true
    sudo -n -u maestro -H kill "$pid" 2>/dev/null || true
    for _ in {1..50}; do
        if ! kill -0 "$pid" 2>/dev/null; then
            wait "$pid" 2>/dev/null || true
            return
        fi
        sleep 0.1
    done

    child_pids="$(pgrep -P "$pid" 2>/dev/null || true)"
    if [[ -n "$child_pids" ]]; then
        kill -KILL $child_pids 2>/dev/null || true
        sudo -n -u maestro -H kill -KILL $child_pids 2>/dev/null || true
    fi
    kill -KILL "$pid" 2>/dev/null || true
    sudo -n -u maestro -H kill -KILL "$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
}

cleanup_maestro_runtime_processes() {
    if [[ "$MODE" != "maestro-runtime" ]]; then
        return
    fi
    if [[ -n "$NATS_STORE_DIR" ]]; then
        local nats_pids
        nats_pids="$(pgrep -u maestro -f "nats-server.*${NATS_STORE_DIR}" 2>/dev/null || true)"
        if [[ -n "$nats_pids" ]]; then
            sudo -n -u maestro -H kill $nats_pids 2>/dev/null || true
            sleep 0.2
            sudo -n -u maestro -H kill -KILL $nats_pids 2>/dev/null || true
        fi
    fi
    local bridge_pids
    bridge_pids="$(pgrep -u maestro -f "${ROOT}/target/debug/jam-nats-bridge" 2>/dev/null || true)"
    if [[ -n "$bridge_pids" ]]; then
        sudo -n -u maestro -H kill $bridge_pids 2>/dev/null || true
        sleep 0.2
        sudo -n -u maestro -H kill -KILL $bridge_pids 2>/dev/null || true
    fi
}

need() {
    command -v "$1" >/dev/null 2>&1 || die "missing required command: $1"
}

usage() {
    cat <<'EOF'
Usage: scripts/smoke-substrate-journal.sh [--existing|--maestro-runtime]

Modes:
  default            Start temporary NATS + jam-nats-bridge and verify JSONL.
  --existing         Use an already-running substrate. Defaults to:
                     NATS_URL=nats://127.0.0.1:4222
                     JAM_HOME=/home/maestro/.jam
  --maestro-runtime  Start cached NATS + target/debug/jam-nats-bridge as
                     maestro on the configured MAESTRO_NATS_PORT and
                     verify /home/maestro/.jam/journal. Does not write
                     /opt/jam/bin or leave processes running.

Environment:
  TRACE_ID             Trace ID to publish.
  NATS_URL             NATS URL for --existing mode.
  NATS_PORT            Temporary NATS port for default mode.
  MAESTRO_NATS_PORT    NATS port for --maestro-runtime mode.
  MAESTRO_JAM_HOME     JAM_HOME for --maestro-runtime mode.
  PROCESS_COMPOSE_BIN  process-compose binary override for default mode.
  NATS_SERVER_BIN      nats-server binary override.
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --existing)
            MODE="existing"
            ;;
        --maestro-runtime)
            MODE="maestro-runtime"
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            die "unknown argument: $1" "    Run scripts/smoke-substrate-journal.sh --help"
            ;;
    esac
    shift
done

if [[ "$MODE" == "existing" ]]; then
    if [[ -z "${NATS_URL:-}" ]]; then
        NATS_URL="nats://127.0.0.1:4222"
    fi
elif [[ "$MODE" == "maestro-runtime" ]]; then
    NATS_URL="nats://127.0.0.1:${MAESTRO_NATS_PORT}"
else
    NATS_URL="nats://127.0.0.1:${NATS_PORT}"
fi

find_bin() {
    local name="$1"
    local env_var="$2"
    local configured="${!env_var:-}"

    if [[ -n "$configured" ]]; then
        [[ -x "$configured" ]] || die "$env_var is not executable: $configured"
        printf '%s\n' "$configured"
        return
    fi

    if command -v "$name" >/dev/null 2>&1; then
        command -v "$name"
        return
    fi

    local cached="/tmp/jam-substrate/bin/$name"
    if [[ -x "$cached" ]]; then
        printf '%s\n' "$cached"
        return
    fi

    die "missing required binary: $name" \
"    Install production binaries with sudo ./scripts/install-substrate.sh, or
    run this smoke with $env_var=/path/to/$name."
}

wait_for_port() {
    python3 - "$NATS_URL" <<'PY'
import socket
import sys
import time
from urllib.parse import urlparse

parsed = urlparse(sys.argv[1])
host = parsed.hostname or "127.0.0.1"
port = parsed.port or 4222
deadline = time.time() + 15
while time.time() < deadline:
    with socket.socket() as sock:
        sock.settimeout(0.2)
        if sock.connect_ex((host, port)) == 0:
            raise SystemExit(0)
    time.sleep(0.1)
raise SystemExit("NATS did not become reachable")
PY
}

wait_bridge_ready() {
    for _ in {1..160}; do
        if [[ -n "$BRIDGE_LOG" ]]; then
            if rg -q 'subscribed to journal.>' "$BRIDGE_LOG" 2>/dev/null; then
                return 0
            fi
            if [[ -n "$BRIDGE_PID" ]] && ! kill -0 "$BRIDGE_PID" 2>/dev/null; then
                die "journal bridge exited before becoming ready"
            fi
        elif rg -q 'subscribed to journal.>' "$SMOKE_DIR/process-compose.stdout.log" "$SMOKE_DIR/process-compose.log" 2>/dev/null; then
            return 0
        fi
        sleep 0.1
    done
    die "journal bridge did not become ready"
}

assert_port_free() {
    python3 - "$NATS_URL" <<'PY'
import socket
import sys
from urllib.parse import urlparse

parsed = urlparse(sys.argv[1])
host = parsed.hostname or "127.0.0.1"
port = parsed.port or 4222
with socket.socket() as sock:
    sock.settimeout(0.2)
    if sock.connect_ex((host, port)) == 0:
        raise SystemExit(1)
PY
}

publish_and_verify() {
    local expected_day
    expected_day="$(date -u +%Y-%m-%d)"
    local timestamp
    timestamp="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

    python3 - "$NATS_URL" "$TRACE_ID" "$timestamp" <<'PY'
import asyncio
import json
import sys
from urllib.parse import urlparse


async def read_info(reader):
    line = await asyncio.wait_for(reader.readline(), timeout=5)
    if not line.startswith(b"INFO "):
        raise RuntimeError(f"expected INFO, got {line!r}")


async def connect(nats_url):
    parsed = urlparse(nats_url)
    reader, writer = await asyncio.open_connection(parsed.hostname or "127.0.0.1", parsed.port or 4222)
    await read_info(reader)
    writer.write(b'CONNECT {"lang":"substrate-journal-smoke","version":"0.1.0","protocol":1,"headers":true}\r\n')
    await writer.drain()
    return reader, writer


async def wait_pong(reader, writer):
    while True:
        line = await asyncio.wait_for(reader.readline(), timeout=5)
        if line.startswith(b"PONG"):
            return
        if line.startswith(b"PING"):
            writer.write(b"PONG\r\n")
            await writer.drain()


async def subscribe(reader, writer, subject):
    writer.write(f"SUB {subject} 1\r\nPING\r\n".encode())
    await writer.drain()
    await wait_pong(reader, writer)


async def request_json(reader, writer, subject, payload):
    inbox = "_INBOX.substrate_journal_smoke"
    await subscribe(reader, writer, inbox)
    body = json.dumps(payload, separators=(",", ":")).encode()
    writer.write(f"PUB {subject} {inbox} {len(body)}\r\n".encode() + body + b"\r\n")
    await writer.drain()
    while True:
        line = await asyncio.wait_for(reader.readline(), timeout=10)
        if line.startswith(b"PING"):
            writer.write(b"PONG\r\n")
            await writer.drain()
            continue
        parts = line.decode().strip().split()
        if not parts or parts[0] in {"+OK", "-ERR", "PONG"}:
            continue
        if parts[0] != "MSG":
            raise RuntimeError(f"unexpected NATS line: {line!r}")
        size = int(parts[-1])
        data = await reader.readexactly(size + 2)
        return json.loads(data[:size])


async def publish_journal(writer, trace_id, timestamp):
    payload = {
        "schema_version": 1,
        "event_type": "test.smoke",
        "event_subtype_version": 1,
        "timestamp": timestamp,
        "journal_seq": 1,
        "trace_id": trace_id,
        "actor": "scripts/smoke-substrate-journal.sh",
        "payload": {"source": "substrate-journal-smoke"},
    }
    body = json.dumps(payload, separators=(",", ":")).encode()
    headers = f"NATS/1.0\r\nTrace-Id: {trace_id}\r\n\r\n".encode()
    total = len(headers) + len(body)
    writer.write(f"HPUB journal.test {len(headers)} {total}\r\n".encode() + headers + body + b"\r\n")
    await writer.drain()


async def main():
    reader, writer = await connect(sys.argv[1])
    names = await request_json(reader, writer, "$JS.API.STREAM.NAMES", {"offset": 0})
    streams = set(names.get("streams", []))
    required = {"journal", "KV_routing-manifest"}
    missing = sorted(required - streams)
    if missing:
        raise RuntimeError(f"missing JetStream resources: {missing}; saw {sorted(streams)}")
    await publish_journal(writer, sys.argv[2], sys.argv[3])
    writer.write(b"PING\r\n")
    await writer.drain()
    await wait_pong(reader, writer)


asyncio.run(main())
PY

    local journal_file="$JOURNAL_HOME/journal/$expected_day/journal.test.jsonl"
    for _ in {1..120}; do
        if [[ -f "$journal_file" ]] && rg -q "\"trace_id\":\"$TRACE_ID\"|\"trace_id\": \"$TRACE_ID\"" "$journal_file"; then
            rg -q '"event_type":"test.smoke"|"event_type": "test.smoke"' "$journal_file"
            pass "journal.test landed at $journal_file with trace_id $TRACE_ID"
            return
        fi
        sleep 0.1
    done

    find "$JOURNAL_HOME/journal" -maxdepth 3 -type f -print -exec sed -n '1,20p' {} \; >&2 || true
    die "journal.test entry did not land"
}

need python3
need rg

if [[ "$MODE" == "existing" ]]; then
    JOURNAL_HOME="${JAM_HOME:-/home/maestro/.jam}"
    [[ -d "$JOURNAL_HOME" ]] || die "JAM_HOME does not exist: $JOURNAL_HOME"
    info "verifying existing substrate at $NATS_URL with journal home $JOURNAL_HOME"
    wait_for_port
    publish_and_verify
    pass "existing substrate journal smoke passed"
    exit 0
fi

need cargo

if [[ "$MODE" == "maestro-runtime" ]]; then
    NATS_SERVER_BIN="$(find_bin nats-server NATS_SERVER_BIN)"
    if ! sudo -n -u maestro -H true; then
        die "cannot run commands as maestro noninteractively" \
"    Run scripts/bootstrap-users.sh, then rerun this smoke from Caleb's shell."
    fi
    if ! assert_port_free; then
        die "NATS port is already in use at $NATS_URL" \
"    If the production substrate is already running, use:
        scripts/smoke-substrate-journal.sh --existing"
    fi

    cd "$ROOT"
    cargo build -p jam-nats-bridge

    SMOKE_DIR="$(mktemp -d /tmp/jam-substrate-maestro-runtime.XXXXXX)"
    JOURNAL_HOME="${MAESTRO_JAM_HOME:-/home/maestro/.jam}"
    NATS_STORE_DIR="$JOURNAL_HOME/nats-data-smoke"
    NATS_LOG="$SMOKE_DIR/nats.log"
    BRIDGE_LOG="$SMOKE_DIR/jam-nats-bridge.log"

    sudo -n -u maestro -H mkdir -p "$JOURNAL_HOME/journal"
    sudo -n -u maestro -H rm -rf "$NATS_STORE_DIR"
    sudo -n -u maestro -H mkdir -p "$NATS_STORE_DIR"

    info "starting cached nats-server as maestro at $NATS_URL"
    sudo -n -u maestro -H env HOME=/home/maestro JAM_HOME="$JOURNAL_HOME" \
        "$NATS_SERVER_BIN" \
        --jetstream \
        --store_dir "$NATS_STORE_DIR" \
        --addr 127.0.0.1 \
        --port "$MAESTRO_NATS_PORT" >"$NATS_LOG" 2>&1 &
    NATS_PID="$!"

    wait_for_port
    NATS_CHILD_PID="$(pgrep -u maestro -f "nats-server.*${NATS_STORE_DIR}" 2>/dev/null | head -n1 || true)"

    info "starting target/debug/jam-nats-bridge as maestro"
    sudo -n -u maestro -H env HOME=/home/maestro JAM_HOME="$JOURNAL_HOME" NATS_URL="$NATS_URL" \
        "$ROOT/target/debug/jam-nats-bridge" >"$BRIDGE_LOG" 2>&1 &
    BRIDGE_PID="$!"
    BRIDGE_CHILD_PID="$(pgrep -u maestro -f "${ROOT}/target/debug/jam-nats-bridge" 2>/dev/null | head -n1 || true)"

    wait_bridge_ready
    publish_and_verify
    pass "maestro runtime journal smoke passed"
    exit 0
fi

PROCESS_COMPOSE_BIN="$(find_bin process-compose PROCESS_COMPOSE_BIN)"
NATS_SERVER_BIN="$(find_bin nats-server NATS_SERVER_BIN)"

cd "$ROOT"
cargo build -p jam-nats-bridge

SMOKE_DIR="$(mktemp -d /tmp/jam-substrate-journal-smoke.XXXXXX)"
JOURNAL_HOME="$SMOKE_DIR/jam-home"
COMPOSE_FILE="$SMOKE_DIR/process-compose.yaml"

cat >"$COMPOSE_FILE" <<YAML
version: "0.5"

environment:
  - HOME=$SMOKE_DIR/home
  - JAM_HOME=$SMOKE_DIR/jam-home
  - NATS_URL=$NATS_URL

processes:
  nats:
    command: "$NATS_SERVER_BIN --jetstream --store_dir $SMOKE_DIR/nats-data --addr 127.0.0.1 --port $NATS_PORT"
    availability:
      restart: on_failure
      backoff_seconds: 1

  jam-nats-bridge:
    command: "$ROOT/target/debug/jam-nats-bridge"
    depends_on:
      nats:
        condition: process_started
    availability:
      restart: on_failure
      backoff_seconds: 1
YAML

info "starting temporary process-compose substrate"
"$PROCESS_COMPOSE_BIN" \
    --no-server \
    -t=false \
    -L "$SMOKE_DIR/process-compose.log" \
    -f "$COMPOSE_FILE" \
    up nats jam-nats-bridge >"$SMOKE_DIR/process-compose.stdout.log" 2>&1 &
PC_PID="$!"

wait_for_port
wait_bridge_ready
publish_and_verify
pass "substrate journal smoke passed"
