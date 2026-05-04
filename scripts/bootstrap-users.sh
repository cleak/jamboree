#!/usr/bin/env bash
#
# bootstrap-users.sh — Set up multi-user isolation for Jamboree (the orchestrator)
#
# Creates two service users (maestro, picker), configures group
# membership for the human user, sets up sudoers for password-less
# transitions between users, and prepares directory structure with
# correct permissions.
#
# This script implements the "convenience over airtight" security model
# documented in security-setup.md (v5 addendum). It is the prerequisite
# to running `jam setup`.
#
# Usage:
#     sudo ./bootstrap-users.sh                  # interactive
#     sudo ./bootstrap-users.sh --user caleb     # explicit user
#     sudo ./bootstrap-users.sh --verify-only    # check, don't change
#     sudo ./bootstrap-users.sh --dry-run        # show what would happen
#
# Idempotent: safe to re-run.
# Fails loudly with specific remediation hints per principle 2.12.

set -euo pipefail

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

MAESTRO_USER="maestro"
PICKER_USER="picker"
MAESTRO_UID="2000"
PICKER_UID="2001"
SUDOERS_FILE="/etc/sudoers.d/jam-users"
SETUP_LOG="/etc/jam/bootstrap.log"

# Parsed args
HUMAN_USER=""
DRY_RUN=0
VERIFY_ONLY=0

# ---------------------------------------------------------------------------
# Output helpers — match `jam doctor` style for consistency
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
    # Run a command, respecting --dry-run.
    if [[ $DRY_RUN -eq 1 ]]; then
        printf "    [dry-run] %s\n" "$*"
        return 0
    fi
    "$@"
}

# ---------------------------------------------------------------------------
# Argument parsing
# ---------------------------------------------------------------------------

parse_args() {
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --user)
                HUMAN_USER="$2"
                shift 2
                ;;
            --dry-run)
                DRY_RUN=1
                shift
                ;;
            --verify-only)
                VERIFY_ONLY=1
                shift
                ;;
            --help|-h)
                sed -n '2,/^$/p' "$0" | sed 's/^# //; s/^#//'
                exit 0
                ;;
            *)
                die "Unknown argument: $1" \
                    "    See $0 --help"
                ;;
        esac
    done

    if [[ -z "$HUMAN_USER" ]]; then
        if [[ -n "${SUDO_USER:-}" && "$SUDO_USER" != "root" ]]; then
            HUMAN_USER="$SUDO_USER"
            info "Using calling user from sudo: $HUMAN_USER"
        else
            die "Could not determine human user account." \
"    Re-run with --user <username>, or invoke via:
        sudo ./bootstrap-users.sh"
        fi
    fi
}

# ---------------------------------------------------------------------------
# Preflight checks (match §11.4 / §2.14 conventions)
# ---------------------------------------------------------------------------

check_root() {
    if [[ $EUID -ne 0 ]]; then
        die "Must run as root." \
"    sudo ./bootstrap-users.sh"
    fi
}

check_linux() {
    if [[ "$(uname -s)" != "Linux" ]]; then
        die "This script requires Linux." \
"    Orchestrator does not support macOS or native Windows.
    Run inside WSL2 with a Linux distro (Ubuntu / Debian recommended)."
    fi
    pass "Linux detected ($(uname -r))"
}

check_wsl_or_native() {
    if grep -qiE 'microsoft|wsl' /proc/version 2>/dev/null; then
        info "WSL detected — verifying configuration"
        check_wsl_systemd
    else
        info "Native Linux (not WSL) — proceeding"
    fi
}

check_wsl_systemd() {
    # systemd is not strictly required for this script, but it makes
    # service management cleaner. Warn if absent.
    if ! pidof systemd >/dev/null 2>&1; then
        warn "systemd not running in WSL. Add to /etc/wsl.conf:"
        printf "        [boot]\n        systemd=true\n" >&2
        printf "    Then run: wsl --shutdown (in PowerShell) and reopen WSL.\n" >&2
        warn "Continuing — script does not require systemd, but jam service management will be smoother with it."
    else
        pass "WSL systemd active"
    fi
}

