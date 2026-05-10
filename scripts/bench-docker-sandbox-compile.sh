#!/usr/bin/env bash
#
# Measure Docker sandbox overhead for a compile-heavy Blueberry check.
# This is intentionally separate from the fast Docker sandbox smoke because
# the cold-target benchmark takes several minutes.

set -euo pipefail

BLUEBERRY_REPO="${BLUEBERRY_REPO:-/home/caleb/blueberry}"
IMAGE="${JAM_DOCKER_BENCH_IMAGE:-blueberry-ops-base:latest}"
THRESHOLD_PERCENT="${JAM_DOCKER_BENCH_THRESHOLD_PERCENT:-25}"
BENCH_DIR=""

cleanup() {
    if [[ -n "$BENCH_DIR" ]]; then
        rm -rf "$BENCH_DIR"
    fi
}
trap cleanup EXIT

need() {
    command -v "$1" >/dev/null 2>&1 || {
        printf 'missing required command: %s\n' "$1" >&2
        exit 1
    }
}

need cargo
need docker
need python3

if [[ ! -d "$BLUEBERRY_REPO/.git" ]]; then
    printf 'Blueberry repo not found at %s\n' "$BLUEBERRY_REPO" >&2
    printf 'Fix: set BLUEBERRY_REPO=/path/to/blueberry\n' >&2
    exit 1
fi

if ! docker image inspect "$IMAGE" >/dev/null 2>&1; then
    printf 'Docker benchmark image not found: %s\n' "$IMAGE" >&2
    printf 'Fix: build the Picker-equivalent image or set JAM_DOCKER_BENCH_IMAGE\n' >&2
    exit 1
fi

BENCH_DIR="$(mktemp -d /tmp/jam-docker-compile-bench.XXXXXX)"

python3 - "$BLUEBERRY_REPO" "$IMAGE" "$BENCH_DIR" "$THRESHOLD_PERCENT" <<'PY'
import os
import pathlib
import subprocess
import sys
import time

repo = pathlib.Path(sys.argv[1]).resolve()
image = sys.argv[2]
bench_dir = pathlib.Path(sys.argv[3]).resolve()
threshold = float(sys.argv[4])

local_target = bench_dir / "local-target"
docker_target = bench_dir / "docker-target"
local_target.mkdir(parents=True, exist_ok=True)
docker_target.mkdir(parents=True, exist_ok=True)

check_cmd = ["cargo", "check", "--bin", "blueberry"]


def run(label: str, cmd: list[str], *, cwd: pathlib.Path | None = None, env: dict[str, str] | None = None) -> float:
    start = time.perf_counter()
    proc = subprocess.run(
        cmd,
        cwd=cwd,
        env=env,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.PIPE,
        text=True,
    )
    elapsed = time.perf_counter() - start
    print(f"{label}: exit={proc.returncode} elapsed={elapsed:.3f}s", flush=True)
    if proc.returncode != 0:
        print(proc.stderr[-8000:], file=sys.stderr)
        raise SystemExit(proc.returncode)
    return elapsed


local_env = os.environ.copy()
local_env["CARGO_TARGET_DIR"] = str(local_target)

local_elapsed = run("local blueberry cold-target", check_cmd, cwd=repo, env=local_env)
docker_elapsed = run(
    "docker blueberry cold-target",
    [
        "docker",
        "run",
        "--rm",
        "--user",
        f"{os.getuid()}:{os.getgid()}",
        "--mount",
        f"type=bind,src={repo},target=/work,readonly",
        "--mount",
        f"type=bind,src={docker_target},target=/target",
        "-w",
        "/work",
        "-e",
        "CARGO_TARGET_DIR=/target",
        image,
        *check_cmd,
    ],
)

regression = (docker_elapsed / local_elapsed - 1.0) * 100.0
print(
    f"cold local={local_elapsed:.3f}s docker={docker_elapsed:.3f}s regression={regression:.1f}% threshold={threshold:.1f}%",
    flush=True,
)
if regression > threshold:
    raise SystemExit(
        f"Docker compile-heavy regression {regression:.1f}% exceeds threshold {threshold:.1f}%"
    )
PY

