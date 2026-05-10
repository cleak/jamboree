#!/usr/bin/env bash
#
# Prove the §6.4 local-backend resource limits with live NATS:
# jam-svc-session wraps local Pickers in user systemd scopes, applies CPU and
# memory properties by task class, and runs risky-architecture tasks under
# ionice idle class.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
NATS_PORT="${NATS_PORT:-42233}"
NATS_URL="nats://127.0.0.1:${NATS_PORT}"
NATS_CONTAINER="jam-smoke-nats-cgroup-$$"
COMPILE_TASK_ID="task-cgroup-compile-heavy-$$"
RISKY_TASK_ID="task-cgroup-risky-$$"
SMOKE_HOME=""
SESSION_PID=""

cleanup() {
    if [[ -n "$SESSION_PID" ]]; then
        kill "$SESSION_PID" 2>/dev/null || true
        wait "$SESSION_PID" 2>/dev/null || true
    fi
    if [[ -n "$SMOKE_HOME" ]]; then
        pkill -f "$SMOKE_HOME/local-picker.sh" 2>/dev/null || true
        rm -rf "$SMOKE_HOME"
    fi
    docker stop "$NATS_CONTAINER" >/dev/null 2>&1 || true
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

wait_for_capture() {
    local capture="$1"
    for _ in {1..120}; do
        if grep -q '^session=codex-cli:' "$capture" 2>/dev/null; then
            return 0
        fi
        sleep 0.1
    done
    printf 'local Picker capture did not appear: %s\n' "$capture" >&2
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

scope_from_result() {
    local key="$1"
    python3 - "$RESULT" "$key" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    data = json.load(handle)
scope = data[sys.argv[2]].get("resource_scope")
if not scope:
    raise SystemExit(f"{sys.argv[2]} response did not include resource_scope")
print(scope)
PY
}

need docker
need git
need ionice
need python3
need systemctl
need systemd-run

cd "$ROOT"
cargo build -p jam-cli -p jam-svc-session

docker run --rm -d --name "$NATS_CONTAINER" -p "127.0.0.1:${NATS_PORT}:4222" nats:2.11.0-alpine -js >/dev/null
wait_for_port

SMOKE_HOME="$(mktemp -d /tmp/jam-cgroup-limits.XXXXXX)"
WORKTREE="$SMOKE_HOME/worktree"
RESULT="$SMOKE_HOME/result.json"
COMPILE_CAPTURE="$WORKTREE/.jam/${COMPILE_TASK_ID}.txt"
RISKY_CAPTURE="$WORKTREE/.jam/${RISKY_TASK_ID}.txt"
mkdir -p "$WORKTREE/.jam" "$SMOKE_HOME/logs"
git -C "$WORKTREE" init -q

cat >"$SMOKE_HOME/local-picker.sh" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

capture=".jam/${JAM_TASK_ID}.txt"
{
    printf 'pwd=%s\n' "$(pwd)"
    printf 'session=%s\n' "${JAM_SESSION_ID:-}"
    printf 'task=%s\n' "${JAM_TASK_ID:-}"
    printf 'class=%s\n' "${JAM_TASK_CLASS:-}"
    printf 'scope=%s\n' "$(sed 's/0:://' /proc/self/cgroup)"
    printf 'ionice=%s\n' "$(ionice -p "$$")"
} > "$capture"

sleep 30
SH
chmod +x "$SMOKE_HOME/local-picker.sh"

SESSION_PID="$(
    start_session_service \
        "$SMOKE_HOME/logs/session.log" \
        env \
        NATS_URL="$NATS_URL" \
        JAM_SESSION_USE_SUDO=false \
        JAM_SESSION_USE_SYSTEMD_SCOPE=true \
        JAM_SESSION_DRY_RUN_COMMAND="$SMOKE_HOME/local-picker.sh" \
        "$ROOT/target/debug/jam-svc-session"
)"

wait_health_ok

SMOKE_WORKTREE="$WORKTREE" \
SMOKE_RESULT="$RESULT" \
SMOKE_COMPILE_TASK_ID="$COMPILE_TASK_ID" \
SMOKE_RISKY_TASK_ID="$RISKY_TASK_ID" \
python3 - "$NATS_URL" <<'PY'
import asyncio
import json
import os
import sys
import uuid
from dataclasses import dataclass
from pathlib import Path
from urllib.parse import urlparse

TRACE_COMPILE = "01HXCG00000000000000000010"
TRACE_RISKY = "01HXCG00000000000000000011"


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
        b'CONNECT {"lang":"cgroup-resource-smoke","version":"0.1.0","protocol":1,"headers":true}\r\n'
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
    inbox = f"_INBOX.cgroup_resource.{uuid.uuid4().hex}"
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


async def spawn_task(
    nats_url: str,
    task_id: str,
    task_class: str,
    trace_id: str,
) -> object:
    response = await request_json(
        nats_url,
        "tool.session.spawn-picker",
        {
            "task_id": task_id,
            "project": "blueberry",
            "harness": "codex-cli",
            "sandbox_backend": "local",
            "sandbox_profile": "default",
            "task_class": task_class,
            "initial_prompt": "dry-run cgroup resource smoke",
            "dry_run": True,
        },
        trace_id,
    )
    if "error" in response:
        raise RuntimeError(f"spawn-picker failed for {task_class}: {response}")
    if not response.get("resource_scope"):
        raise RuntimeError(f"spawn-picker did not return resource_scope: {response}")
    return response


async def main() -> None:
    nats_url = sys.argv[1]
    worktree_path = os.environ["SMOKE_WORKTREE"]
    result_path = Path(os.environ["SMOKE_RESULT"])
    ready = asyncio.Event()
    worktree_task = asyncio.create_task(worktree_responder(nats_url, worktree_path, ready))
    await asyncio.wait_for(ready.wait(), timeout=5)

    compile_heavy = await spawn_task(
        nats_url,
        os.environ["SMOKE_COMPILE_TASK_ID"],
        "compile-heavy-rust",
        TRACE_COMPILE,
    )
    risky = await spawn_task(
        nats_url,
        os.environ["SMOKE_RISKY_TASK_ID"],
        "risky-architecture",
        TRACE_RISKY,
    )
    result_path.write_text(
        json.dumps({"compile": compile_heavy, "risky": risky}, indent=2, sort_keys=True) + "\n"
    )
    worktree_task.cancel()


asyncio.run(main())
PY

wait_for_capture "$COMPILE_CAPTURE"
wait_for_capture "$RISKY_CAPTURE"

COMPILE_SCOPE="$(scope_from_result compile)"
RISKY_SCOPE="$(scope_from_result risky)"
COMPILE_PROPS="$SMOKE_HOME/compile.scope"
RISKY_PROPS="$SMOKE_HOME/risky.scope"
systemctl --user show "$COMPILE_SCOPE" -p CPUQuotaPerSecUSec -p MemoryMax > "$COMPILE_PROPS"
systemctl --user show "$RISKY_SCOPE" -p CPUQuotaPerSecUSec -p MemoryMax > "$RISKY_PROPS"

grep -q '^CPUQuotaPerSecUSec=8s$' "$COMPILE_PROPS"
grep -q '^MemoryMax=8589934592$' "$COMPILE_PROPS"
grep -q '^CPUQuotaPerSecUSec=1s$' "$RISKY_PROPS"
grep -q '^MemoryMax=8589934592$' "$RISKY_PROPS"
grep -q '^ionice=idle' "$RISKY_CAPTURE"

cat "$COMPILE_CAPTURE"
cat "$RISKY_CAPTURE"
cat "$COMPILE_PROPS"
cat "$RISKY_PROPS"
cat "$RESULT"
printf 'cgroup resource limits smoke passed\n'
