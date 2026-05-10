#!/usr/bin/env bash
#
# Rootless smoke for the production substrate installer verification contract.
# It stages the pinned third-party binaries plus the enabled first-party
# runtime binaries and UI bundle into temporary runtime dirs, then runs
# install-substrate.sh --verify-only against those dirs.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SMOKE_DIR=""

PASS_GLYPH='\033[32m✓\033[0m'
FAIL_GLYPH='\033[31m✗\033[0m'
INFO_GLYPH='\033[34mi\033[0m'

pass() { printf "  ${PASS_GLYPH} %s\n" "$*"; }
fail() { printf "  ${FAIL_GLYPH} %s\n" "$*" >&2; }
info() { printf "  ${INFO_GLYPH} %s\n" "$*"; }

die() {
    fail "$1"
    if [[ -n "${2:-}" ]]; then
        printf "\n    Fix:\n%s\n\n" "$2" >&2
    fi
    exit 1
}

cleanup() {
    if [[ -n "$SMOKE_DIR" ]]; then
        rm -rf "$SMOKE_DIR"
    fi
}
trap cleanup EXIT

need() {
    command -v "$1" >/dev/null 2>&1 || die "missing required command: $1"
}

find_bin() {
    local name="$1"
    local env_var="$2"
    local configured="${!env_var:-}"

    if [[ -n "$configured" ]]; then
        [[ -x "$configured" ]] || die "$env_var is not executable: $configured"
        printf '%s\n' "$configured"
        return
    fi

    if command -v "$name" >/dev/null 2>&1; then
        command -v "$name"
        return
    fi

    local cached="/tmp/jam-substrate/bin/$name"
    if [[ -x "$cached" ]]; then
        printf '%s\n' "$cached"
        return
    fi

    die "missing required binary: $name" \
"    Install production binaries with sudo ./scripts/install-substrate.sh, or
    run this smoke with $env_var=/path/to/$name."
}

need cargo
need install
need npm

NATS_SERVER_BIN="$(find_bin nats-server NATS_SERVER_BIN)"
PROCESS_COMPOSE_BIN="$(find_bin process-compose PROCESS_COMPOSE_BIN)"

cd "$ROOT"

info "building enabled first-party runtime binaries"
cargo build --release \
    -p jam-cli \
    -p jam-nats-bridge \
    -p jam-svc-message \
    -p jam-svc-supervise \
    -p jam-ui-server

info "building UI static bundle"
npm --prefix ui ci
npm --prefix ui run build

SMOKE_DIR="$(mktemp -d /tmp/jam-install-substrate-smoke.XXXXXX)"
INSTALL_DIR="$SMOKE_DIR/bin"
UI_DIST_DIR="$SMOKE_DIR/ui/dist"
mkdir -p "$INSTALL_DIR"
mkdir -p "$UI_DIST_DIR"

install -m 755 "$NATS_SERVER_BIN" "$INSTALL_DIR/nats-server"
install -m 755 "$PROCESS_COMPOSE_BIN" "$INSTALL_DIR/process-compose"

for bin in jam jam-nats-bridge jam-svc-message jam-svc-supervise jam-ui-server; do
    install -m 755 "$ROOT/target/release/$bin" "$INSTALL_DIR/$bin"
done

cp -a "$ROOT/ui/dist/." "$UI_DIST_DIR/"

INSTALL_DIR="$INSTALL_DIR" UI_DIST_DIR="$UI_DIST_DIR" scripts/install-substrate.sh --verify-only
pass "install-substrate verify-only smoke passed"