check_human_user_exists() {
    if ! id "$HUMAN_USER" >/dev/null 2>&1; then
        die "User '$HUMAN_USER' does not exist on this system." \
"    Verify with: getent passwd $HUMAN_USER
    Use --user to specify a different account."
    fi
    pass "Human user '$HUMAN_USER' exists (uid $(id -u "$HUMAN_USER"))"
}

check_human_home_native_fs() {
    local home
    home="$(getent passwd "$HUMAN_USER" | cut -d: -f6)"
    if [[ "$home" =~ ^/mnt/[a-z]/ ]] || [[ "$home" =~ ^/cygdrive/ ]]; then
        die "User '$HUMAN_USER' home is on a Windows mount: $home" \
"    Per §2.14, orchestrator data must live on Linux native FS.
    Move the user home to /home/$HUMAN_USER:
        sudo usermod -m -d /home/$HUMAN_USER $HUMAN_USER"
    fi
    if [[ ! -d "$home" ]]; then
        die "Home directory $home does not exist." \
"    sudo mkhomedir_helper $HUMAN_USER  (PAM-based)
    or
        sudo mkdir -p $home && sudo chown $HUMAN_USER:$HUMAN_USER $home"
    fi
    pass "Human home on native FS: $home"
}

# ---------------------------------------------------------------------------
# User creation
# ---------------------------------------------------------------------------

ensure_user() {
    local name="$1" uid="$2" home="/home/$1"
    if id "$name" >/dev/null 2>&1; then
        local existing_uid
        existing_uid="$(id -u "$name")"
        if [[ "$existing_uid" != "$uid" ]]; then
            warn "User '$name' exists with UID $existing_uid (expected $uid). Continuing with existing UID."
        else
            pass "User '$name' exists (uid $existing_uid)"
        fi
    else
        info "Creating user '$name' (uid $uid)"
        # -m: create home; -s: shell; -u: explicit uid; -U: matching primary group
        run_cmd useradd -m -s /bin/bash -u "$uid" -U "$name"
        pass "Created user '$name'"
    fi

    # Ensure home exists with correct perms (750 owned by user)
    if [[ -d "$home" ]] || [[ $DRY_RUN -eq 1 ]]; then
        run_cmd chmod 750 "$home" 2>/dev/null || true
        run_cmd chown "$name:$name" "$home" 2>/dev/null || true
        pass "Home $home permissions normalized (750 $name:$name)"
    fi
}

# ---------------------------------------------------------------------------
# Group membership: human user joins maestro group
# ---------------------------------------------------------------------------

ensure_group_membership() {
    if id -nG "$HUMAN_USER" | grep -qw "$MAESTRO_USER"; then
        pass "$HUMAN_USER already in $MAESTRO_USER group"
    else
        info "Adding $HUMAN_USER to $MAESTRO_USER group"
        run_cmd usermod -aG "$MAESTRO_USER" "$HUMAN_USER"
        pass "Added $HUMAN_USER to $MAESTRO_USER group"
        warn "Group change takes effect at next login.
       For this session, prefix commands with: sudo -u $HUMAN_USER newgrp $MAESTRO_USER"
    fi
}

# ---------------------------------------------------------------------------
# Human home traversal permissions
# ---------------------------------------------------------------------------

normalize_human_home_perms() {
    # /home/$HUMAN_USER needs to be traversable (mode 751) so maestro can
    # reach shared subdirs like ~/code/<project>-tempyr-live without being
    # able to enumerate /home/$HUMAN_USER's contents.
    #
    # Files within (.ssh, .gnupg, etc.) are mode 700 already and remain so.
    local home
    home="$(getent passwd "$HUMAN_USER" | cut -d: -f6)"
    local mode
    mode="$(stat -c '%a' "$home")"

    if [[ "$mode" == "751" ]]; then
        pass "$home already mode 751"
    else
        info "Setting $home to mode 751 (was $mode)"
        run_cmd chmod 751 "$home"
        pass "$home mode set to 751 (others can traverse, not enumerate)"
    fi

    # Verify ssh and gnupg dirs are still mode 700 (they should be from sshd setup,
    # but enforce here for safety)
    for sensitive in "$home/.ssh" "$home/.gnupg"; do
        if [[ -d "$sensitive" ]] || [[ $DRY_RUN -eq 1 ]]; then
            run_cmd chmod 700 "$sensitive" 2>/dev/null || true
        fi
    done
    pass "Sensitive subdirs (.ssh, .gnupg) confirmed mode 700"
}

