#!/usr/bin/env bash
#
# Prove `jam-svc-research` request/reply, journal publication, and the uniform
# research output directory shape in explicit fake-provider mode.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
NATS_PORT="${NATS_PORT:-42239}"
NATS_URL="nats://127.0.0.1:${NATS_PORT}"
NATS_CONTAINER="jam-smoke-nats-research-$$"
SMOKE_DIR=""
BRIDGE_PID=""
SERVICE_PID=""

cleanup() {
    local status=$?
    if [[ $status -ne 0 && -n "$SMOKE_DIR" ]]; then
        [[ -f "$SMOKE_DIR/research.log" ]] && cat "$SMOKE_DIR/research.log" >&2
        [[ -f "$SMOKE_DIR/bridge.log" ]] && cat "$SMOKE_DIR/bridge.log" >&2
    fi
    for pid in "$SERVICE_PID" "$BRIDGE_PID"; do
        if [[ -n "$pid" ]]; then
            kill "$pid" 2>/dev/null || true
            wait "$pid" 2>/dev/null || true
        fi
    done
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
        if target/debug/jam health ping research --subject tool.research.ping --nats-url "$NATS_URL" --timeout-secs 1 >/dev/null 2>&1; then
            return 0
        fi
        sleep 0.1
    done
    printf 'research health ping did not become ok\n' >&2
    cat "$SMOKE_DIR/research.log" >&2 || true
    exit 1
}

wait_bridge_ready() {
    for _ in {1..120}; do
        if rg -q 'subscribed to journal.>' "$SMOKE_DIR/bridge.log" 2>/dev/null; then
            return 0
        fi
        sleep 0.1
    done
    printf 'journal bridge did not become ready\n' >&2
    cat "$SMOKE_DIR/bridge.log" >&2 || true
    exit 1
}

request_research() {
    python3 - "$NATS_URL" "$SMOKE_DIR/result.json" <<'PY'
import asyncio
import json
import sys
import uuid
from pathlib import Path
from urllib.parse import urlparse

TRACE_ID = "01HXDK00000000000000000013"


async def read_info(reader):
    line = await asyncio.wait_for(reader.readline(), timeout=5)
    if not line.startswith(b"INFO "):
        raise RuntimeError(f"expected INFO, got {line!r}")


async def connect(nats_url):
    parsed = urlparse(nats_url)
    reader, writer = await asyncio.open_connection(parsed.hostname, parsed.port)
    await read_info(reader)
    writer.write(
        b'CONNECT {"lang":"research-smoke","version":"0.1.0","protocol":1,"headers":true}\r\n'
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
        line = await asyncio.wait_for(reader.readline(), timeout=30)
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
    inbox = f"_INBOX.research_smoke.{uuid.uuid4().hex}"
    await subscribe(reader, writer, inbox)
    await hpub_json(
        writer,
        "tool.research.request-research",
        inbox,
        {
            "question": "What should the fake research smoke prove?",
            "tier": "deep",
            "scope": "blueberry/research-smoke",
        },
        TRACE_ID,
    )
    subject, payload = await read_message(reader, writer)
    if subject != inbox:
        raise RuntimeError(f"unexpected reply subject: {subject}")
    if "error" in payload:
        raise RuntimeError(f"research request failed: {payload}")
    if payload["status"] != "completed":
        raise RuntimeError(f"unexpected status: {payload}")
    if payload["provider"] != "fake-deep":
        raise RuntimeError(f"unexpected provider: {payload}")
    if payload["trace_id"] != TRACE_ID:
        raise RuntimeError(f"trace id was not echoed: {payload}")
    Path(sys.argv[2]).write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n")
    print(json.dumps(payload, indent=2, sort_keys=True))


asyncio.run(main())
PY
}

need cargo
need docker
need python3

cd "$ROOT"
cargo build -p jam-cli -p jam-nats-bridge -p jam-svc-research

docker run --rm -d --name "$NATS_CONTAINER" -p "127.0.0.1:${NATS_PORT}:4222" nats:2.11.0-alpine -js >/dev/null
wait_for_port

SMOKE_DIR="$(mktemp -d /tmp/jam-research-smoke.XXXXXX)"
mkdir -p "$SMOKE_DIR/graph"
JAM_HOME="$SMOKE_DIR/jam-home" NATS_URL="$NATS_URL" "$ROOT/target/debug/jam-nats-bridge" >"$SMOKE_DIR/bridge.log" 2>&1 &
BRIDGE_PID="$!"

env \
    NATS_URL="$NATS_URL" \
    JAM_HOME="$SMOKE_DIR/jam-home" \
    JAM_RESEARCH_ROOT="$SMOKE_DIR/jam-home/research" \
    JAM_RESEARCH_TEMPYR_GRAPH_DIR="$SMOKE_DIR/graph" \
    JAM_RESEARCH_FAKE_PROVIDER=true \
    "$ROOT/target/debug/jam-svc-research" >"$SMOKE_DIR/research.log" 2>&1 &
SERVICE_PID="$!"

wait_health_ok
wait_bridge_ready
request_research

OUTPUT_DIR="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["output_dir"])' "$SMOKE_DIR/result.json")"
for file in report.md findings.json sources.jsonl transcript.jsonl metadata.json; do
    test -f "$OUTPUT_DIR/$file"
done
TEMPYR_NOTE="$SMOKE_DIR/graph/notes/note-research-blueberry-research-smoke-00000013.md"
test -f "$TEMPYR_NOTE"
rg -q 'research_provider: fake-deep' "$TEMPYR_NOTE"
rg -q 'Fake provider smoke output' "$TEMPYR_NOTE"

for _ in {1..80}; do
    if rg -q '"status":"completed"|"status": "completed"' "$SMOKE_DIR/jam-home/journal" 2>/dev/null && \
       rg -q '"node_id":"note-research-blueberry-research-smoke-00000013"|"node_id": "note-research-blueberry-research-smoke-00000013"' "$SMOKE_DIR/jam-home/journal" 2>/dev/null; then
        printf 'research service smoke passed\n'
        exit 0
    fi
    sleep 0.1
done

printf 'research completed journal entry did not land\n' >&2
find "$SMOKE_DIR/jam-home" -maxdepth 5 -type f -print >&2
find "$SMOKE_DIR/jam-home/journal" -type f -maxdepth 3 -print -exec sed -n '1,20p' {} \; >&2 || true
exit 1
