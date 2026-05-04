#!/usr/bin/env bash
#
# install-cli-tools.sh — Per-user installation of CLI harness tools for Jamboree.
#
# Tools per user:
#   <human>  — codex + claude-code  (dev/test on caleb's account)
#   maestro  — codex + claude-code  (conductor harness; either may drive it)
#   picker   — codex + claude-code  (Picker harnesses execute as picker)
#
# Per-user installs (not root) because both @openai/codex and the official
# Claude Code installer auto-update by writing to their install location;
# root-owned installs break that mechanism. Anthropic explicitly refuses
# to run claude when its prefix is root-owned (claude-code issue #43).
#
# After install, daily auto-updates run via /etc/cron.d/jam-cli-update at
# staggered times (4:15, 4:30, 4:45 AM local). See cli-tools-update.sh.
#
# Usage:
#     sudo ./install-cli-tools.sh                  # interactive
#     sudo ./install-cli-tools.sh --user caleb     # explicit human user
#     sudo ./install-cli-tools.sh --verify-only    # check, don't change
#     sudo ./install-cli-tools.sh --dry-run        # show what would happen
#
# Idempotent: safe to re-run.
# Prerequisite: bootstrap-users.sh must have completed.

set -euo pipefail

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

MAESTRO_USER="maestro"
PICKER_USER="picker"

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
UPDATE_SCRIPT_SRC="$SCRIPT_DIR/cli-tools-update.sh"
UPDATE_SCRIPT_DEST="/usr/local/bin/jam-cli-update"
CRON_FILE="/etc/cron.d/jam-cli-update"
SETUP_LOG="/etc/jam/cli-tools.log"

# Parsed args
HUMAN_USER=""
DRY_RUN=0
VERIFY_ONLY=0

# ---------------------------------------------------------------------------
# Output helpers — match bootstrap-users.sh / `jam doctor` style
# ---------------------------------------------------------------------------

PASS_GLYPH='\033[32m✓\033[0m'
FAIL_GLYPH='\033[31m✗\033[0m'
INFO_GLYPH='\033[34mi\033[0m'
WARN_GLYPH='\033[33m!\033[0m'

pass()   { printf "  ${PASS_GLYPH} %s\n" "$*"; }
fail()   { printf "  ${FAIL_GLYPH} %s\n" "$*" >&2; }
info()   { printf "  ${INFO_GLYPH} %s\n" "$*"; }
warn()   { printf "  ${WARN_GLYPH} %s\n" "$*" >&2; }
header() { printf "\n\033[1m%s\033[0m\n" "$*"; }

die() {
    fail "$1"
    if [[ -n "${2:-}" ]]; then
        printf "\n    Fix:\n%s\n\n" "$2" >&2
    fi
    exit 1
}

run_cmd() {
    if [[ $DRY_RUN -eq 1 ]]; then
        printf "    [dry-run] %s\n" "$*"
        return 0
    fi
    "$@"
}

# Run the bash script piped on stdin as a target user, in their login shell
# (so $HOME, .profile, npm config etc. all resolve correctly). Safer than
# embedding shell snippets in -c arguments because of quoting.
run_as() {
    local user="$1"
    if [[ $DRY_RUN -eq 1 ]]; then
        printf "    [dry-run] sudo -u %s -i bash <<-EOF\n" "$user"
        sed 's/^/        /'
        printf "    [dry-run] EOF\n"
        return 0
    fi
    sudo -u "$user" -i bash
}

# ---------------------------------------------------------------------------
# Argument parsing
# ---------------------------------------------------------------------------

parse_args() {
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --user)        HUMAN_USER="$2"; shift 2 ;;
            --dry-run)     DRY_RUN=1; shift ;;
            --verify-only) VERIFY_ONLY=1; shift ;;
            --help|-h)
                sed -n '2,/^$/p' "$0" | sed 's/^# //; s/^#//'
                exit 0
                ;;
            *) die "Unknown argument: $1" "    See $0 --help" ;;
        esac
    done

    if [[ -z "$HUMAN_USER" ]]; then
        if [[ -n "${SUDO_USER:-}" && "$SUDO_USER" != "root" ]]; then
            HUMAN_USER="$SUDO_USER"
            info "Using calling user from sudo: $HUMAN_USER"
        else
            die "Could not determine human user." "    Re-run with --user <username>."
        fi
    fi
}

# ---------------------------------------------------------------------------
# Preflight
# ---------------------------------------------------------------------------

check_root() {
    [[ $EUID -eq 0 ]] || die "Must run as root." "    sudo $0"
}

check_node_npm() {
    if ! command -v npm >/dev/null 2>&1; then
        die "npm not found on system PATH." \
"    Node.js + npm must be installed system-wide so service users can use it.
        sudo apt install nodejs npm
    Or install via NodeSource for newer versions:
        https://github.com/nodesource/distributions"
    fi
    info "Node $(node --version 2>/dev/null), npm $(npm --version 2>/dev/null)"
}

