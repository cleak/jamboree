#!/usr/bin/env bash
#
# install-substrate.sh — Download and install nats-server + process-compose
# binaries to /opt/jam/bin/.
#
# Per `dec-single-node-jetstream` and security-setup §7.4. Pinned versions
# avoid surprise upgrades; override via env vars NATS_VERSION /
# PROCESS_COMPOSE_VERSION when known to be needed.
#
# Usage:
#     sudo ./scripts/install-substrate.sh
#     sudo ./scripts/install-substrate.sh --dry-run
#     sudo ./scripts/install-substrate.sh --verify-only
#
# Idempotent: skips downloads when the target version is already installed.
# Validates SHA-256 checksums (set CHECKSUM_VERIFY=0 to skip; not recommended).

set -euo pipefail

NATS_VERSION="${NATS_VERSION:-v2.11.0}"
PROCESS_COMPOSE_VERSION="${PROCESS_COMPOSE_VERSION:-v1.40.1}"
INSTALL_DIR="${INSTALL_DIR:-/opt/jam/bin}"
CACHE_DIR="${CACHE_DIR:-/var/cache/jam-substrate}"
ARCH="${ARCH:-linux_amd64}"
CHECKSUM_VERIFY="${CHECKSUM_VERIFY:-1}"

DRY_RUN=0
VERIFY_ONLY=0

# ---------------------------------------------------------------------------
# Output helpers — match bootstrap-users.sh style
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

parse_args() {
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --dry-run)    DRY_RUN=1; shift ;;
            --verify-only) VERIFY_ONLY=1; shift ;;
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

check_root() {
    if [[ $EUID -ne 0 ]]; then
        die "Must run as root." "    sudo ./scripts/install-substrate.sh"
    fi
}

check_linux() {
    if [[ "$(uname -s)" != "Linux" ]]; then
        die "This script requires Linux." \
"    Substrate requires nats-server and process-compose Linux binaries.
    macOS / Windows-native: out of scope (principle-linux-only-deployment)."
    fi
    pass "Linux detected ($(uname -r))"
}

check_arch() {
    local arch
    arch="$(uname -m)"
    case "$arch" in
        x86_64) ARCH="linux_amd64" ;;
        aarch64|arm64) ARCH="linux_arm64" ;;
        *)
            die "Unsupported architecture: $arch" \
"    nats-server and process-compose ship binaries for amd64 and arm64 only.
    Override via ARCH=<arch> if you have a custom build at \$INSTALL_DIR."
            ;;
    esac
    pass "Architecture: $arch -> $ARCH"
}

