#!/usr/bin/env bash
#
# Prove the §6.2 / §17.3 Docker sandbox path with live NATS:
# jam-svc-session accepts `sandbox_backend=docker`, launches Picker commands
# through `docker run`, mounts the worktree read-write, mounts git metadata
# read-only, and applies the hardened network policy.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
NATS_PORT="${NATS_PORT:-42232}"
NATS_URL="nats://127.0.0.1:${NATS_PORT}"
NATS_CONTAINER="jam-smoke-nats-docker-sandbox-$$"
TASK_ID="task-docker-sandbox-smoke-$$"
HARDENED_TASK_ID="task-hardened-docker-smoke-$$"
SMOKE_HOME=""
SESSION_PID=""

cleanup() {
    if [[ -n "$SESSION_PID" ]]; then
        kill "$SESSION_PID" 2>/dev/null || true
        wait "$SESSION_PID" 2>/dev/null || true
    fi
    docker rm -f "$(docker ps -aq --filter "label=org.jamboree.task=${TASK_ID}")" >/dev/null 2>&1 || true
    docker rm -f "$(docker ps -aq --filter "label=org.jamboree.task=${HARDENED_TASK_ID}")" >/dev/null 2>&1 || true
    docker stop "$NATS_CONTAINER" >/dev/null 2>&1 || true
    if [[ -n "$SMOKE_HOME" ]]; then
        rm -rf "$SMOKE_HOME"
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
    for _ in {1..100}; do
        if target/debug/jam health ping session --subject tool.session.ping --nats-url "$NATS_URL" --timeout-secs 1 >/dev/null 2>&1; then
            return 0
        fi
        sleep 0.1
    done
    printf 'session health ping did not become ok\n' >&2
    target/debug/jam health ping session --subject tool.session.ping --nats-url "$NATS_URL" --timeout-secs 1 >&2 || true
    exit 1
}

wait_for_default_capture() {
    local capture="$1"
    for _ in {1..120}; do
        if grep -q '^pwd=/work$' "$capture" 2>/dev/null \
            && grep -q '^session=codex-cli:' "$capture" 2>/dev/null \
            && grep -q '^profile=default$' "$capture" 2>/dev/null \
            && grep -q '^wrote_from_container=1$' "$capture" 2>/dev/null; then
            return 0
        fi
        sleep 0.1
    done
    printf 'Docker Picker capture did not appear\n' >&2
    [[ -f "$capture" ]] && cat "$capture" >&2
    [[ -f "$SMOKE_HOME/logs/session.log" ]] && cat "$SMOKE_HOME/logs/session.log" >&2
    exit 1
}

wait_for_hardened_capture() {
    local capture="$1"
    for _ in {1..120}; do
        if grep -q '^profile=hardened$' "$capture" 2>/dev/null \
            && grep -q '^network_blocked=1$' "$capture" 2>/dev/null; then
            return 0
        fi
        sleep 0.1
    done
    printf 'Hardened Docker Picker did not block outbound network\n' >&2
    [[ -f "$capture" ]] && cat "$capture" >&2
    [[ -f "$SMOKE_HOME/logs/session.log" ]] && cat "$SMOKE_HOME/logs/session.log" >&2
    exit 1
}

start_session_service() {
    local log="$1"
    shift
    "$@" >"$log" 2>&1 &
    local pid="$!"
    for _ in {1..100}; do
        if grep -q "subscribed" "$log"; then
            printf '%s\n' "$pid"
            return 0
        fi
        if ! kill -0 "$pid" 2>/dev/null; then
            cat "$log" >&2
            printf 'jam-svc-session exited before subscribing\n' >&2
            exit 1
        fi
        sleep 0.1
    done
    cat "$log" >&2
    printf 'jam-svc-session did not subscribe\n' >&2
    exit 1
}

need docker
need git
need python3

cd "$ROOT"
cargo build -p jam-cli -p jam-svc-session

docker run --rm -d --name "$NATS_CONTAINER" -p "127.0.0.1:${NATS_PORT}:4222" nats:2.11.0-alpine -js >/dev/null
wait_for_port

SMOKE_HOME="$(mktemp -d /tmp/jam-docker-sandbox.XXXXXX)"
WORKTREE="$SMOKE_HOME/worktree"
CAPTURE="$WORKTREE/.jam/${TASK_ID}.txt"
HARDENED_CAPTURE="$WORKTREE/.jam/${HARDENED_TASK_ID}.txt"
RESULT="$SMOKE_HOME/result.json"
mkdir -p "$WORKTREE/.jam" "$SMOKE_HOME/logs"
git -C "$WORKTREE" init -q

