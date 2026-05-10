#!/usr/bin/env bash
#
# Prove `jam-svc-search` request/reply, backend routing, Brave cooldown, and
# routing journal publication against deterministic local fake backends.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
NATS_PORT="${NATS_PORT:-42240}"
HTTP_PORT="${HTTP_PORT:-18081}"
NATS_URL="nats://127.0.0.1:${NATS_PORT}"
NATS_CONTAINER="jam-smoke-nats-search-$$"
SMOKE_DIR=""
BRIDGE_PID=""
SERVICE_PID=""
HTTP_PID=""

cleanup() {
    local status=$?
    if [[ $status -ne 0 && -n "$SMOKE_DIR" ]]; then
        [[ -f "$SMOKE_DIR/search.log" ]] && cat "$SMOKE_DIR/search.log" >&2
        [[ -f "$SMOKE_DIR/bridge.log" ]] && cat "$SMOKE_DIR/bridge.log" >&2
        [[ -f "$SMOKE_DIR/http.log" ]] && cat "$SMOKE_DIR/http.log" >&2
    fi
    for pid in "$SERVICE_PID" "$BRIDGE_PID" "$HTTP_PID"; do
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
    python3 - "$1" <<'PY'
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
raise SystemExit(f"port {port} did not become reachable")
PY
}