check_cron() {
    if ! command -v crontab >/dev/null 2>&1; then
        warn "cron not installed — daily auto-update won't run."
        warn "    sudo apt install cron"
    elif ! pgrep -x cron >/dev/null 2>&1 && ! pgrep -x crond >/dev/null 2>&1; then
        warn "cron is installed but the daemon is not running."
        warn "    sudo service cron start"
        warn "    For WSL persistence, add to /etc/wsl.conf:"
        warn "        [boot]"
        warn "        command=\"service cron start\""
    else
        pass "cron daemon running"
    fi
}

check_users_exist() {
    for user in "$HUMAN_USER" "$MAESTRO_USER" "$PICKER_USER"; do
        if ! id "$user" >/dev/null 2>&1; then
            die "User '$user' does not exist." "    Run bootstrap-users.sh first."
        fi
    done
    pass "All target users exist"
}

check_update_script_present() {
    if [[ ! -f "$UPDATE_SCRIPT_SRC" ]]; then
        die "Companion script missing: $UPDATE_SCRIPT_SRC" \
"    cli-tools-update.sh must live alongside install-cli-tools.sh"
    fi
}

# ---------------------------------------------------------------------------
# Per-user install steps
# ---------------------------------------------------------------------------

ensure_npm_prefix() {
    local user="$1"
    info "$user: configuring npm prefix → ~/.npm-global"
    run_as "$user" <<'EOF'
mkdir -p "$HOME/.npm-global"
npm config set prefix "$HOME/.npm-global" >/dev/null

# Add PATH export to .profile if not already there. .profile is sourced
# by bash login shells (cron uses these via `bash -i` only sometimes,
# but the daily updater sets PATH explicitly so this is a convenience
# for interactive sessions).
profile="$HOME/.profile"
[ -f "$profile" ] || touch "$profile"
if ! grep -q 'npm-global/bin' "$profile"; then
    {
        echo ''
        echo '# Added by jamboree install-cli-tools.sh'
        echo 'export PATH="$HOME/.npm-global/bin:$HOME/.local/bin:$PATH"'
    } >> "$profile"
fi
EOF
    pass "$user: npm prefix configured"
}

install_codex() {
    local user="$1"
    if run_as "$user" <<<'command -v codex >/dev/null 2>&1' 2>/dev/null; then
        pass "$user: codex already installed"
        return 0
    fi
    info "$user: installing @openai/codex"
    if ! run_as "$user" <<'EOF'
export PATH="$HOME/.npm-global/bin:$PATH"
npm install -g @openai/codex
EOF
    then
        warn "$user: codex install failed — see above"
        return 1
    fi
    pass "$user: codex installed"
}

install_claude_code() {
    local user="$1"
    if run_as "$user" <<<'command -v claude >/dev/null 2>&1' 2>/dev/null; then
        pass "$user: claude-code already installed"
        return 0
    fi
    info "$user: installing claude-code (native installer)"
    if ! run_as "$user" <<'EOF'
curl -fsSL https://claude.ai/install.sh | bash
EOF
    then
        warn "$user: claude-code install failed — see above"
        return 1
    fi
    pass "$user: claude-code installed"
}

# ---------------------------------------------------------------------------
# Auto-update plumbing
# ---------------------------------------------------------------------------

install_update_script() {
    info "Installing update script → $UPDATE_SCRIPT_DEST"
    run_cmd install -m 755 -o root -g root "$UPDATE_SCRIPT_SRC" "$UPDATE_SCRIPT_DEST"
    pass "Update script installed"
}

write_cron_config() {
    info "Writing cron config → $CRON_FILE"

    local cron_content="# /etc/cron.d/jam-cli-update
# Daily auto-update for codex + claude-code, per Jamboree user.
# Times are staggered to spread network/load.
# Each line runs as the named user (third field).
SHELL=/bin/bash
PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin

15 4 * * * $HUMAN_USER    $UPDATE_SCRIPT_DEST
30 4 * * * $MAESTRO_USER  $UPDATE_SCRIPT_DEST
45 4 * * * $PICKER_USER   $UPDATE_SCRIPT_DEST
"
    if [[ $DRY_RUN -eq 1 ]]; then
        printf "    [dry-run] would write %s:\n" "$CRON_FILE"
        printf '%s' "$cron_content" | sed 's/^/        /'
        return 0
    fi
    printf '%s' "$cron_content" > "$CRON_FILE"
    chmod 644 "$CRON_FILE"
    chown root:root "$CRON_FILE"
    pass "Cron config installed"
}