cat >"$WORKTREE/.jam/docker-picker.sh" <<'SH'
#!/bin/sh
set -eu

capture="/work/.jam/${JAM_TASK_ID}.txt"

{
    printf 'pwd=%s\n' "$(pwd)"
    printf 'home=%s\n' "${HOME:-}"
    printf 'session=%s\n' "${JAM_SESSION_ID:-}"
    printf 'task=%s\n' "${JAM_TASK_ID:-}"
    printf 'backend=%s\n' "${JAM_SANDBOX_BACKEND:-}"
    printf 'profile=%s\n' "${JAM_SANDBOX_PROFILE:-}"
    printf 'wrote_from_container=1\n'
    printf 'root_has_host_home='
    if [ -d /home/caleb ]; then
        printf '1\n'
    else
        printf '0\n'
    fi
    if [ "${JAM_SANDBOX_PROFILE:-}" = "hardened" ]; then
        printf 'network_blocked='
        if wget -q -T 3 -O /tmp/example.html http://example.org; then
            printf '0\n'
        else
            printf '1\n'
        fi
    fi
} > "$capture"

sleep 30
SH
chmod +x "$WORKTREE/.jam/docker-picker.sh"

SESSION_PID="$(
    start_session_service \
        "$SMOKE_HOME/logs/session.log" \
        env \
        NATS_URL="$NATS_URL" \
        JAM_SESSION_USE_SUDO=false \
        JAM_DOCKER_IMAGE="${JAM_DOCKER_IMAGE:-alpine:3.20}" \
        JAM_SESSION_DRY_RUN_COMMAND="/work/.jam/docker-picker.sh" \
        "$ROOT/target/debug/jam-svc-session"
)"

wait_health_ok

SMOKE_WORKTREE="$WORKTREE" \
SMOKE_RESULT="$RESULT" \
SMOKE_TASK_ID="$TASK_ID" \
SMOKE_HARDENED_TASK_ID="$HARDENED_TASK_ID" \
python3 - "$NATS_URL" <<'PY'
import asyncio
import json
import os
import sys
import uuid
from dataclasses import dataclass
from pathlib import Path
from urllib.parse import urlparse

TRACE_SPAWN = "01HXDK00000000000000000010"
TRACE_HARDENED = "01HXDK00000000000000000011"


@dataclass
class NatsMessage:
    subject: str
    payload: object
    reply: str | None = None


async def read_info(reader: asyncio.StreamReader) -> None:
    line = await asyncio.wait_for(reader.readline(), timeout=5)
    if not line.startswith(b"INFO "):
        raise RuntimeError(f"expected INFO, got {line!r}")


async def connect(nats_url: str) -> tuple[asyncio.StreamReader, asyncio.StreamWriter]:
    parsed = urlparse(nats_url)
    reader, writer = await asyncio.open_connection(parsed.hostname, parsed.port)
    await read_info(reader)
    writer.write(
        b'CONNECT {"lang":"docker-sandbox-smoke","version":"0.1.0","protocol":1,"headers":true}\r\n'
    )
    await writer.drain()
    return reader, writer


async def wait_pong(reader: asyncio.StreamReader, writer: asyncio.StreamWriter) -> None:
    while True:
        line = await asyncio.wait_for(reader.readline(), timeout=5)
        if line.startswith(b"PONG"):
            return
        if line.startswith(b"PING"):
            writer.write(b"PONG\r\n")
            await writer.drain()


async def subscribe(
    reader: asyncio.StreamReader,
    writer: asyncio.StreamWriter,
    subject: str,
    sid: int = 1,
) -> None:
    writer.write(f"SUB {subject} {sid}\r\nPING\r\n".encode())
    await writer.drain()
    await wait_pong(reader, writer)


async def read_message(
    reader: asyncio.StreamReader,
    writer: asyncio.StreamWriter,
    timeout: float = 15.0,
) -> NatsMessage:
    while True:
        line = await asyncio.wait_for(reader.readline(), timeout=timeout)
        if not line:
            raise RuntimeError("NATS connection closed")
        parts = line.decode().strip().split()
        if not parts or parts[0] in {"+OK", "-ERR", "PONG"}:
            continue
        if parts[0] == "PING":
            writer.write(b"PONG\r\n")
            await writer.drain()
            continue
        if parts[0] == "HMSG":
            subject = parts[1]
            if len(parts) == 5:
                reply = None
                header_len = int(parts[3])
                total_len = int(parts[4])
            else:
                reply = parts[3]
                header_len = int(parts[4])
                total_len = int(parts[5])
            data = await reader.readexactly(total_len + 2)
            payload = data[header_len:total_len]
            return NatsMessage(subject, json.loads(payload), reply)
        if parts[0] == "MSG":
            subject = parts[1]
            if len(parts) == 4:
                reply = None
                total_len = int(parts[3])
            else:
                reply = parts[3]
                total_len = int(parts[4])
            data = await reader.readexactly(total_len + 2)
            return NatsMessage(subject, json.loads(data[:total_len]), reply)
        raise RuntimeError(f"unexpected NATS protocol line: {line!r}")


