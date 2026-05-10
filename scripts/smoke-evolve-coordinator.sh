#!/usr/bin/env bash
#
# Prove `jam-svc-evolve` request/reply wiring and subprocess invocation using
# the vendored Hermes adapter in dry-run mode. This does not spend LLM tokens.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
NATS_PORT="${NATS_PORT:-42238}"
NATS_URL="nats://127.0.0.1:${NATS_PORT}"
NATS_CONTAINER="jam-smoke-nats-evolve-$$"
SMOKE_DIR=""
SERVICE_PID=""

cleanup() {
    local status=$?
    if [[ $status -ne 0 && -n "$SMOKE_DIR" && -f "$SMOKE_DIR/evolve.log" ]]; then
        printf '\n--- jam-svc-evolve log ---\n' >&2
        cat "$SMOKE_DIR/evolve.log" >&2
        printf '\n--- end jam-svc-evolve log ---\n' >&2
    fi
    if [[ -n "$SERVICE_PID" ]]; then
        kill "$SERVICE_PID" 2>/dev/null || true
        wait "$SERVICE_PID" 2>/dev/null || true
    fi
    docker stop "$NATS_CONTAINER" >/dev/null 2>&1 || true
    if [[ -n "$SMOKE_DIR" ]]; then
        rm -rf "$SMOKE_DIR"
    fi
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

wait_health_ok() {
    for _ in {1..120}; do
        if target/debug/jam health ping evolve --subject tool.evolve.ping --nats-url "$NATS_URL" --timeout-secs 1 >/dev/null 2>&1; then
            return 0
        fi
        sleep 0.1
    done
    printf 'evolve health ping did not become ok\n' >&2
    [[ -f "$SMOKE_DIR/evolve.log" ]] && cat "$SMOKE_DIR/evolve.log" >&2
    target/debug/jam health ping evolve --subject tool.evolve.ping --nats-url "$NATS_URL" --timeout-secs 1 >&2 || true
    exit 1
}

request_evolution() {
    python3 - "$NATS_URL" <<'PY'
import asyncio
import json
import sys
import uuid
from urllib.parse import urlparse

TRACE_ID = "01HXDK00000000000000000012"


async def read_info(reader):
    line = await asyncio.wait_for(reader.readline(), timeout=5)
    if not line.startswith(b"INFO "):
        raise RuntimeError(f"expected INFO, got {line!r}")


async def connect(nats_url):
    parsed = urlparse(nats_url)
    reader, writer = await asyncio.open_connection(parsed.hostname, parsed.port)
    await read_info(reader)
    writer.write(
        b'CONNECT {"lang":"evolve-smoke","version":"0.1.0","protocol":1,"headers":true}\r\n'
    )
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


async def hpub_json(writer, subject, reply, payload, trace_id):
    body = json.dumps(payload, separators=(",", ":")).encode()
    headers = f"NATS/1.0\r\nTrace-Id: {trace_id}\r\n\r\n".encode()
    total = len(headers) + len(body)
    writer.write(f"HPUB {subject} {reply} {len(headers)} {total}\r\n".encode() + headers + body + b"\r\n")
    await writer.drain()


async def read_message(reader, writer):
    while True:
        line = await asyncio.wait_for(reader.readline(), timeout=120)
        if line.startswith(b"PING"):
            writer.write(b"PONG\r\n")
            await writer.drain()
            continue
        parts = line.decode().strip().split()
        if not parts or parts[0] in {"+OK", "-ERR", "PONG"}:
            continue
        if parts[0] != "HMSG":
            raise RuntimeError(f"unexpected NATS line: {line!r}")
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


async def main():
    reader, writer = await connect(sys.argv[1])
    inbox = f"_INBOX.evolve_smoke.{uuid.uuid4().hex}"
    await subscribe(reader, writer, inbox)
    await hpub_json(
        writer,
        "tool.evolve.request-skill-evolution",
        inbox,
        {"skill_name": "task-types/example", "eval_source": "golden", "reason": "smoke"},
        TRACE_ID,
    )
    subject, payload = await read_message(reader, writer)
    if subject != inbox:
        raise RuntimeError(f"unexpected reply subject: {subject}")
    if "error" in payload:
        raise RuntimeError(f"evolve request failed: {payload}")
    if payload["status"] != "dry-run-complete":
        raise RuntimeError(f"unexpected status: {payload}")
    if payload["skill_name"] != "task-types/example":
        raise RuntimeError(f"wrong skill name: {payload}")
    if payload["trace_id"] != TRACE_ID:
        raise RuntimeError(f"trace id was not echoed: {payload}")
    if "DRY RUN" not in payload["stdout_tail"]:
        raise RuntimeError(f"adapter dry-run output missing: {payload}")
    print(json.dumps(payload, indent=2, sort_keys=True))


asyncio.run(main())
PY
}

need cargo
need docker
need python3
need uv

cd "$ROOT"
cargo build -p jam-cli -p jam-svc-evolve

docker run --rm -d --name "$NATS_CONTAINER" -p "127.0.0.1:${NATS_PORT}:4222" nats:2.11.0-alpine -js >/dev/null
wait_for_port

SMOKE_DIR="$(mktemp -d /tmp/jam-evolve-smoke.XXXXXX)"
mkdir -p "$SMOKE_DIR/skills/task-types" "$SMOKE_DIR/candidates"
cat >"$SMOKE_DIR/skills/task-types/example.md" <<'MD'
---
scope: task-types/example
always-loaded: false
---

# Example Skill

Use this skill for the evolve coordinator smoke.

## Procedure

1. Read the task.
2. Return a concise answer.
MD

env \
    NATS_URL="$NATS_URL" \
    JAM_REPO_ROOT="$ROOT" \
    JAM_EVOLVE_SKILLS_DIR="$SMOKE_DIR/skills" \
    JAM_EVOLVE_CANDIDATE_DIR="$SMOKE_DIR/candidates" \
    JAM_EVOLVE_DRY_RUN=true \
    "$ROOT/target/debug/jam-svc-evolve" >"$SMOKE_DIR/evolve.log" 2>&1 &
SERVICE_PID="$!"

wait_health_ok
request_evolution
printf 'evolve coordinator smoke passed\n'
