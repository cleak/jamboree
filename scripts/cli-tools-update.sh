#!/usr/bin/env bash
#
# cli-tools-update.sh — Daily per-user updater for codex + claude-code + opencode.
#
# Invoked by /etc/cron.d/jam-cli-update once a day for each Jamboree user
# (caleb, maestro, picker). Updates whichever tools that user has installed.
#
# Safety properties:
#   - Per-user execution; no root. A bad update can only damage one user's
#     installation, never the system or other users.
#   - flock prevents concurrent runs (manual + cron, or two cron firings).
#   - Failures are logged but do not propagate; cron will not email errors.
#   - Log file rotated (last ~10000 lines kept).
#   - PATH is set explicitly because cron's default PATH does not include
#     ~/.npm-global/bin or ~/.local/bin.
#
# Manual invocation (e.g. for testing): just run it as the target user.
#   sudo -u maestro -i /usr/local/bin/jam-cli-update

# Note: no `set -e` — we want to log failures from each tool and continue,
# rather than aborting the whole update if codex's network call fails.
set -uo pipefail

LOG_DIR="$HOME/.cache"
LOG_FILE="$LOG_DIR/jam-cli-update.log"
LOCK_FILE="$LOG_DIR/jam-cli-update.lock"

mkdir -p "$LOG_DIR"

# Acquire exclusive lock; bail silently if another run is in flight.
exec 9> "$LOCK_FILE"
if ! flock -n 9; then
    echo "[$(date -u -Iseconds)] $(whoami): another update already running, skipping" \
        >> "$LOG_FILE"
    exit 0
fi

# Trim log if larger than 1 MB.
if [[ -f "$LOG_FILE" ]] && (( $(stat -c%s "$LOG_FILE" 2>/dev/null || echo 0) > 1048576 )); then
    tail -n 10000 "$LOG_FILE" > "$LOG_FILE.tmp" && mv "$LOG_FILE.tmp" "$LOG_FILE"
fi

# cron's default PATH is bare; surface the per-user npm prefix and the
# Claude Code native install location.
export PATH="$HOME/.npm-global/bin:$HOME/.local/bin:$PATH"

ts() { date -u -Iseconds; }
log() { echo "[$(ts)] $(whoami): $*" >> "$LOG_FILE"; }

log "===== run start ====="

# --- codex --------------------------------------------------------------
if command -v codex >/dev/null 2>&1; then
    log "updating @openai/codex via npm"
    if npm install -g @openai/codex >> "$LOG_FILE" 2>&1; then
        log "codex ok: $(codex --version 2>&1 | head -1)"
    else
        log "codex FAILED — see above"
    fi
else
    log "codex not installed for this user — skipping"
fi

# --- claude-code --------------------------------------------------------
if command -v claude >/dev/null 2>&1; then
    log "updating claude-code via 'claude update'"
    if claude update >> "$LOG_FILE" 2>&1; then
        log "claude-code ok: $(claude --version 2>&1 | head -1)"
    else
        log "claude-code FAILED — see above"
    fi
else
    log "claude-code not installed for this user — skipping"
fi

# --- opencode -----------------------------------------------------------
if command -v opencode >/dev/null 2>&1; then
    log "updating opencode-ai via npm"
    if npm install -g opencode-ai >> "$LOG_FILE" 2>&1; then
        log "opencode ok: $(opencode --version 2>&1 | head -1)"
    else
        log "opencode FAILED — see above"
    fi
else
    log "opencode not installed for this user — skipping"
fi

log "===== run end ====="
echo "" >> "$LOG_FILE"

exit 0
