#!/usr/bin/env bash
#
# Prove the §20.3 atomic-swap acceptance path with a live NATS server:
# a long-lived Maestro-side router starts an in-flight observe call on the old
# prefix, `jam patch apply` swaps observe, the old call completes, and the next
# call from the same router uses the new prefix.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
NATS_PORT="${NATS_PORT:-42229}"
NATS_URL="nats://127.0.0.1:${NATS_PORT}"
CONTAINER="jam-smoke-nats-mid-session-$$"
SMOKE_HOME=""
PY_PID=""

cleanup() {
    if [[ -n "$PY_PID" ]]; then
        wait "$PY_PID" 2>/dev/null || true
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

need docker
need uv

cd "$ROOT"
cargo build -p jam-cli -p jam-svc-observe

docker run --rm -d --name "$CONTAINER" -p "127.0.0.1:${NATS_PORT}:4222" nats:2.11.0-alpine -js >/dev/null

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

SMOKE_HOME="$(mktemp -d /tmp/jam-patch-mid-session.XXXXXX)"
mkdir -p "$SMOKE_HOME/staging"
cp target/debug/jam-svc-observe "$SMOKE_HOME/staging/jam-svc-observe-0.0.9"
cp target/debug/jam-svc-observe "$SMOKE_HOME/staging/jam-svc-observe-0.1.0"
chmod +x "$SMOKE_HOME/staging/jam-svc-observe-0.0.9" "$SMOKE_HOME/staging/jam-svc-observe-0.1.0"

export JAM_HOME="$SMOKE_HOME"
export NATS_URL
export JAM_OBSERVE_GITHUB_LOOKUP=false
export JAM_OBSERVE_WORLD_SNAPSHOT_DELAY_MS=1500
export JAM_PATCH_HEALTH_TIMEOUT_SECS=10
export JAM_PATCH_DRAIN_TIMEOUT_SECS=10

target/debug/jam patch apply observe 0.0.9 --nats-url "$NATS_URL"

READY="$SMOKE_HOME/maestro-ready"
RESULT="$SMOKE_HOME/maestro-proof.json"
PYTHONPATH="$ROOT/maestro/src" \
SMOKE_READY="$READY" \
SMOKE_RESULT="$RESULT" \
uv run --project "$ROOT/maestro" python - <<'PY' &
import asyncio
import json
import os
from pathlib import Path

from jam_maestro.nats_rpc import request_json, subscribe_json
from jam_maestro.routing_manifest import (
    NatsRoutingManifestSource,
    ROUTING_MANIFEST_UPDATED_SUBJECT,
    RoutingManifestRouter,
)

TRACE_ID = "01HXKJ00000000000000000000"


async def main() -> None:
    nats_url = os.environ["NATS_URL"]
    ready = Path(os.environ["SMOKE_READY"])
    result = Path(os.environ["SMOKE_RESULT"])
    router = RoutingManifestRouter(NatsRoutingManifestSource(nats_url))

    first_subject = await router.subject_for("observe", "world-snapshot", TRACE_ID)
    if first_subject != "tool.observe.v009.world-snapshot":
        raise RuntimeError(f"expected first route to use v009, got {first_subject}")

    updates = subscribe_json(
        nats_url=nats_url,
        subject=ROUTING_MANIFEST_UPDATED_SUBJECT,
        timeout_secs=15.0,
    )
    update_iter = updates.__aiter__()
    update_task = asyncio.create_task(update_iter.__anext__())
    await asyncio.sleep(0.3)

    first_task = asyncio.create_task(
        request_json(
            nats_url=nats_url,
            subject=first_subject,
            payload={"task_id": "task-atomic-swap-mid-session"},
            trace_id=TRACE_ID,
            timeout_secs=10.0,
        )
    )
    await asyncio.sleep(0.3)
    ready.write_text("ready\n")

    update = await asyncio.wait_for(update_task, timeout=15.0)
    first = await first_task
    reloaded = await router.reload_on_update(update)
    if not reloaded:
        raise RuntimeError("routing-manifest.updated did not reload the router")

    second_subject = await router.subject_for("observe", "world-snapshot", TRACE_ID)
    if second_subject != "tool.observe.v010.world-snapshot":
        raise RuntimeError(f"expected second route to use v010, got {second_subject}")
    second = await request_json(
        nats_url=nats_url,
        subject=second_subject,
        payload={"task_id": "task-atomic-swap-mid-session"},
        trace_id=TRACE_ID,
        timeout_secs=10.0,
    )
    if "error" in first:
        raise RuntimeError(f"in-flight old-prefix call failed: {first}")
    if "error" in second:
        raise RuntimeError(f"new-prefix call failed: {second}")

    result.write_text(
        json.dumps(
            {
                "first_subject": first_subject,
                "second_subject": second_subject,
                "first_cache": first.get("cache"),
                "second_cache": second.get("cache"),
            },
            indent=2,
            sort_keys=True,
        )
        + "\n"
    )
    await updates.aclose()


asyncio.run(main())
PY
PY_PID="$!"

for _ in {1..100}; do
    [[ -f "$READY" ]] && break
    sleep 0.1
done
[[ -f "$READY" ]] || {
    printf 'Maestro-side smoke did not reach in-flight marker\n' >&2
    exit 1
}

target/debug/jam patch apply observe 0.1.0 --nats-url "$NATS_URL"
wait "$PY_PID"
PY_PID=""

target/debug/jam health ping observe --subject tool.observe.v010.ping --nats-url "$NATS_URL" --timeout-secs 3 >/dev/null
if target/debug/jam health ping observe --subject tool.observe.v009.ping --nats-url "$NATS_URL" --timeout-secs 1 >/dev/null 2>&1; then
    printf 'old observe prefix still answered ok after drain\n' >&2
    exit 1
fi

cat "$RESULT"
printf 'atomic-swap mid-session smoke passed\n'
