#!/usr/bin/env bash
#
# seed-maestro-secrets.sh — Interactive seeder for maestro's pass store.
#
# Walks the canonical Jamboree secret list (spec §11.3.1, minus the
# OAuth-handled credentials that live in ~maestro/.codex/ and ~maestro/.claude/),
# prompting for each value. Reads with echo disabled and pipes the value to
# `pass insert` via stdin — never on the command line, in the environment,
# or in shell history.
#
# Idempotent: existing entries are detected and you decide per-entry whether
# to keep them, overwrite, or skip the rest. Re-runnable as credentials
# trickle in.
#
# Usage:
#     sudo ./seed-maestro-secrets.sh           # interactive
#     ./seed-maestro-secrets.sh                # also works (NOPASSWD sudo to maestro)
#     sudo ./seed-maestro-secrets.sh --list    # show what would be prompted, no input
#
# Prerequisites:
#   - bootstrap-users.sh has run (maestro user + sudoers in place)
#   - init-maestro-keyring.sh has run (maestro has a GPG key + initialised pass)

set -euo pipefail

MAESTRO_USER="maestro"
LIST_ONLY=0

# ---------------------------------------------------------------------------
# Output helpers
# ---------------------------------------------------------------------------

PASS_GLYPH='\033[32m✓\033[0m'
SKIP_GLYPH='\033[33m-\033[0m'
INFO_GLYPH='\033[34mi\033[0m'
FAIL_GLYPH='\033[31m✗\033[0m'

p_pass() { printf "  ${PASS_GLYPH} %s\n" "$*"; }
p_skip() { printf "  ${SKIP_GLYPH} %s\n" "$*"; }
p_info() { printf "  ${INFO_GLYPH} %s\n" "$*"; }
p_fail() { printf "  ${FAIL_GLYPH} %s\n" "$*" >&2; }
header() { printf "\n\033[1m%s\033[0m\n" "$*"; }

die() {
    p_fail "$1"
    [[ -n "${2:-}" ]] && printf "\n    Fix:\n%s\n\n" "$2" >&2
    exit 1
}

# ---------------------------------------------------------------------------
# Canonical secret list (spec §11.3.1, OAuth-handled keys omitted)
#
# Format:  KEY|MODE|TIER|DESCRIPTION
#   MODE  = single | multi   (multi = until line containing "END")
#   TIER  = required | recommended | optional
# ---------------------------------------------------------------------------

SECRETS=(
    "jam/pickers/github-app-id|single|recommended|Numeric GitHub App ID for the Jamboree-installed App. Required for any Picker that creates PRs."
    "jam/pickers/github-app-key|multi|recommended|PEM-format private key for the GitHub App (the entire -----BEGIN/-----END block)."
    "jam/notify/ntfy-token|single|recommended|ntfy.sh token used to page the Manager when something fails or needs sign-off."
    "jam/search/brave|single|recommended|Brave Search API key — default search-router backend per spec §4.8. Free tier (2k/mo) at https://brave.com/search/api/. Without this, web-search tool calls have no backend until you enable another provider below."
    "jam/nats/token|single|optional|NATS auth token. Skip unless you have enabled NATS authentication."
    "jam/search/firecrawl|single|optional|Firecrawl URL-fetch + scrape API key. Add when Pickers need clean page extraction."
    "jam/search/exa|single|optional|Exa neural-search API key. Add when code-pattern semantic discovery becomes a frequent query intent."
    "jam/search/linkup|single|optional|Linkup search API key. Defer unless you specifically need its factuality/freshness profile."
    "jam/search/perplexity|single|optional|Perplexity Sonar API key. Returns synthesized answers; usually wrong shape for an agent."
    "jam/search/tavily|single|optional|Tavily search API key. Smaller free tier than Brave; benchmarks slightly behind."
    "jam/pickers/deepseek-api-key|single|optional|DeepSeek API key. Only needed if a DeepSeek-backed harness is configured."
    "jam/mcp/composio|single|optional|Composio MCP gateway API key."
    "jam/tailscale/auth-key|single|optional|Tailscale tailnet auth key. Only needed if remote access is configured."
)

# ---------------------------------------------------------------------------
# Argument parsing
# ---------------------------------------------------------------------------

parse_args() {
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --list) LIST_ONLY=1; shift ;;
            --help|-h)
                sed -n '2,/^$/p' "$0" | sed 's/^# //; s/^#//'
                exit 0
                ;;
            *) die "Unknown argument: $1" "    See $0 --help" ;;
        esac
    done
}

# ---------------------------------------------------------------------------
# Preflight
# ---------------------------------------------------------------------------

check_prereqs() {
    if ! id "$MAESTRO_USER" >/dev/null 2>&1; then
        die "User '$MAESTRO_USER' does not exist." \
"    Run bootstrap-users.sh first."
    fi
    if ! sudo -u "$MAESTRO_USER" -i bash -c 'test -f "$HOME/.password-store/.gpg-id"' 2>/dev/null; then
        die "$MAESTRO_USER's pass store is not initialised." \
"    Run init-maestro-keyring.sh first:
        sudo -u $MAESTRO_USER -i $(cd "$(dirname "$0")" && pwd)/init-maestro-keyring.sh"
    fi
    p_pass "Maestro user + pass store ready"
}