# ---------------------------------------------------------------------------
# Verification
# ---------------------------------------------------------------------------

check_user_has_tool() {
    local user="$1" tool="$2"
    sudo -u "$user" -i bash -c "command -v $tool >/dev/null 2>&1"
}

verify() {
    local failed=0

    for entry in "$HUMAN_USER:codex" "$HUMAN_USER:claude" \
                 "$MAESTRO_USER:codex" "$MAESTRO_USER:claude" \
                 "$PICKER_USER:codex" "$PICKER_USER:claude"; do
        local user="${entry%%:*}"
        local tool="${entry##*:}"
        if check_user_has_tool "$user" "$tool"; then
            pass "$user: $tool present"
        else
            fail "$user: $tool missing"
            failed=1
        fi
    done

    if [[ -x "$UPDATE_SCRIPT_DEST" ]]; then
        pass "Update script: $UPDATE_SCRIPT_DEST"
    else
        fail "Update script missing: $UPDATE_SCRIPT_DEST"
        failed=1
    fi

    if [[ -f "$CRON_FILE" ]]; then
        pass "Cron config: $CRON_FILE"
    else
        fail "Cron config missing: $CRON_FILE"
        failed=1
    fi

    return $failed
}

# ---------------------------------------------------------------------------
# Audit log
# ---------------------------------------------------------------------------

write_setup_log() {
    [[ $DRY_RUN -eq 1 || $VERIFY_ONLY -eq 1 ]] && return
    install -d -m 755 -o root -g root /etc/jam
    cat > "$SETUP_LOG" <<EOF
# install-cli-tools.sh setup record
setup_completed_at = "$(date -u -Iseconds)"
human_user    = "$HUMAN_USER"
maestro_user  = "$MAESTRO_USER"
picker_user   = "$PICKER_USER"
update_script = "$UPDATE_SCRIPT_DEST"
cron_file     = "$CRON_FILE"
script_version = "v5-addendum-1.0"
EOF
    chmod 644 "$SETUP_LOG"
    pass "Setup record → $SETUP_LOG"
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

main() {
    parse_args "$@"

    header "Per-user CLI tool installation"
    [[ $DRY_RUN -eq 1 ]] && info "DRY RUN — no changes will be made"
    [[ $VERIFY_ONLY -eq 1 ]] && info "VERIFY ONLY — checking existing setup"

    header "Preflight checks"
    check_root
    check_node_npm
    check_cron
    check_users_exist
    check_update_script_present

    if [[ $VERIFY_ONLY -eq 1 ]]; then
        header "Verification"
        if verify; then
            header "All checks passed."
            exit 0
        else
            header "Verification failed. Re-run without --verify-only to fix."
            exit 1
        fi
    fi

    header "Installing tools for $HUMAN_USER"
    ensure_npm_prefix "$HUMAN_USER"
    install_codex "$HUMAN_USER"
    install_claude_code "$HUMAN_USER"

    header "Installing tools for $MAESTRO_USER"
    ensure_npm_prefix "$MAESTRO_USER"
    install_codex "$MAESTRO_USER"
    install_claude_code "$MAESTRO_USER"

    header "Installing tools for $PICKER_USER"
    ensure_npm_prefix "$PICKER_USER"
    install_codex "$PICKER_USER"
    install_claude_code "$PICKER_USER"

    header "Setting up daily auto-update"
    install_update_script
    write_cron_config

    write_setup_log

    header "Verification"
    verify || warn "Some verification checks failed — see above."

    header "CLI tool install complete."
    cat <<EOF

Next steps:
  1. Authenticate each user to their subscription. codex must use
     --device-auth so the OAuth flow does not require a local browser
     redirect (Maestro/Picker have no display; --device-auth prints a
     URL + code to enter on another device):
        sudo -u $HUMAN_USER   -i claude                     # first launch triggers OAuth
        sudo -u $HUMAN_USER   -i codex login --device-auth
        sudo -u $MAESTRO_USER -i codex login --device-auth
        sudo -u $MAESTRO_USER -i claude
        sudo -u $PICKER_USER  -i codex login --device-auth
        sudo -u $PICKER_USER  -i claude

  2. Confirm cron is running (WSL doesn't auto-start it):
        sudo service cron status
        sudo service cron start    # if not running
     For persistence across WSL restarts, add to /etc/wsl.conf:
        [boot]
        command="service cron start"

  3. Watch for the first auto-update run (4:15 AM onward):
        tail -f /home/$HUMAN_USER/.cache/jam-cli-update.log

To uninstall the auto-update only:  sudo rm $CRON_FILE $UPDATE_SCRIPT_DEST
To uninstall the tools per user:    rm -rf ~/.npm-global  (and re-run the
                                    Claude Code uninstaller per ~/.local/...)

EOF
}

main "$@"
