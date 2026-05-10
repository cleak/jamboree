#!/usr/bin/env bash
#
# Prove the §5.7 queue/interrupt path with live NATS:
# jam-svc-message publishes session-scoped commands, jam-svc-session receives
# them for a running Picker, writes both messages to Picker stdin, and emits the
# expected status lifecycle on picker.<session-id>.msg.status.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
NATS_PORT="${NATS_PORT:-42231}"
NATS_URL="nats://127.0.0.1:${NATS_PORT}"
CONTAINER="jam-smoke-nats-message-modes-$$"
SMOKE_HOME=""
SESSION_PID=""
MESSAGE_PID=""

cleanup() {
    if [[ -n "$SESSION_PID" ]]; then
        kill "$SESSION_PID" 2>/dev/null || true
        wait "$SESSION_PID" 2>/dev/null || true
    fi
    if [[ -n "$MESSAGE_PID" ]]; then
        kill "$MESSAGE_PID" 2>/dev/null || true
        wait "$MESSAGE_PID" 2>/dev/null || true
    fi
    if [[ -n "$SMOKE_HOME" ]]; then
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

wait_health_ok() {
    local service="$1"
    local subject="$2"
    for _ in {1..100}; do
        if target/debug/jam health ping "$service" --subject "$subject" --nats-url "$NATS_URL" --timeout-secs 1 >/dev/null 2>&1; then
            return 0
        fi
        sleep 0.1
    done
    printf 'health ping did not become ok: %s\n' "$subject" >&2
    target/debug/jam health ping "$service" --subject "$subject" --nats-url "$NATS_URL" --timeout-secs 1 >&2 || true
    exit 1
}

wait_for_capture() {
    local capture="$1"
    for _ in {1..100}; do
        if grep -q "queued smoke message" "$capture" 2>/dev/null \
            && grep -q "interrupt smoke message" "$capture" 2>/dev/null; then
            return 0
        fi
        sleep 0.1
    done
    printf 'Picker stdin capture did not include both messages\n' >&2
    [[ -f "$capture" ]] && cat "$capture" >&2
    exit 1
}

start_service() {
    local name="$1"
    local log="$2"
    shift 2
    "$@" >"$log" 2>&1 &
    local pid="$!"
    for _ in {1..100}; do
        if grep -q "subscribed" "$log"; then
            printf '%s\n' "$pid"
            return 0
        fi
        if ! kill -0 "$pid" 2>/dev/null; then
            cat "$log" >&2
            printf '%s exited before subscribing\n' "$name" >&2
            exit 1
        fi
        sleep 0.1
    done
    cat "$log" >&2
    printf '%s did not subscribe\n' "$name" >&2
    exit 1
}

need docker
need git
need python3

cd "$ROOT"
cargo build -p jam-cli -p jam-svc-session -p jam-svc-message

docker run --rm -d --name "$CONTAINER" -p "127.0.0.1:${NATS_PORT}:4222" nats:2.11.0-alpine -js >/dev/null
wait_for_port

SMOKE_HOME="$(mktemp -d /tmp/jam-message-modes.XXXXXX)"
WORKTREE="$SMOKE_HOME/worktree"
CAPTURE="$SMOKE_HOME/picker-stdin.txt"
RESULT="$SMOKE_HOME/result.json"
PICKER_SCRIPT="$SMOKE_HOME/picker-stdin-capture.sh"
mkdir -p "$WORKTREE" "$SMOKE_HOME/logs"
git -C "$WORKTREE" init -q

cat >"$PICKER_SCRIPT" <<SH
#!/usr/bin/env bash
set -euo pipefail

: > "$CAPTURE"
seen=0
while IFS= read -r line; do
    printf '%s\n' "\$line" >> "$CAPTURE"
    if [[ "\$line" == "</jamboree-message>" ]]; then
        seen=\$((seen + 1))
        if [[ "\$seen" -ge 2 ]]; then
            break
        fi
    fi
done
sleep 0.2
SH
chmod +x "$PICKER_SCRIPT"

SESSION_PID="$(
    start_service \
        jam-svc-session \
        "$SMOKE_HOME/logs/session.log" \
        env \
        NATS_URL="$NATS_URL" \
        JAM_SESSION_USE_SUDO=false \
        JAM_SESSION_DRY_RUN_COMMAND="$PICKER_SCRIPT" \
        "$ROOT/target/debug/jam-svc-session"
)"
MESSAGE_PID="$(
    start_service \
        jam-svc-message \
        "$SMOKE_HOME/logs/message.log" \
        env \
        NATS_URL="$NATS_URL" \
        "$ROOT/target/debug/jam-svc-message"
)"

