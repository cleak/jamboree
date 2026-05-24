#!/usr/bin/env bash
#
# install-auto-deploy.sh — wire up the caleb-side auto-deploy cron driver.
#
# After install: every minute, cron runs auto-deploy.sh as caleb. The
# script fetches origin/main, diffs against the last-deployed sha
# (recorded under ~/.jam/last-auto-deploy.sha), and invokes
# `jam deploy --since` for every service whose source changed.
#
# Idempotent — re-run anytime to update the installed script and cron entry.
#
# Usage: sudo ./scripts/install-auto-deploy.sh
#        sudo ./scripts/install-auto-deploy.sh --dry-run
#        sudo ./scripts/install-auto-deploy.sh --uninstall

set -euo pipefail

DRY_RUN=false
UNINSTALL=false
HUMAN_USER="${SUDO_USER:-caleb}"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --dry-run)
            DRY_RUN=true
            shift
            ;;
        --uninstall)
            UNINSTALL=true
            shift
            ;;
        --user)
            HUMAN_USER="$2"
            shift 2
            ;;
        *)
            echo "unknown arg: $1" >&2
            exit 2
            ;;
    esac
done

SOURCE_SCRIPT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/auto-deploy.sh"
SCRIPT_DEST="/usr/local/bin/jam-auto-deploy"
CRON_FILE="/etc/cron.d/jam-auto-deploy"

pass() { printf "\033[32mPASS\033[0m %s\n" "$*"; }
info() { printf "\033[36mINFO\033[0m %s\n" "$*"; }
warn() { printf "\033[33mWARN\033[0m %s\n" "$*"; }
fail() { printf "\033[31mFAIL\033[0m %s\n" "$*" >&2; exit 1; }

if [[ "$(id -u)" -ne 0 ]]; then
    fail "must run as root (sudo)"
fi

if ! id -u "$HUMAN_USER" >/dev/null 2>&1; then
    fail "user $HUMAN_USER does not exist; pass --user <name> or run as the human user via sudo"
fi

# Handle uninstall before checking the source script — the source checkout
# may have moved or been deleted, but uninstall should still work.
if $UNINSTALL; then
    if $DRY_RUN; then
        info "(dry-run) would remove $SCRIPT_DEST and $CRON_FILE"
        exit 0
    fi
    rm -f "$SCRIPT_DEST" "$CRON_FILE"
    pass "uninstalled jam-auto-deploy"
    exit 0
fi

if [[ ! -f "$SOURCE_SCRIPT" ]]; then
    fail "source script $SOURCE_SCRIPT not found"
fi

# Cron entry: every minute, run as the human user. The script self-locks,
# so overlapping invocations are safe; running every minute keeps lag low
# (a merge lands → within ~60s the deploy starts).
CRON_CONTENT="# /etc/cron.d/jam-auto-deploy
# Auto-deploy services affected by commits merged to origin/main. Runs
# as $HUMAN_USER so the cargo build + jam deploy path has the right
# home (target/, ~/.cargo/, etc). Installed by scripts/install-auto-deploy.sh.
SHELL=/bin/bash
PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin

* * * * * $HUMAN_USER $SCRIPT_DEST
"

if $DRY_RUN; then
    info "(dry-run) would install $SOURCE_SCRIPT → $SCRIPT_DEST"
    info "(dry-run) would write cron entry to $CRON_FILE:"
    printf '%s' "$CRON_CONTENT" | sed 's/^/  /'
    exit 0
fi

install -m 0755 -o root -g root "$SOURCE_SCRIPT" "$SCRIPT_DEST"
pass "installed $SCRIPT_DEST"

printf '%s' "$CRON_CONTENT" > "$CRON_FILE.tmp"
chmod 0644 "$CRON_FILE.tmp"
chown root:root "$CRON_FILE.tmp"
mv "$CRON_FILE.tmp" "$CRON_FILE"
pass "wrote cron entry $CRON_FILE"

info "first tick will run within 60s; logs land in /home/$HUMAN_USER/.cache/jam-auto-deploy.log"
info "tail logs:  sudo tail -F /home/$HUMAN_USER/.cache/jam-auto-deploy.log"
info "uninstall:  sudo $0 --uninstall"
