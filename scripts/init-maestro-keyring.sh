#!/usr/bin/env bash
#
# init-maestro-keyring.sh — One-time GPG keyring + pass store init for maestro.
#
# Implements security-setup.md §5.1 (GPG key) and §5.2 (pass init) without
# the heredoc-paste fragility that fails over `sudo -i` on some terminals.
#
# Run AS the maestro user, after install-cli-tools.sh and after Codex/Claude
# OAuth logins are complete:
#
#     sudo -u maestro -i ~/scripts/init-maestro-keyring.sh
#
# Idempotent: re-running detects existing key + pass store and skips.
#
# Convenience-first: generates a passphrase-less ed25519/cv25519 key. The
# orchestrator runs unattended; pinentry prompts on every secret access
# defeat the orchestration model. See security-setup.md §5.1 for rationale.
# If you want a passphrase, run `gpg --full-generate-key` manually instead
# and skip this script.

set -euo pipefail

KEY_NAME_REAL="Jamboree Maestro"
KEY_NAME_EMAIL="maestro@localhost"

PASS_GLYPH='\033[32m✓\033[0m'
FAIL_GLYPH='\033[31m✗\033[0m'
INFO_GLYPH='\033[34mi\033[0m'

pass_msg() { printf "  ${PASS_GLYPH} %s\n" "$*"; }
fail()     { printf "  ${FAIL_GLYPH} %s\n" "$*" >&2; }
info()     { printf "  ${INFO_GLYPH} %s\n" "$*"; }
header()   { printf "\n\033[1m%s\033[0m\n" "$*"; }

die() {
    fail "$1"
    [[ -n "${2:-}" ]] && printf "\n    Fix:\n%s\n\n" "$2" >&2
    exit 1
}

check_running_as_maestro() {
    if [[ "$(id -un)" != "maestro" ]]; then
        die "This script must run as the maestro user (currently $(id -un))." \
"    sudo -u maestro -i ~/scripts/init-maestro-keyring.sh"
    fi
}

check_tools() {
    for cmd in gpg pass; do
        command -v "$cmd" >/dev/null 2>&1 || \
            die "$cmd not found on PATH." \
"    Install via apt:  sudo apt install gnupg pass"
    done
    pass_msg "gpg + pass present"
}

generate_gpg_key() {
    if gpg --list-secret-keys --with-colons 2>/dev/null \
        | grep -q ":${KEY_NAME_EMAIL}>"
    then
        pass_msg "GPG key for ${KEY_NAME_EMAIL} already exists — skipping"
        return 0
    fi

    info "Generating ed25519/cv25519 keypair for ${KEY_NAME_EMAIL}"

    local params
    params="$(mktemp "$HOME/.maestro-key.params.XXXXXX")"
    chmod 600 "$params"
    # Trap so the params file is removed even on failure (it has no secret
    # in it, but tidy is tidy).
    trap 'rm -f "$params"' RETURN

    cat > "$params" <<KEY_PARAMS
%no-protection
Key-Type: EDDSA
Key-Curve: ed25519
Key-Usage: sign
Subkey-Type: ECDH
Subkey-Curve: cv25519
Subkey-Usage: encrypt
Name-Real: ${KEY_NAME_REAL}
Name-Email: ${KEY_NAME_EMAIL}
Expire-Date: 0
%commit
KEY_PARAMS

    gpg --batch --pinentry-mode loopback --gen-key "$params"
    pass_msg "GPG key generated"
}

init_pass_store() {
    if [[ -f "$HOME/.password-store/.gpg-id" ]]; then
        local existing
        existing="$(cat "$HOME/.password-store/.gpg-id")"
        if [[ "$existing" == "$KEY_NAME_EMAIL" ]]; then
            pass_msg "pass store already initialised for ${KEY_NAME_EMAIL}"
            return 0
        else
            die "pass store exists but is keyed to '${existing}', not '${KEY_NAME_EMAIL}'." \
"    If this is unexpected, inspect ~/.password-store/.gpg-id and resolve manually.
    To re-init from scratch (DESTROYS existing entries):
        rm -rf ~/.password-store && $0"
        fi
    fi

    info "Initialising pass store keyed to ${KEY_NAME_EMAIL}"
    pass init "${KEY_NAME_EMAIL}"
    pass_msg "pass store ready at ~/.password-store/"
}

verify() {
    gpg --list-secret-keys "${KEY_NAME_EMAIL}" >/dev/null 2>&1 \
        && pass_msg "Verified: GPG secret key for ${KEY_NAME_EMAIL}" \
        || { fail "GPG secret key not found"; return 1; }

    [[ -f "$HOME/.password-store/.gpg-id" ]] \
        && pass_msg "Verified: pass store at ~/.password-store/" \
        || { fail "pass store missing"; return 1; }
}

print_next_steps() {
    cat <<EOF

Next: populate the orchestrator's secrets (security-setup.md §5.3).
Each command prompts for the secret; -m means multi-line (paste, then Ctrl+D):

    pass insert    jam/pickers/github-app-id
    pass insert    jam/pickers/github-app-installation-id
    pass insert -m jam/pickers/github-app-key
    pass insert    jam/search/brave
    pass insert    jam/search/firecrawl
    pass insert    jam/notify/ntfy-token
    pass insert    jam/nats/token

(Full key list: spec §11.3.1. Codex OAuth is already covered by
~/.codex/auth.json, so no jam/maestro/* entry is needed for the default
ChatGPT-subscription setup.)

Verify with:

    pass list
    pass show jam/pickers/github-app-id
    pass show jam/pickers/github-app-installation-id
EOF
}

main() {
    header "Maestro keyring init"
    check_running_as_maestro
    check_tools
    generate_gpg_key
    init_pass_store
    header "Verification"
    verify || die "Verification failed — see above."
    print_next_steps
}

main "$@"