wait_health_ok() {
    for _ in {1..120}; do
        if target/debug/jam health ping search --subject tool.search.ping --nats-url "$NATS_URL" --timeout-secs 1 >/dev/null 2>&1; then
            return 0
        fi
        sleep 0.1
    done
    printf 'search health ping did not become ok\n' >&2
    cat "$SMOKE_DIR/search.log" >&2 || true
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

start_fake_http() {
    python3 - "$HTTP_PORT" >"$SMOKE_DIR/http.log" 2>&1 <<'PY' &
import json
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, urlparse


class Handler(BaseHTTPRequestHandler):
    server_version = "JamSearchSmoke/0.1"

    def log_message(self, fmt, *args):
        print(fmt % args, flush=True)

    def send_json(self, status, payload):
        body = json.dumps(payload, separators=(",", ":")).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        parsed = urlparse(self.path)
        query = parse_qs(parsed.query)
        q = query.get("q", [""])[0]
        if parsed.path == "/brave/search":
            if "fail" in q:
                self.send_json(503, {"message": "forced Brave failure"})
                return
            self.send_json(
                200,
                {
                    "web": {
                        "results": [
                            {
                                "title": "Brave Fake",
                                "url": "https://example.org/brave",
                                "description": f"fake brave result for {q}",
                            }
                        ]
                    }
                },
            )
            return
        if parsed.path == "/searx/search":
            self.send_json(
                200,
                {
                    "results": [
                        {
                            "title": "SearXNG Fake",
                            "url": "https://example.org/searxng",
                            "content": f"private fake result for {q}",
                        }
                    ]
                },
            )
            return
        self.send_json(404, {"message": f"unexpected GET {parsed.path}"})

    def do_POST(self):
        parsed = urlparse(self.path)
        length = int(self.headers.get("Content-Length", "0"))
        raw = self.rfile.read(length) if length else b"{}"
        try:
            body = json.loads(raw)
        except json.JSONDecodeError:
            body = {}
        if parsed.path == "/linkup/search":
            self.send_json(
                200,
                {
                    "results": [
                        {
                            "name": "Linkup Fake",
                            "url": "https://example.org/linkup",
                            "content": f"source-backed fake result for {body.get('q', '')}",
                        }
                    ]
                },
            )
            return
        if parsed.path == "/firecrawl/scrape":
            self.send_json(
                200,
                {
                    "success": True,
                    "data": {
                        "markdown": "# Firecrawl Fake\nBody from fake Firecrawl scrape.",
                        "metadata": {
                            "title": "Firecrawl Fake",
                            "sourceURL": body.get("url", "https://example.org/fire"),
                        },
                        "links": ["https://example.org/next"],
                        "images": ["https://example.org/image.png"],
                    },
                },
            )
            return
        self.send_json(404, {"message": f"unexpected POST {parsed.path}"})


port = int(sys.argv[1])
server = ThreadingHTTPServer(("127.0.0.1", port), Handler)
server.serve_forever()
PY
    HTTP_PID="$!"
}

request_search() {
    python3 - "$NATS_URL" "$SMOKE_DIR/result.json" <<'PY'
import asyncio
import json
import sys
import uuid
from pathlib import Path
from urllib.parse import urlparse


async def read_info(reader):
    line = await asyncio.wait_for(reader.readline(), timeout=5)
    if not line.startswith(b"INFO "):
        raise RuntimeError(f"expected INFO, got {line!r}")


async def connect(nats_url):
    parsed = urlparse(nats_url)
    reader, writer = await asyncio.open_connection(parsed.hostname, parsed.port)
    await read_info(reader)
    writer.write(
        b'CONNECT {"lang":"search-smoke","version":"0.1.0","protocol":1,"headers":true}\r\n'
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
    writer.write(
        f"HPUB {subject} {reply} {len(headers)} {total}\r\n".encode()
        + headers
        + body
        + b"\r\n"
    )
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


async def request(reader, writer, inbox, subject, payload, trace_id):
    await hpub_json(writer, subject, inbox, payload, trace_id)
    reply_subject, reply = await read_message(reader, writer)
    if reply_subject != inbox:
        raise RuntimeError(f"unexpected reply subject: {reply_subject}")
    return reply


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


async def main():
    reader, writer = await connect(sys.argv[1])
    inbox = f"_INBOX.search_smoke.{uuid.uuid4().hex}"
    await subscribe(reader, writer, inbox)
    results = {}

    trace = "01HXDK00000000000000000021"
    brave = await request(
        reader,
        writer,
        inbox,
        "tool.search.web-search",
        {"query": "bevy ecs", "intent": "fast factual lookup"},
        trace,
    )
    require("error" not in brave, f"Brave search failed: {brave}")
    require(brave["routing"]["backend"] == "brave", f"unexpected Brave route: {brave}")
    require(brave["trace_id"] == trace, f"trace id was not echoed: {brave}")
    require(brave["results"][0]["title"] == "Brave Fake", f"unexpected Brave result: {brave}")
    results["brave"] = brave

    failed = await request(
        reader,
        writer,
        inbox,
        "tool.search.web-search",
        {"query": "fail brave", "intent": "fast factual lookup"},
        "01HXDK00000000000000000022",
    )
    require(
        failed.get("error", {}).get("kind") == "backend-request-failed",
        f"expected backend-request-failed: {failed}",
    )
    results["failed"] = failed

    cooldown = await request(
        reader,
        writer,
        inbox,
        "tool.search.web-search",
        {"query": "bevy ecs", "intent": "fast factual lookup"},
        "01HXDK00000000000000000023",
    )
    require(
        cooldown.get("error", {}).get("kind") == "backend-in-cooldown",
        f"expected backend-in-cooldown: {cooldown}",
    )
    results["cooldown"] = cooldown

    searxng = await request(
        reader,
        writer,
        inbox,
        "tool.search.web-search",
        {"query": "private docs", "intent": "privacy-sensitive lookup"},
        "01HXDK00000000000000000024",
    )
    require("error" not in searxng, f"SearXNG search failed: {searxng}")
    require(searxng["routing"]["backend"] == "searxng", f"unexpected SearXNG route: {searxng}")
    require(searxng["results"][0]["title"] == "SearXNG Fake", f"unexpected SearXNG result: {searxng}")
    results["searxng"] = searxng

    linkup = await request(
        reader,
        writer,
        inbox,
        "tool.search.web-search",
        {"query": "source-backed docs", "intent": "citation/source-backed lookup"},
        "01HXDK00000000000000000025",
    )
    require("error" not in linkup, f"Linkup search failed: {linkup}")
    require(linkup["routing"]["backend"] == "linkup", f"unexpected Linkup route: {linkup}")
    require(linkup["results"][0]["title"] == "Linkup Fake", f"unexpected Linkup result: {linkup}")
    results["linkup"] = linkup

    extract = await request(
        reader,
        writer,
        inbox,
        "tool.search.web-extract",
        {
            "urls": ["https://example.org/fire"],
            "render_js": True,
            "include_images": True,
        },
        "01HXDK00000000000000000026",
    )
    require("error" not in extract, f"Firecrawl extract failed: {extract}")
    require(extract["routing"]["backend"] == "firecrawl", f"unexpected extract route: {extract}")
    require(extract["contents"][0]["title"] == "Firecrawl Fake", f"unexpected extract title: {extract}")
    require("Body from fake Firecrawl scrape." in extract["contents"][0]["text"], f"unexpected extract text: {extract}")
    require(extract["contents"][0]["images"] == ["https://example.org/image.png"], f"unexpected extract images: {extract}")
    results["extract"] = extract

    Path(sys.argv[2]).write_text(json.dumps(results, indent=2, sort_keys=True) + "\n")
    print(json.dumps(results, indent=2, sort_keys=True))


asyncio.run(main())
PY
}

need cargo
need curl
need docker
need python3
need rg

cd "$ROOT"
cargo build -p jam-cli -p jam-nats-bridge -p jam-svc-search

SMOKE_DIR="$(mktemp -d /tmp/jam-search-smoke.XXXXXX)"
start_fake_http
wait_for_port "$HTTP_PORT"

docker run --rm -d --name "$NATS_CONTAINER" -p "127.0.0.1:${NATS_PORT}:4222" nats:2.11.0-alpine -js >/dev/null
wait_for_port "$NATS_PORT"

JAM_HOME="$SMOKE_DIR/jam-home" NATS_URL="$NATS_URL" "$ROOT/target/debug/jam-nats-bridge" >"$SMOKE_DIR/bridge.log" 2>&1 &
BRIDGE_PID="$!"

env \
    NATS_URL="$NATS_URL" \
    JAM_HOME="$SMOKE_DIR/jam-home" \
    JAM_BRAVE_API_KEY="brave-smoke" \
    JAM_BRAVE_SEARCH_ENDPOINT="http://127.0.0.1:${HTTP_PORT}/brave/search" \
    JAM_SEARXNG_ENDPOINT="http://127.0.0.1:${HTTP_PORT}/searx/search" \
    JAM_LINKUP_API_KEY="linkup-smoke" \
    JAM_LINKUP_ENDPOINT="http://127.0.0.1:${HTTP_PORT}/linkup/search" \
    JAM_FIRECRAWL_API_KEY="firecrawl-smoke" \
    JAM_FIRECRAWL_ENDPOINT="http://127.0.0.1:${HTTP_PORT}/firecrawl" \
    "$ROOT/target/debug/jam-svc-search" >"$SMOKE_DIR/search.log" 2>&1 &
SERVICE_PID="$!"

wait_health_ok
wait_bridge_ready
request_search

for _ in {1..80}; do
    if rg -q '"event_type":"search.web-search"|"event_type": "search.web-search"' "$SMOKE_DIR/jam-home/journal" 2>/dev/null && \
       rg -q '"backend":"brave"|"backend": "brave"' "$SMOKE_DIR/jam-home/journal" 2>/dev/null && \
       rg -q '"backend":"searxng"|"backend": "searxng"' "$SMOKE_DIR/jam-home/journal" 2>/dev/null && \
       rg -q '"backend":"linkup"|"backend": "linkup"' "$SMOKE_DIR/jam-home/journal" 2>/dev/null; then
        printf 'search service smoke passed\n'
        exit 0
    fi
    sleep 0.1
done

printf 'search journal entries did not land\n' >&2
find "$SMOKE_DIR/jam-home" -maxdepth 5 -type f -print >&2
find "$SMOKE_DIR/jam-home/journal" -maxdepth 3 -type f -print -exec sed -n '1,20p' {} \; >&2 || true
exit 1