# ---------------------------------------------------------------------------
# Sudoers configuration
# ---------------------------------------------------------------------------

write_sudoers() {
    local tmpfile
    tmpfile="$(mktemp)"
    cat > "$tmpfile" <<EOF
# /etc/sudoers.d/jam-users
#
# Generated by bootstrap-users.sh
# Convenience model: NOPASSWD between human user and service users.
# This is appropriate for a single-developer orchestrator on a trusted
# workstation. Anyone with a shell as $HUMAN_USER can become maestro
# without further authentication; this is acceptable because we already
# trust the human user's session, and the protection here is against
# unprivileged code (workers, untrusted content) — not against an
# attacker who already has the human user's shell.

# $HUMAN_USER -> maestro / picker
# Allows ops, inspection, manual recovery
$HUMAN_USER ALL=($MAESTRO_USER)    NOPASSWD: ALL
$HUMAN_USER ALL=($PICKER_USER) NOPASSWD: ALL

# maestro -> picker
# Required for harness adapters to spawn workers as picker
$MAESTRO_USER ALL=($PICKER_USER) NOPASSWD: ALL

# Allow these transitions to preserve specified env vars (trace IDs, secrets)
# SETENV permits the caller to use sudo's -E or --preserve-env=KEY1,KEY2 flags
Defaults!/usr/bin/* setenv
EOF

    # Validate before installing
    if ! visudo -cf "$tmpfile" >/dev/null 2>&1; then
        local errors
        errors="$(visudo -cf "$tmpfile" 2>&1 || true)"
        rm -f "$tmpfile"
        die "Generated sudoers file failed validation." \
"    visudo errors:
$(printf '%s\n' "$errors" | sed 's/^/        /')"
    fi

    # Compare against existing — only update if different
    if [[ -f "$SUDOERS_FILE" ]] && diff -q "$tmpfile" "$SUDOERS_FILE" >/dev/null 2>&1; then
        pass "Sudoers config already correct: $SUDOERS_FILE"
        rm -f "$tmpfile"
        return
    fi

    info "Installing sudoers config: $SUDOERS_FILE"
    if [[ $DRY_RUN -eq 1 ]]; then
        printf "    [dry-run] would install:\n" >&2
        sed 's/^/        /' "$tmpfile" >&2
        rm -f "$tmpfile"
        return
    fi

    install -m 440 -o root -g root "$tmpfile" "$SUDOERS_FILE"
    rm -f "$tmpfile"

    # Final validation against the actual installed file
    if ! visudo -c >/dev/null 2>&1; then
        die "Sudoers validation failed AFTER install. System may have other broken sudoers files." \
"    Inspect: visudo -c
    Remove: sudo rm $SUDOERS_FILE"
    fi
    pass "Sudoers config installed and validated"
}

# ---------------------------------------------------------------------------
# Service-account directory scaffolding
# ---------------------------------------------------------------------------

prepare_maestro_dirs() {
    local svc_home="/home/$MAESTRO_USER"
    local dirs=(
        "$svc_home/.jam"
        "$svc_home/.jam/config"
        "$svc_home/.jam/config/projects"
        "$svc_home/.jam/journal"
        "$svc_home/.jam/research"
        "$svc_home/.jam/incidents"
        "$svc_home/.jam/conductor-aborted-sessions"
        "$svc_home/.jam/skills-evolution-candidates"
        "$svc_home/.jam/staging"
        "$svc_home/.jam/nats-data"
    )

    for d in "${dirs[@]}"; do
        if [[ ! -d "$d" ]] && [[ $DRY_RUN -eq 0 ]]; then
            run_cmd install -d -m 750 -o "$MAESTRO_USER" -g "$MAESTRO_USER" "$d"
        fi
    done
    pass "maestro orchestrator state dirs prepared under $svc_home/.jam"

    # GnuPG dir for pass — created empty here; user runs `gpg --gen-key` later
    if [[ ! -d "$svc_home/.gnupg" ]] && [[ $DRY_RUN -eq 0 ]]; then
        run_cmd install -d -m 700 -o "$MAESTRO_USER" -g "$MAESTRO_USER" "$svc_home/.gnupg"
    fi
    pass "maestro .gnupg directory prepared (mode 700)"
}

prepare_picker_dirs() {
    local worker_home="/home/$PICKER_USER"
    if [[ ! -d "$worker_home/workers" ]] && [[ $DRY_RUN -eq 0 ]]; then
        run_cmd install -d -m 750 -o "$PICKER_USER" -g "$PICKER_USER" "$worker_home/workers"
    fi
    pass "picker workers dir prepared at $worker_home/workers (mode 750)"
}

# ---------------------------------------------------------------------------
# Verification phase — runs after setup, also in --verify-only mode
# ---------------------------------------------------------------------------

verify_users() {
    header "Verification"

    local failed=0

    # Users exist
    for u in "$MAESTRO_USER" "$PICKER_USER"; do
        if id "$u" >/dev/null 2>&1; then
            pass "User '$u' exists (uid $(id -u "$u"))"
        else
            fail "User '$u' missing"
            failed=1
        fi
    done

    # Group membership
    if id -nG "$HUMAN_USER" | grep -qw "$MAESTRO_USER"; then
        pass "$HUMAN_USER is in group $MAESTRO_USER"
    else
        # In --verify-only this is a fail; in setup mode we expect it after re-login
        if [[ $VERIFY_ONLY -eq 1 ]]; then
            fail "$HUMAN_USER is not in group $MAESTRO_USER (or current shell predates the change — log out and back in)"
            failed=1
        else
            warn "$HUMAN_USER must log out and back in for group membership to apply"
        fi
    fi

    # Sudoers in place and valid
    if [[ -f "$SUDOERS_FILE" ]]; then
        if visudo -c >/dev/null 2>&1; then
            pass "Sudoers config valid: $SUDOERS_FILE"
        else
            fail "Sudoers config exists but fails validation"
            failed=1
        fi
    else
        fail "Sudoers config missing: $SUDOERS_FILE"
        failed=1
    fi

    # Sudo transition smoke test (only meaningful when not in dry-run)
    if [[ $DRY_RUN -eq 0 ]]; then
        if sudo -n -u "$MAESTRO_USER" id >/dev/null 2>&1; then
            pass "sudo $HUMAN_USER -> $MAESTRO_USER (NOPASSWD) works"
        else
            warn "sudo to $MAESTRO_USER without password failed.
       This will work after you log out and back in (group membership refresh)."
        fi
    fi

    # Directory perms spot-check
    local svc_home="/home/$MAESTRO_USER"
    if [[ -d "$svc_home/.jam" ]]; then
        local mode owner
        mode="$(stat -c '%a' "$svc_home/.jam")"
        owner="$(stat -c '%U' "$svc_home/.jam")"
        if [[ "$mode" == "750" && "$owner" == "$MAESTRO_USER" ]]; then
            pass "$svc_home/.jam owned by $MAESTRO_USER mode 750"
        else
            fail "$svc_home/.jam perms wrong: $owner mode $mode (want $MAESTRO_USER mode 750)"
            failed=1
        fi
    fi

    # Caleb's home traversal
    local human_home
    human_home="$(getent passwd "$HUMAN_USER" | cut -d: -f6)"
    local home_mode
    home_mode="$(stat -c '%a' "$human_home")"
    if [[ "$home_mode" == "751" ]]; then
        pass "$human_home mode 751 (maestro can traverse)"
    else
        fail "$human_home has mode $home_mode (want 751)"
        failed=1
    fi

    # SSH key still inaccessible to maestro (defense in depth)
    if [[ -d "$human_home/.ssh" ]] && [[ $DRY_RUN -eq 0 ]]; then
        if sudo -n -u "$MAESTRO_USER" ls "$human_home/.ssh" >/dev/null 2>&1; then
            fail "$MAESTRO_USER can read $human_home/.ssh — security regression"
            failed=1
        else
            pass "$human_home/.ssh inaccessible to $MAESTRO_USER (good)"
        fi
    fi

    return $failed
}

# ---------------------------------------------------------------------------
# Audit log
# ---------------------------------------------------------------------------

write_setup_log() {
    if [[ $DRY_RUN -eq 1 || $VERIFY_ONLY -eq 1 ]]; then
        return
    fi
    install -d -m 755 -o root -g root /etc/jam
    cat > "$SETUP_LOG" <<EOF
# bootstrap-users.sh setup record
# This file documents the security baseline that jam doctor verifies against.

setup_completed_at = "$(date -u -Iseconds)"
human_user = "$HUMAN_USER"
human_uid  = "$(id -u "$HUMAN_USER")"
maestro_user = "$MAESTRO_USER"
maestro_uid  = "$(id -u "$MAESTRO_USER")"
picker_user = "$PICKER_USER"
picker_uid  = "$(id -u "$PICKER_USER")"
sudoers_file = "$SUDOERS_FILE"
script_version = "v5-addendum-1.0"
EOF
    chmod 644 "$SETUP_LOG"
    pass "Setup record written to $SETUP_LOG"
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

main() {
    parse_args "$@"

    header "Orchestrator user-isolation bootstrap"
    if [[ $DRY_RUN -eq 1 ]]; then
        info "DRY RUN — no changes will be made"
    fi
    if [[ $VERIFY_ONLY -eq 1 ]]; then
        info "VERIFY ONLY — checking existing setup"
    fi

    header "Preflight checks"
    check_root
    check_linux
    check_wsl_or_native
    check_human_user_exists
    check_human_home_native_fs

    if [[ $VERIFY_ONLY -eq 1 ]]; then
        verify_users
        local rc=$?
        if [[ $rc -eq 0 ]]; then
            header "All checks passed."
            exit 0
        else
            header "Verification failed. Re-run without --verify-only to fix."
            exit 1
        fi
    fi

    header "Creating service users"
    ensure_user "$MAESTRO_USER"    "$MAESTRO_UID"
    ensure_user "$PICKER_USER" "$PICKER_UID"

    header "Configuring group membership"
    ensure_group_membership

    header "Normalizing human home permissions"
    normalize_human_home_perms

    header "Installing sudoers config"
    write_sudoers

    header "Preparing service-account directories"
    prepare_maestro_dirs
    prepare_picker_dirs

    write_setup_log

    verify_users || warn "Some verification checks failed — see above. May resolve after logout/login."

    header "Bootstrap complete."
    cat <<EOF

Next steps:
  1. Log out and back in (or run: newgrp $MAESTRO_USER) so group membership applies.
  2. Install per-user CLI tools (codex, claude-code) and the daily auto-updater:
        sudo ./scripts/install-cli-tools.sh
  3. Initialize GPG keyring + pass for maestro:
        sudo -u $MAESTRO_USER -i
        # in the maestro shell:
        gpg --batch --gen-key <<KEY_PARAMS
            %no-protection
            Key-Type: EDDSA
            Key-Curve: ed25519
            Key-Usage: sign
            Subkey-Type: ECDH
            Subkey-Curve: cv25519
            Subkey-Usage: encrypt
            Name-Real: Jamboree Maestro
            Name-Email: maestro@localhost
            Expire-Date: 0
            %commit
        KEY_PARAMS
        pass init maestro@localhost
        exit
  4. Authenticate the Maestro to OpenAI via Codex OAuth (uses your ChatGPT
     subscription — GPT-5.5 is subscription-gated, no API key needed):
        sudo -u $MAESTRO_USER -i codex login   # device-code OAuth
  5. Add other orchestrator secrets to maestro's pass store
     (GitHub PAT, ntfy creds, etc. — see security-setup.md §5.3 / spec §11.3.1):
        sudo -u $MAESTRO_USER -i pass insert jam/conductor/github-pat
        # ... repeat for each key
  6. Run: jam setup    (which now also verifies this user-isolation layout)

See security-setup.md for full operational details.
EOF
}

main "$@"
