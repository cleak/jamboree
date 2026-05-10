#!/usr/bin/env bash
#
# Validate the vendored Hermes self-evolution subsystem without spending LLM
# tokens. This installs the vendored package in an isolated uv environment,
# runs upstream tests, and verifies Jamboree invokes the pipeline as a
# subprocess against a temporary skill.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VENDOR="$ROOT/evolution/hermes-agent-self-evolution"
SMOKE_DIR=""

cleanup() {
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

need uv
need python3

if [[ ! -f "$VENDOR/pyproject.toml" ]]; then
    printf 'vendored Hermes self-evolution pyproject is missing: %s\n' "$VENDOR/pyproject.toml" >&2
    exit 1
fi

SMOKE_DIR="$(mktemp -d /tmp/jam-hermes-evolution.XXXXXX)"
SKILL="$SMOKE_DIR/jamboree-skill.md"

cat >"$SKILL" <<'MD'
---
scope: task-types/example
always-loaded: false
---

# Example Skill

Use this skill for the vendored Hermes evolution smoke.

## Procedure

1. Read the task.
2. Return a concise answer.
MD

uv run --no-project --with-editable "$VENDOR" --with pytest \
    python -m pytest "$VENDOR/tests" -q

uv run --no-project --with-editable "$VENDOR" \
    python "$ROOT/evolution/jamboree_evolve_skill.py" \
    --skill-path "$SKILL" \
    --candidate-dir "$SMOKE_DIR/candidates" \
    --work-dir "$SMOKE_DIR/work" \
    --iterations 1 \
    --dry-run

printf 'Hermes evolution vendor smoke passed\n'