check_curl_and_sha256() {
    local missing=()
    command -v curl >/dev/null 2>&1 || missing+=(curl)
    command -v sha256sum >/dev/null 2>&1 || missing+=(sha256sum)
    command -v tar >/dev/null 2>&1 || missing+=(tar)
    if [[ ${#missing[@]} -ne 0 ]]; then
        die "Missing tools: ${missing[*]}" \
"    sudo apt install ${missing[*]}"
    fi
    pass "curl, sha256sum, tar available"
}

# ---------------------------------------------------------------------------
# nats-server install
# ---------------------------------------------------------------------------

install_nats() {
    local installed_version=""
    if [[ -x "$INSTALL_DIR/nats-server" ]]; then
        installed_version="$("$INSTALL_DIR/nats-server" --version 2>&1 | awk '/version/ {print $NF; exit}' || true)"
    fi

    if [[ "$installed_version" == "${NATS_VERSION#v}" ]]; then
        pass "nats-server $NATS_VERSION already installed at $INSTALL_DIR"
        return
    fi

    if [[ -n "$installed_version" ]]; then
        info "nats-server $installed_version installed; upgrading to $NATS_VERSION"
    else
        info "Installing nats-server $NATS_VERSION"
    fi

    local archive="nats-server-${NATS_VERSION}-${ARCH}.tar.gz"
    local url="https://github.com/nats-io/nats-server/releases/download/${NATS_VERSION}/${archive}"
    local cached="$CACHE_DIR/$archive"

    run_cmd mkdir -p "$CACHE_DIR"
    if [[ ! -f "$cached" ]]; then
        info "Downloading $url"
        run_cmd curl -fsSL -o "$cached" "$url"
    else
        info "Using cached $cached"
    fi

    if [[ "$CHECKSUM_VERIFY" == "1" && $DRY_RUN -eq 0 ]]; then
        local sums="$CACHE_DIR/nats-${NATS_VERSION}-SHA256SUMS"
        if [[ ! -f "$sums" ]]; then
            curl -fsSL -o "$sums" "https://github.com/nats-io/nats-server/releases/download/${NATS_VERSION}/SHA256SUMS"
        fi
        local expected
        expected="$(awk -v f="$archive" '$2==f{print $1}' "$sums" || true)"
        if [[ -z "$expected" ]]; then
            warn "No SHA-256 entry for $archive in upstream SHA256SUMS — skipping verify"
        else
            local actual
            actual="$(sha256sum "$cached" | awk '{print $1}')"
            if [[ "$actual" != "$expected" ]]; then
                die "SHA-256 mismatch for $archive" \
"    expected: $expected
    actual:   $actual
    Re-run after deleting $cached"
            fi
            pass "nats-server checksum verified"
        fi
    fi

    run_cmd mkdir -p "$INSTALL_DIR"
    if [[ $DRY_RUN -eq 0 ]]; then
        local tmpdir
        tmpdir="$(mktemp -d)"
        tar -C "$tmpdir" -xzf "$cached"
        # Archive structure: nats-server-<ver>-<arch>/nats-server
        find "$tmpdir" -name nats-server -type f -executable -exec install -m 755 {} "$INSTALL_DIR/nats-server" \;
        rm -rf "$tmpdir"
    else
        run_cmd tar -C "<tmp>" -xzf "$cached"
        run_cmd install -m 755 "<tmp>/.../nats-server" "$INSTALL_DIR/nats-server"
    fi
    pass "nats-server installed at $INSTALL_DIR/nats-server"
}

# ---------------------------------------------------------------------------
# process-compose install
# ---------------------------------------------------------------------------

install_process_compose() {
    local installed_version=""
    if [[ -x "$INSTALL_DIR/process-compose" ]]; then
        installed_version="v$("$INSTALL_DIR/process-compose" version 2>&1 | head -1 | awk '{print $NF}' | sed 's/^v//' || true)"
    fi

    if [[ "$installed_version" == "$PROCESS_COMPOSE_VERSION" ]]; then
        pass "process-compose $PROCESS_COMPOSE_VERSION already installed"
        return
    fi

    if [[ -n "$installed_version" ]]; then
        info "process-compose $installed_version installed; upgrading to $PROCESS_COMPOSE_VERSION"
    else
        info "Installing process-compose $PROCESS_COMPOSE_VERSION"
    fi

    # process-compose ships as a single binary inside a tar.gz archive.
    # Naming convention as of v1.x: process-compose_${VERSION#v}_linux_amd64.tar.gz
    local pc_arch
    case "$ARCH" in
        linux_amd64) pc_arch="linux_amd64" ;;
        linux_arm64) pc_arch="linux_arm64" ;;
        *) pc_arch="$ARCH" ;;
    esac
    local version_no_v="${PROCESS_COMPOSE_VERSION#v}"
    local archive="process-compose_${version_no_v}_${pc_arch}.tar.gz"
    local url="https://github.com/F1bonacc1/process-compose/releases/download/${PROCESS_COMPOSE_VERSION}/${archive}"
    local cached="$CACHE_DIR/$archive"

    if [[ ! -f "$cached" ]]; then
        info "Downloading $url"
        run_cmd curl -fsSL -o "$cached" "$url"
    else
        info "Using cached $cached"
    fi

    if [[ $DRY_RUN -eq 0 ]]; then
        local tmpdir
        tmpdir="$(mktemp -d)"
        tar -C "$tmpdir" -xzf "$cached"
        find "$tmpdir" -name process-compose -type f -executable \
            -exec install -m 755 {} "$INSTALL_DIR/process-compose" \;
        rm -rf "$tmpdir"
    else
        run_cmd tar -C "<tmp>" -xzf "$cached"
        run_cmd install -m 755 "<tmp>/process-compose" "$INSTALL_DIR/process-compose"
    fi
    pass "process-compose installed at $INSTALL_DIR/process-compose"
}

# ---------------------------------------------------------------------------
# Verification
# ---------------------------------------------------------------------------

verify_install() {
    header "Verification"
    local failed=0

    for bin in nats-server process-compose; do
        if [[ -x "$INSTALL_DIR/$bin" ]]; then
            pass "$INSTALL_DIR/$bin executable"
        else
            fail "$INSTALL_DIR/$bin missing or non-executable"
            failed=1
        fi
    done

    return $failed
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

main() {
    parse_args "$@"

    header "Substrate binary installer"
    if [[ $DRY_RUN -eq 1 ]]; then
        info "DRY RUN — no changes will be made"
    fi
    if [[ $VERIFY_ONLY -eq 1 ]]; then
        info "VERIFY ONLY — no downloads or installs"
    fi

    header "Preflight checks"
    check_root
    check_linux
    check_arch
    check_curl_and_sha256

    if [[ $VERIFY_ONLY -eq 1 ]]; then
        verify_install
        exit $?
    fi

    header "Install nats-server"
    install_nats

    header "Install process-compose"
    install_process_compose

    verify_install || warn "Some verification checks failed."

    header "Substrate ready."
    cat <<EOF

Next steps:
  1. Verify the layout:  $INSTALL_DIR/nats-server --version
                          $INSTALL_DIR/process-compose version
  2. Start the substrate (after Phase 0 binaries land):
         sudo -u maestro $INSTALL_DIR/process-compose -f /home/caleb/jamboree/process-compose.yaml up
EOF
}

main "$@"