async def pub_json(writer: asyncio.StreamWriter, subject: str, payload: object) -> None:
    body = json.dumps(payload, separators=(",", ":")).encode()
    writer.write(f"PUB {subject} {len(body)}\r\n".encode() + body + b"\r\n")
    await writer.drain()


async def hpub_json(
    writer: asyncio.StreamWriter,
    subject: str,
    reply: str | None,
    payload: object,
    trace_id: str,
) -> None:
    body = json.dumps(payload, separators=(",", ":")).encode()
    headers = f"NATS/1.0\r\nTrace-Id: {trace_id}\r\n\r\n".encode()
    total = len(headers) + len(body)
    if reply is None:
        command = f"HPUB {subject} {len(headers)} {total}\r\n".encode()
    else:
        command = f"HPUB {subject} {reply} {len(headers)} {total}\r\n".encode()
    writer.write(command + headers + body + b"\r\n")
    await writer.drain()


async def request_json(nats_url: str, subject: str, payload: object, trace_id: str) -> object:
    reader, writer = await connect(nats_url)
    inbox = f"_INBOX.docker_sandbox.{uuid.uuid4().hex}"
    await subscribe(reader, writer, inbox)
    await hpub_json(writer, subject, inbox, payload, trace_id)
    try:
        while True:
            message = await read_message(reader, writer)
            if message.subject == inbox:
                return message.payload
    finally:
        writer.close()
        await writer.wait_closed()


async def worktree_responder(nats_url: str, worktree_path: str, ready: asyncio.Event) -> None:
    reader, writer = await connect(nats_url)
    await subscribe(reader, writer, "tool.worktree.create")
    ready.set()
    while True:
        message = await read_message(reader, writer, timeout=30.0)
        if message.subject != "tool.worktree.create" or message.reply is None:
            continue
        request = message.payload
        await pub_json(
            writer,
            message.reply,
            {
                "task_id": request["task_id"],
                "project": request.get("project", "blueberry"),
                "worktree_path": worktree_path,
            },
        )


async def main() -> None:
    nats_url = sys.argv[1]
    worktree_path = os.environ["SMOKE_WORKTREE"]
    result_path = Path(os.environ["SMOKE_RESULT"])
    task_id = os.environ["SMOKE_TASK_ID"]
    hardened_task_id = os.environ["SMOKE_HARDENED_TASK_ID"]
    ready = asyncio.Event()
    worktree_task = asyncio.create_task(worktree_responder(nats_url, worktree_path, ready))
    await asyncio.wait_for(ready.wait(), timeout=5)

    spawn = await request_json(
        nats_url,
        "tool.session.spawn-picker",
        {
            "task_id": task_id,
            "project": "blueberry",
            "harness": "codex-cli",
            "sandbox_backend": "docker",
            "sandbox_profile": "default",
            "initial_prompt": "dry-run docker sandbox smoke",
            "dry_run": True,
        },
        TRACE_SPAWN,
    )
    if "error" in spawn:
        raise RuntimeError(f"spawn-picker failed: {spawn}")
    if spawn["sandbox_backend"] != "docker":
        raise RuntimeError(f"spawn-picker returned wrong backend: {spawn}")

    hardened = await request_json(
        nats_url,
        "tool.session.spawn-picker",
        {
            "task_id": hardened_task_id,
            "project": "blueberry",
            "harness": "codex-cli",
            "sandbox_backend": "docker",
            "sandbox_profile": "hardened",
            "initial_prompt": "dry-run hardened docker sandbox smoke",
            "dry_run": True,
        },
        TRACE_HARDENED,
    )
    if "error" in hardened:
        raise RuntimeError(f"hardened spawn-picker failed: {hardened}")
    if hardened["sandbox_profile"] != "hardened":
        raise RuntimeError(f"spawn-picker returned wrong profile: {hardened}")

    result_path.write_text(
        json.dumps({"default": spawn, "hardened": hardened}, indent=2, sort_keys=True) + "\n"
    )

    worktree_task.cancel()


asyncio.run(main())
PY

wait_for_default_capture "$CAPTURE"
wait_for_hardened_capture "$HARDENED_CAPTURE"
cat "$CAPTURE"
cat "$HARDENED_CAPTURE"
cat "$RESULT"
printf 'docker sandbox backend smoke passed\n'