wait_health_ok session tool.session.ping
wait_health_ok message tool.message.ping

SMOKE_WORKTREE="$WORKTREE" \
SMOKE_RESULT="$RESULT" \
python3 - "$NATS_URL" <<'PY'
import asyncio
import json
import os
import sys
import uuid
from dataclasses import dataclass
from pathlib import Path
from urllib.parse import urlparse

TRACE_SPAWN = "01HXKJ00000000000000000010"
TRACE_QUEUE = "01HXKJ00000000000000000011"
TRACE_INTERRUPT = "01HXKJ00000000000000000012"
QUEUE_TEXT = "queued smoke message"
INTERRUPT_TEXT = "interrupt smoke message"


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
        b'CONNECT {"lang":"message-modes-smoke","version":"0.1.0","protocol":1,"headers":true}\r\n'
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
    inbox = f"_INBOX.message_modes.{uuid.uuid4().hex}"
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


async def status_collector(
    nats_url: str,
    queue: asyncio.Queue[object],
    ready: asyncio.Event,
) -> None:
    reader, writer = await connect(nats_url)
    await subscribe(reader, writer, "picker.>")
    ready.set()
    while True:
        message = await read_message(reader, writer, timeout=30.0)
        if message.subject.endswith(".msg.status"):
            queue.put_nowait(message.payload)


async def wait_for_statuses(
    queue: asyncio.Queue[object],
    queue_id: str,
    interrupt_id: str,
) -> dict[str, list[str]]:
    expected = {
        queue_id: {"queued", "delivered"},
        interrupt_id: {"interrupt-requested", "interrupt-accepted", "delivered"},
    }
    seen: dict[str, list[str]] = {queue_id: [], interrupt_id: []}
    deadline = asyncio.get_running_loop().time() + 15
    while asyncio.get_running_loop().time() < deadline:
        item = await asyncio.wait_for(queue.get(), timeout=1.0)
        message_id = item.get("message_id")
        status = item.get("status")
        if message_id in seen and status not in seen[message_id]:
            seen[message_id].append(status)
        if all(expected[mid].issubset(set(statuses)) for mid, statuses in seen.items()):
            return seen
    raise RuntimeError(f"timed out waiting for message statuses, saw {seen}")


async def main() -> None:
    nats_url = sys.argv[1]
    worktree_path = os.environ["SMOKE_WORKTREE"]
    result_path = Path(os.environ["SMOKE_RESULT"])
    status_queue: asyncio.Queue[object] = asyncio.Queue()
    worktree_ready = asyncio.Event()
    status_ready = asyncio.Event()
    worktree_task = asyncio.create_task(worktree_responder(nats_url, worktree_path, worktree_ready))
    status_task = asyncio.create_task(status_collector(nats_url, status_queue, status_ready))
    await asyncio.wait_for(worktree_ready.wait(), timeout=5)
    await asyncio.wait_for(status_ready.wait(), timeout=5)

    spawn = await request_json(
        nats_url,
        "tool.session.spawn-picker",
        {
            "task_id": "task-message-modes-ui-smoke",
            "project": "blueberry",
            "harness": "codex-cli",
            "initial_prompt": "dry-run message delivery smoke",
            "dry_run": True,
        },
        TRACE_SPAWN,
    )
    if "error" in spawn:
        raise RuntimeError(f"spawn-picker failed: {spawn}")
    session_id = spawn["session_id"]

    queue = await request_json(
        nats_url,
        "tool.message.enqueue-message",
        {"session_id": session_id, "text": QUEUE_TEXT, "from": "human:caleb"},
        TRACE_QUEUE,
    )
    interrupt = await request_json(
        nats_url,
        "tool.message.interrupt-with-message",
        {"session_id": session_id, "text": INTERRUPT_TEXT, "from": "human:caleb"},
        TRACE_INTERRUPT,
    )
    if "error" in queue:
        raise RuntimeError(f"enqueue-message failed: {queue}")
    if "error" in interrupt:
        raise RuntimeError(f"interrupt-with-message failed: {interrupt}")

    seen = await wait_for_statuses(status_queue, queue["message_id"], interrupt["message_id"])
    result_path.write_text(
        json.dumps(
            {
                "session_id": session_id,
                "queue_statuses": seen[queue["message_id"]],
                "interrupt_statuses": seen[interrupt["message_id"]],
            },
            indent=2,
            sort_keys=True,
        )
        + "\n"
    )
    worktree_task.cancel()
    status_task.cancel()


asyncio.run(main())
PY

wait_for_capture "$CAPTURE"
cat "$RESULT"
printf 'message modes delivery smoke passed\n'
