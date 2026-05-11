#!/usr/bin/env bash
#
# authorize-github-user-token.sh — interactive device-flow auth for the
# jamboree-cleak GitHub App, producing a user-to-server access token
# authorized by the human user. Stored at `jam/pickers/github-user-token`
# in maestro's pass store.
#
# Why this exists: PRs opened with the App's *installation* token are
# attributed to `app/<app-name>` with `is_bot:true`. CodeRabbit (and any
# other reviewer that filters bot-authored PRs) hard-skips them. A
# user-to-server token authorizes the App on behalf of a real user, so
# PRs open as that user (`is_bot:false`) and CodeRabbit auto-reviews
# through the normal path. See graph/decisions/dec-github-app-not-pat.md.
#
# Prerequisites:
#   - bootstrap-users.sh + init-maestro-keyring.sh have run.
#   - The GitHub App has Device Flow enabled (App settings →
#     "Identifying and authorizing users" → check "Enable Device Flow").
#   - The GitHub App's "User-to-server token expiration" has been
#     opted out (App settings → Optional Features → next to
#     "User-to-server token expiration", click "Opt-out"). Without that,
#     the token expires every 8 hours and this script's output is
#     short-lived.
#   - The App's Client ID is known. Find it at
#     https://github.com/settings/apps/<your-app> → "About" section.
#
# Usage:
#     ./authorize-github-user-token.sh                # interactive
#     ./authorize-github-user-token.sh --client-id Iv1.xxxxxxxxxxxxxxxx
#     ./authorize-github-user-token.sh --help
#
# Idempotent: re-running overwrites the stored token.

set -euo pipefail

MAESTRO_USER="maestro"
PASS_KEY="jam/pickers/github-user-token"
CLIENT_ID=""

PASS_GLYPH='\033[32m✓\033[0m'
INFO_GLYPH='\033[34mi\033[0m'
WARN_GLYPH='\033[33m!\033[0m'
FAIL_GLYPH='\033[31m✗\033[0m'

pass()   { printf "  ${PASS_GLYPH} %s\n" "$*"; }
info()   { printf "  ${INFO_GLYPH} %s\n" "$*"; }
warn()   { printf "  ${WARN_GLYPH} %s\n" "$*" >&2; }
fail()   { printf "  ${FAIL_GLYPH} %s\n" "$*" >&2; }
header() { printf "\n\033[1m%s\033[0m\n" "$*"; }

die() {
    fail "$1"
    [[ -n "${2:-}" ]] && printf "\n    Fix:\n%s\n\n" "$2" >&2
    exit 1
}

parse_args() {
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --client-id) CLIENT_ID="$2"; shift 2 ;;
            --help|-h)
                sed -n '2,/^$/p' "$0" | sed 's/^# //; s/^#//'
                exit 0
                ;;
            *) die "Unknown argument: $1" "    See $0 --help" ;;
        esac
    done
}

check_prereqs() {
    command -v curl >/dev/null 2>&1 \
        || die "curl is required." "    Install curl (sudo apt install curl)."
    command -v jq >/dev/null 2>&1 \
        || die "jq is required." "    Install jq (sudo apt install jq)."
    if ! id "$MAESTRO_USER" >/dev/null 2>&1; then
        die "User '$MAESTRO_USER' does not exist." "    Run bootstrap-users.sh first."
    fi
    if ! sudo -u "$MAESTRO_USER" -i bash -c 'test -f "$HOME/.password-store/.gpg-id"' 2>/dev/null; then
        die "$MAESTRO_USER's pass store is not initialised." \
"    Run init-maestro-keyring.sh first."
    fi
    pass "curl, jq, maestro user, and pass store all ready"
}

prompt_for_client_id() {
    if [[ -n "$CLIENT_ID" ]]; then
        return
    fi
    info "Find the App's Client ID at https://github.com/settings/apps/<your-app>"
    info "It looks like: Iv23.. or Iv1...."
    read -r -p "  GitHub App Client ID: " CLIENT_ID </dev/tty
    if [[ -z "$CLIENT_ID" ]]; then
        die "Client ID is required." \
"    Pass --client-id Iv1.xxxxxxxxxxxx or paste at the prompt."
    fi
}

# Start the device flow. Returns device_code, user_code, verification_uri,
# interval, and expires_in on stdout as JSON.
request_device_code() {
    local response
    response="$(curl -fsS -X POST \
        -H "Accept: application/json" \
        -d "client_id=$CLIENT_ID" \
        https://github.com/login/device/code)"
    if ! printf '%s' "$response" | jq -e '.device_code' >/dev/null 2>&1; then
        die "GitHub did not return a device code." \
"    Response: $response
    Verify the Client ID is correct and that device flow is enabled on the App
    (App settings → 'Device flow' must be checked)."
    fi
    printf '%s' "$response"
}

