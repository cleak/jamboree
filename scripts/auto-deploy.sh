#!/usr/bin/env bash
#
# auto-deploy.sh — caleb-side driver that auto-deploys merged services.
#
# Invoked by /etc/cron.d/jam-auto-deploy every minute. Fetches origin/main,
# diffs against the last successfully-deployed sha (recorded in
# $STATE_FILE), maps the changed paths to deploy targets via `jam deploy
# --since`, and runs the deploys serially. On success the new sha lands in
# $STATE_FILE; on failure the file is unchanged so the next tick retries.
#
# Why a cron driver and not a NATS subscriber:
#   - `jam deploy` runs `cargo build --release` and needs caleb's home
#     (target/, ~/.cargo/), but all NATS subscribers run under maestro.
#   - A polling driver is self-healing: even if a `pr.merged` event were
#     missed, the next minute's tick catches it.
#   - Idempotent — re-running on the same diff yields no work.
#
# Safety properties:
#   - flock guards against concurrent runs (cron overlap + manual invocation).
#   - On *any* `jam deploy` failure, the state sha is not updated, so the
#     next tick retries from the same baseline. Repeated retries on the
#     same broken commit are bounded by the natural cycle time of CI / git
#     activity, not by an exponential backoff.
#   - No `set -e`: per-deploy failures get logged and the loop continues
#     to give the next service a shot (rather than orphaning one bad
#     service blocking the others).
#   - Logs rotated when over 1 MB.
#
# Manual invocation (e.g. for testing): just run it as caleb. With
# JAM_AUTO_DEPLOY_DRY_RUN=1, prints what would be deployed without
# actually shelling out to `jam deploy`.

set -uo pipefail

REPO_DIR="${JAM_AUTO_DEPLOY_REPO:-/home/caleb/jamboree}"
STATE_DIR="${JAM_AUTO_DEPLOY_STATE_DIR:-$HOME/.jam}"
STATE_FILE="$STATE_DIR/last-auto-deploy.sha"
LOG_DIR="${JAM_AUTO_DEPLOY_LOG_DIR:-$HOME/.cache}"
LOG_FILE="$LOG_DIR/jam-auto-deploy.log"
LOCK_FILE="$LOG_DIR/jam-auto-deploy.lock"

mkdir -p "$STATE_DIR" "$LOG_DIR"

log() {
    echo "[$(date -u -Iseconds)] $*" >> "$LOG_FILE"
}

# Lock — silently skip if another run is in flight.
exec 9> "$LOCK_FILE"
if ! flock -n 9; then
    log "another run already in flight, skipping"
    exit 0
fi

# Rotate log if over 1 MB.
if [[ -f "$LOG_FILE" ]] && (( $(stat -c%s "$LOG_FILE" 2>/dev/null || echo 0) > 1048576 )); then
    tail -n 10000 "$LOG_FILE" > "$LOG_FILE.tmp" && mv "$LOG_FILE.tmp" "$LOG_FILE"
fi

cd "$REPO_DIR" || { log "fatal: REPO_DIR $REPO_DIR not accessible"; exit 1; }

# Don't run if we're not on main — auto-deploy only ships what's in main,
# and a caleb-side feature-branch checkout would either fail to fetch the
# right ref or risk deploying half-finished work.
current_branch=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo "")
if [[ "$current_branch" != "main" ]]; then
    log "skip: caleb's checkout is on '$current_branch', not 'main'"
    exit 0
fi

# Fetch the latest main. --quiet suppresses normal progress output;
# errors still land on stderr (captured by cron to LOG_FILE) and the
# subsequent rev-parse will fail loudly if the fetch was incomplete.
if ! git fetch --quiet origin main 2>>"$LOG_FILE"; then
    log "fatal: git fetch origin main failed"
    exit 1
fi

# Sync the local main pointer to origin/main. Use --ff-only so an
# accidentally-diverged local main (which would mean someone hand-edited
# the worktree) fails loudly instead of silently merging.
if ! git merge --ff-only --quiet origin/main 2>>"$LOG_FILE"; then
    log "fatal: local main has diverged from origin/main — refusing auto-deploy"
    exit 1
fi

head_sha=$(git rev-parse HEAD)

# First run: just baseline at the current sha. Avoid a full rebuild of
# every service on initial install.
if [[ ! -f "$STATE_FILE" ]]; then
    echo "$head_sha" > "$STATE_FILE"
    log "baseline set to $head_sha (no deploy on first run)"
    exit 0
fi

prev_sha=$(< "$STATE_FILE")
if [[ "$prev_sha" == "$head_sha" ]]; then
    # Common case: nothing new on main since last tick.
    exit 0
fi

# Sanity-check: ensure prev_sha is still reachable. After a force-push to
# main (rare but possible), the recorded sha may be unreachable; the diff
# would then return every file and try to deploy everything. Refuse and
# require manual reset.
if ! git rev-parse --verify --quiet "$prev_sha^{commit}" >/dev/null; then
    log "fatal: previous deploy sha $prev_sha is no longer reachable; manually clear $STATE_FILE to baseline"
    exit 1
fi

log "main moved $prev_sha → $head_sha; computing affected services"

# Drive the deploy via `jam deploy --since <prev_sha>`. The CLI does the
# path→service mapping (shared with `--dirty`) so this script doesn't
# duplicate the logic. We pass `--since` and let the CLI decide whether
# anything actually needs deploying.
if [[ "${JAM_AUTO_DEPLOY_DRY_RUN:-0}" == "1" ]]; then
    log "dry-run: jam deploy --since $prev_sha"
    if ! jam deploy --since "$prev_sha" --help >/dev/null 2>&1; then
        log "warn: jam binary missing --since support (dry-run)"
    fi
    echo "$head_sha" > "$STATE_FILE"
    exit 0
fi

deploy_output=$(jam deploy --since "$prev_sha" 2>&1)
deploy_status=$?
echo "$deploy_output" | sed "s/^/  /" >> "$LOG_FILE"

if [[ $deploy_status -ne 0 ]]; then
    log "jam deploy --since $prev_sha exited $deploy_status; state sha unchanged"
    exit $deploy_status
fi

# All deploys succeeded; advance the watermark.
echo "$head_sha" > "$STATE_FILE"
log "deploy complete; advanced state to $head_sha"