# ---------------------------------------------------------------------------
# pass-store helpers (run as maestro)
# ---------------------------------------------------------------------------

# Does $key already exist in maestro's pass store?
secret_exists() {
    local key="$1"
    sudo -u "$MAESTRO_USER" -i bash -c 'pass show "$1" >/dev/null 2>&1' _ "$key"
}

# Store $value (from stdin) at $key as a single-line entry.
# We pipe via stdin so the secret never lands on a command line / in env / in history.
seed_single() {
    local key="$1" value="$2"
    printf '%s' "$value" \
      | sudo -u "$MAESTRO_USER" -H bash -c 'pass insert -e -f -- "$1" >/dev/null' _ "$key"
}

# Store $value (from stdin, may contain newlines) at $key as multi-line.
seed_multi() {
    local key="$1" value="$2"
    printf '%s' "$value" \
      | sudo -u "$MAESTRO_USER" -H bash -c 'pass insert -m -f -- "$1" >/dev/null' _ "$key"
}

# ---------------------------------------------------------------------------
# Input helpers (read from controlling terminal, not stdin)
# ---------------------------------------------------------------------------

# Single-line, echoless. Returns value via stdout.
read_single() {
    local prompt="$1"
    local value=""
    read -r -s -p "$prompt" value </dev/tty
    printf "\n" >&2  # newline after silent input
    printf '%s' "$value"
}

# Multi-line, echo on. Reads until a single line containing "END" or EOF.
read_multi() {
    local lines="" line
    printf "  Paste the full secret. Type a line containing only END to finish.\n" >&2
    while IFS= read -r line </dev/tty; do
        [[ "$line" == "END" ]] && break
        lines+="$line"$'\n'
    done
    printf '%s' "$lines"
}

# Single keystroke, with default. Echoed.
read_choice() {
    local prompt="$1" default="$2" choice
    read -r -p "$prompt" choice </dev/tty || choice=""
    printf '%s' "${choice:-$default}"
}

# ---------------------------------------------------------------------------
# Per-secret prompt
# ---------------------------------------------------------------------------

prompt_one() {
    local key="$1" mode="$2" tier="$3" desc="$4"
    local already; already=0
    secret_exists "$key" && already=1

    printf "\n  \033[1m%s\033[0m  [%s, %s]\n" "$key" "$mode" "$tier"
    printf "  %s\n" "$desc"

    if [[ $LIST_ONLY -eq 1 ]]; then
        if [[ $already -eq 1 ]]; then p_pass "exists in store"; else p_skip "not set"; fi
        return 0
    fi

    local action
    if [[ $already -eq 1 ]]; then
        action="$(read_choice "  Already set. [k]eep / [o]verwrite / [q]uit walk? [k] " "k")"
    else
        action="$(read_choice "  [s]et value / s[k]ip / [q]uit walk? [k] " "k")"
    fi

    case "${action,,}" in
        q|quit)
            return 99   # signal: stop the walk
            ;;
        o|overwrite|s|set)
            ;;          # fall through to read input
        *)
            if [[ $already -eq 1 ]]; then p_skip "kept existing"; else p_skip "skipped"; fi
            return 0
            ;;
    esac

    local value
    if [[ "$mode" == "multi" ]]; then
        value="$(read_multi)"
    else
        value="$(read_single "  Value: ")"
    fi

    if [[ -z "$value" ]]; then
        p_skip "empty input — not stored"
        return 0
    fi

    if [[ "$mode" == "multi" ]]; then
        if seed_multi "$key" "$value"; then p_pass "stored (multi)"; else p_fail "store failed"; fi
    else
        if seed_single "$key" "$value"; then p_pass "stored"; else p_fail "store failed"; fi
    fi
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

main() {
    parse_args "$@"

    header "Maestro pass-store seed"
    [[ $LIST_ONLY -eq 1 ]] && p_info "LIST ONLY — no values will be prompted or stored"

    header "Preflight"
    check_prereqs

    header "Walking secrets (spec §11.3.1)"
    p_info "Per entry: [s]et / [k]eep / [o]verwrite / s[k]ip / [q]uit"
    p_info "Re-run anytime to add or update — already-set entries are detected."

    local rc=0
    local quit=0
    for entry in "${SECRETS[@]}"; do
        IFS='|' read -r key mode tier desc <<<"$entry"
        if [[ $quit -eq 1 ]]; then
            p_skip "$key — skipped (quit)"
            continue
        fi
        prompt_one "$key" "$mode" "$tier" "$desc" || rc=$?
        if [[ $rc -eq 99 ]]; then quit=1; rc=0; fi
    done

    header "Final pass-store contents"
    sudo -u "$MAESTRO_USER" -i pass list 2>/dev/null | sed 's/^/    /'

    header "Done."
    cat <<EOF

Re-run anytime to add or update entries:
    sudo $0

Note: OAuth-stored credentials (codex + claude) are NOT in pass — they
live in ~maestro/.codex/auth.json and ~maestro/.claude/, populated by:
    sudo -u $MAESTRO_USER -i codex login --device-auth
    sudo -u $MAESTRO_USER -i claude

EOF
}

main "$@"