# Poll for the access token. Reads the device_code response on stdin.
poll_for_token() {
    local device_response="$1"
    local device_code user_code verification_uri interval expires_in
    device_code="$(jq -r '.device_code'      <<<"$device_response")"
    user_code="$(jq -r '.user_code'           <<<"$device_response")"
    verification_uri="$(jq -r '.verification_uri' <<<"$device_response")"
    interval="$(jq -r '.interval'             <<<"$device_response")"
    expires_in="$(jq -r '.expires_in'         <<<"$device_response")"

    # All user-facing chatter goes to stderr — stdout is captured by the
    # caller's $(...) substitution and reserved for the final JSON payload.
    printf "\n\033[1m%s\033[0m\n" "Authorize the App" >&2
    printf "\n" >&2
    printf "    Visit:       %s\n" "$verification_uri" >&2
    printf "    Enter code:  \033[1m%s\033[0m\n" "$user_code" >&2
    printf "    Expires in:  %d seconds\n" "$expires_in" >&2
    printf "\n" >&2
    info "Sign in as the user you want PRs attributed to (e.g. cleak)." >&2
    info "Approve the requested scopes, then return here. Polling..." >&2

    local deadline=$(( $(date +%s) + expires_in ))
    while (( $(date +%s) < deadline )); do
        sleep "$interval"
        local response
        response="$(curl -fsS -X POST \
            -H "Accept: application/json" \
            -d "client_id=$CLIENT_ID" \
            -d "device_code=$device_code" \
            -d "grant_type=urn:ietf:params:oauth:grant-type:device_code" \
            https://github.com/login/oauth/access_token)"

        local error access_token
        error="$(jq -r '.error // empty' <<<"$response")"
        access_token="$(jq -r '.access_token // empty' <<<"$response")"

        case "$error" in
            authorization_pending)  continue ;;
            slow_down)
                # GitHub asked us to back off; bump the interval.
                interval=$(( interval + 5 ))
                continue
                ;;
            "" )
                if [[ -n "$access_token" ]]; then
                    printf '%s' "$response"
                    return 0
                fi
                ;;
            * )
                die "Device-flow error from GitHub: $error" \
"    Full response: $response"
                ;;
        esac
    done

    die "Device code expired before authorization." \
"    Re-run this script and complete the browser flow within the window."
}

store_token() {
    local response="$1"
    local token refresh_token
    token="$(jq -r '.access_token'              <<<"$response")"
    refresh_token="$(jq -r '.refresh_token // empty' <<<"$response")"

    if [[ -z "$token" || "$token" == "null" ]]; then
        die "Missing access_token in successful response." \
"    Full response: $response"
    fi

    printf '%s' "$token" \
      | sudo -u "$MAESTRO_USER" -H bash -c "pass insert -e -f -- '$PASS_KEY' >/dev/null"
    pass "Token stored at $PASS_KEY (in maestro's pass store)"

    if [[ -n "$refresh_token" ]]; then
        warn "GitHub returned a refresh_token — the App still has User-to-server token expiration enabled."
        warn "Click 'Opt-out' next to 'User-to-server token expiration' under App settings → Optional Features to get a permanent token."
        warn "Otherwise this token expires in ~8h and jam-svc-repo will fall back to the installation token + the [bot] author."
    else
        pass "Token is non-expiring (no refresh_token returned)."
    fi
}

verify_token() {
    local response="$1"
    local token
    token="$(jq -r '.access_token' <<<"$response")"
    local user_response
    user_response="$(curl -fsS -H "Authorization: Bearer $token" \
        -H "Accept: application/vnd.github+json" \
        https://api.github.com/user 2>&1 || true)"
    local login
    login="$(jq -r '.login // empty' <<<"$user_response" 2>/dev/null || true)"
    if [[ -n "$login" ]]; then
        pass "Token authenticates as user: $login"
    else
        warn "Could not verify token identity (response: $user_response)"
    fi
}

main() {
    parse_args "$@"
    header "GitHub App user-to-server authorization"

    header "Preflight"
    check_prereqs
    prompt_for_client_id

    header "Requesting device code"
    local device_response
    device_response="$(request_device_code)"
    pass "Device code received"

    local token_response
    token_response="$(poll_for_token "$device_response")"

    header "Storing token"
    store_token "$token_response"

    header "Verifying token"
    verify_token "$token_response"

    header "Done."
    printf "
Next steps:
  - jam-svc-repo will pick up the new token at next startup.
  - Hot-patch the running service if you want it live now:
        jam deploy repo
  - The next orchestrator-opened PR should be attributed to the
    authorizing user (is_bot:false) and CodeRabbit should auto-review.

"
}

main "$@"
