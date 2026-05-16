#!/usr/bin/env bash
#
# install-substrate.sh — Download/install substrate binaries and the enabled
# first-party Jamboree runtime binaries to /opt/jam/bin/.
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

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
NATS_VERSION="${NATS_VERSION:-v2.11.0}"
PROCESS_COMPOSE_VERSION="${PROCESS_COMPOSE_VERSION:-v1.40.1}"
INSTALL_DIR="${INSTALL_DIR:-/opt/jam/bin}"
CACHE_DIR="${CACHE_DIR:-/var/cache/jam-substrate}"
ARCH="${ARCH:-linux_amd64}"
CHECKSUM_VERIFY="${CHECKSUM_VERIFY:-1}"
BUILD_USER="${BUILD_USER:-}"
UI_DIST_DIR="${UI_DIST_DIR:-/home/maestro/.jam/ui/dist}"
UI_SRC_DIR="$REPO_ROOT/ui"
MAESTRO_SRC_DIR="$REPO_ROOT/maestro"
MAESTRO_INSTALL_DIR="${MAESTRO_INSTALL_DIR:-/opt/jam/maestro}"
UV_BIN="${UV_BIN:-}"
MAESTRO_USER="${MAESTRO_USER:-maestro}"
PICKER_USER="${PICKER_USER:-picker}"
JAM_BLUEBERRY_REPO="${JAM_BLUEBERRY_REPO:-/home/caleb/blueberry}"
JAM_WORKTREE_ROOT="${JAM_WORKTREE_ROOT:-/home/picker/workers}"
JAM_CANONICAL_TEMPYR_WORKTREE="${JAM_CANONICAL_TEMPYR_WORKTREE:-/home/caleb/blueberry-jam}"
JAM_CANONICAL_TEMPYR_BRANCH="${JAM_CANONICAL_TEMPYR_BRANCH:-tempyr-live}"
GITHUB_APP_CREDENTIAL_HELPER_SRC="$REPO_ROOT/scripts/github-app-git-credential.py"
GITHUB_APP_CREDENTIAL_HELPER_DEST="$INSTALL_DIR/jam-github-app-git-credential"

FIRST_PARTY_PACKAGES=(
    jam-cli
    jam-nats-bridge
    jam-patch-agent
    jam-pr-poller
    jam-svc-message
    jam-svc-observe
    jam-svc-repo
    jam-svc-supervise
    jam-svc-session
    jam-svc-worktree
    jam-task-lifecycle
    jam-ui-server
)

FIRST_PARTY_BINS=(
    jam
    jam-nats-bridge
    jam-patch-agent
    jam-pr-poller
    jam-svc-message
    jam-svc-observe
    jam-svc-repo
    jam-svc-supervise
    jam-svc-session
    jam-svc-worktree
    jam-task-lifecycle
    jam-ui-server
)

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

run_as() {
    local user="$1"
    if [[ $DRY_RUN -eq 1 ]]; then
        printf "    [dry-run] sudo -u %s -i bash <<-EOF\n" "$user"
        sed 's/^/        /'
        printf "    [dry-run] EOF\n"
        return 0
    fi
    # `sudo -u <user> -i bash -c <stdin>` runs bash non-login/non-interactive, so
    # it doesn't read .profile/.bashrc/.zshrc. Pre-source ~/.cargo/env (the
    # standard rustup PATH shim) so build users that only wire cargo into a
    # zsh-specific rc file still have it on PATH here.
    {
        printf '[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"\n'
        cat
    } | sudo -u "$user" -i bash
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
        if [[ $DRY_RUN -eq 1 || $VERIFY_ONLY -eq 1 ]]; then
            warn "not running as root — continuing because no writes will be made"
            return
        fi
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

detect_build_user() {
    if [[ -n "$BUILD_USER" ]]; then
        return
    fi

    local owner
    owner="$(stat -c '%U' "$REPO_ROOT" 2>/dev/null || true)"
    if [[ -n "$owner" && "$owner" != "root" ]]; then
        BUILD_USER="$owner"
    elif [[ -n "${SUDO_USER:-}" && "${SUDO_USER:-}" != "root" ]]; then
        BUILD_USER="$SUDO_USER"
    else
        BUILD_USER="caleb"
    fi
}

check_build_user() {
    detect_build_user
    if ! id "$BUILD_USER" >/dev/null 2>&1; then
        die "Build user does not exist: $BUILD_USER" \
"    Set BUILD_USER=<human-user> or run from the caleb-owned checkout."
    fi
    pass "Build user: $BUILD_USER"
}

check_build_tools() {
    if [[ $VERIFY_ONLY -eq 1 ]]; then
        return
    fi
    if ! run_as "$BUILD_USER" <<'EOF' >/dev/null
command -v cargo >/dev/null 2>&1
EOF
    then
        die "cargo is not available for build user $BUILD_USER" \
"    Install Rust for $BUILD_USER, then rerun:
        sudo -u $BUILD_USER -i rustup toolchain install stable"
    fi
    pass "cargo available for $BUILD_USER"

    if ! run_as "$BUILD_USER" <<'EOF' >/dev/null
command -v npm >/dev/null 2>&1
EOF
    then
        die "npm is not available for build user $BUILD_USER" \
"    Install Node.js/npm, then rerun:
        sudo apt install nodejs npm"
    fi
    pass "npm available for $BUILD_USER"

    if ! command -v python3 >/dev/null 2>&1; then
        die "python3 is not available" \
"    Install Python 3 and venv support, then rerun:
        sudo apt install python3 python3-venv"
    fi
    pass "python3 available ($(python3 --version 2>/dev/null))"

    detect_uv_bin
    if [[ -n "$UV_BIN" ]]; then
        pass "uv available at $UV_BIN"
    elif ! python3 - <<'PY'
import ensurepip
PY
    then
        die "neither uv nor Python ensurepip is available for Maestro virtualenv setup" \
"    Install uv for the build user, or install venv support, then rerun:
        curl -LsSf https://astral.sh/uv/install.sh | sh
    or:
        sudo apt install python3.12-venv"
    else
        pass "python ensurepip available"
    fi
}

# ---------------------------------------------------------------------------
# Runtime access repair
# ---------------------------------------------------------------------------

prepare_group_access_tree() {
    local path="$1" group="$2" label="$3"
    if [[ ! -e "$path" ]]; then
        warn "$label missing at $path; skipping permission repair"
        return
    fi
    info "Repairing $label group access at $path"
    run_cmd chgrp -R "$group" "$path"
    run_cmd chmod -R g+rwX "$path"
    if [[ $DRY_RUN -eq 1 ]]; then
        run_cmd find "$path" -type d -exec chmod g+s {} +
    else
        find "$path" -type d -exec chmod g+s {} +
    fi
}

ensure_safe_directory() {
    local user="$1" path="$2"
    if [[ $DRY_RUN -eq 1 ]]; then
        run_cmd sudo -u "$user" -H git config --global --add safe.directory "$path"
        return
    fi
    if sudo -u "$user" -H git config --global --get-all safe.directory 2>/dev/null \
        | grep -Fxq "$path"; then
        pass "$path already safe for $user git"
    else
        sudo -u "$user" -H git config --global --add safe.directory "$path"
        pass "$path marked safe for $user git"
    fi
}

install_github_app_git_helper() {
    if [[ ! -f "$GITHUB_APP_CREDENTIAL_HELPER_SRC" ]]; then
        warn "GitHub App credential helper missing at $GITHUB_APP_CREDENTIAL_HELPER_SRC"
        return
    fi
    run_cmd mkdir -p "$INSTALL_DIR"
    run_cmd install -m 755 "$GITHUB_APP_CREDENTIAL_HELPER_SRC" "$GITHUB_APP_CREDENTIAL_HELPER_DEST"
    run_cmd sudo -u "$MAESTRO_USER" -H git config --global \
        credential.https://github.com.helper "$GITHUB_APP_CREDENTIAL_HELPER_DEST"
    pass "GitHub App git credential helper configured for $MAESTRO_USER"
}

ensure_canonical_tempyr_worktree_registered() {
    if [[ ! -d "$JAM_BLUEBERRY_REPO/.git" ]]; then
        warn "Blueberry git metadata not found at $JAM_BLUEBERRY_REPO/.git; cannot verify canonical Tempyr worktree"
        return
    fi
    if [[ ! -d "$JAM_CANONICAL_TEMPYR_WORKTREE" ]]; then
        warn "Canonical Tempyr worktree missing at $JAM_CANONICAL_TEMPYR_WORKTREE"
        return
    fi
    if sudo -u "$MAESTRO_USER" -H git -C "$JAM_CANONICAL_TEMPYR_WORKTREE" status --short >/dev/null 2>&1; then
        lock_canonical_tempyr_worktree
        pass "Canonical Tempyr worktree is registered"
        return
    fi

    local backup
    backup="${JAM_CANONICAL_TEMPYR_WORKTREE}.broken-$(date -u +%Y%m%dT%H%M%SZ)"
    warn "Canonical Tempyr worktree is not registered; preserving current tree at $backup"
    run_cmd mv "$JAM_CANONICAL_TEMPYR_WORKTREE" "$backup"
    run_as "$BUILD_USER" <<EOF
set -euo pipefail
git -C "$JAM_BLUEBERRY_REPO" worktree add "$JAM_CANONICAL_TEMPYR_WORKTREE" "$JAM_CANONICAL_TEMPYR_BRANCH"
EOF
    if [[ -d "$backup/graph/tasks" ]]; then
        run_cmd mkdir -p "$JAM_CANONICAL_TEMPYR_WORKTREE/graph/tasks"
        if [[ $DRY_RUN -eq 1 ]]; then
            run_cmd find "$backup/graph/tasks" -maxdepth 1 -type f -name '*.md' -exec cp --update=none {} "$JAM_CANONICAL_TEMPYR_WORKTREE/graph/tasks/" \;
        else
            find "$backup/graph/tasks" -maxdepth 1 -type f -name '*.md' \
                -exec cp --update=none {} "$JAM_CANONICAL_TEMPYR_WORKTREE/graph/tasks/" \;
        fi
    fi
    prepare_group_access_tree "$JAM_CANONICAL_TEMPYR_WORKTREE" "$MAESTRO_USER" "canonical Tempyr worktree"
    ensure_safe_directory "$MAESTRO_USER" "$JAM_CANONICAL_TEMPYR_WORKTREE"
    lock_canonical_tempyr_worktree
    pass "Canonical Tempyr worktree re-registered from $JAM_CANONICAL_TEMPYR_BRANCH"
}

lock_canonical_tempyr_worktree() {
    local git_dir
    git_dir="$(sudo -u "$MAESTRO_USER" -H git -C "$JAM_CANONICAL_TEMPYR_WORKTREE" rev-parse --git-dir 2>/dev/null || true)"
    if [[ -z "$git_dir" ]]; then
        warn "Could not resolve canonical Tempyr worktree gitdir for lock"
        return
    fi
    if [[ -f "$git_dir/locked" ]]; then
        pass "Canonical Tempyr worktree already locked against git worktree prune"
    else
        run_as "$BUILD_USER" <<EOF
set -euo pipefail
git -C "$JAM_BLUEBERRY_REPO" worktree lock "$JAM_CANONICAL_TEMPYR_WORKTREE" --reason "Jamboree canonical Tempyr worktree"
EOF
        pass "Canonical Tempyr worktree locked against git worktree prune"
    fi
    run_cmd chmod g+rw "$git_dir/locked" 2>/dev/null || true
}

prepare_runtime_access() {
    if ! id "$MAESTRO_USER" >/dev/null 2>&1; then
        die "maestro user does not exist: $MAESTRO_USER" \
"    Run sudo ./scripts/bootstrap-users.sh first."
    fi
    if ! id "$PICKER_USER" >/dev/null 2>&1; then
        die "picker user does not exist: $PICKER_USER" \
"    Run sudo ./scripts/bootstrap-users.sh first."
    fi

    if id -nG "$MAESTRO_USER" | grep -qw "$PICKER_USER"; then
        pass "$MAESTRO_USER already in $PICKER_USER group"
    else
        info "Adding $MAESTRO_USER to $PICKER_USER group for /home/picker traversal"
        run_cmd usermod -aG "$PICKER_USER" "$MAESTRO_USER"
    fi

    if id -nG "$PICKER_USER" | grep -qw "$MAESTRO_USER"; then
        pass "$PICKER_USER already in $MAESTRO_USER group"
    else
        info "Adding $PICKER_USER to $MAESTRO_USER group for shared worktree writes"
        run_cmd usermod -aG "$MAESTRO_USER" "$PICKER_USER"
    fi

    run_cmd install -d -o "$PICKER_USER" -g "$MAESTRO_USER" -m 2770 "$JAM_WORKTREE_ROOT"
    pass "Picker worktree root prepared at $JAM_WORKTREE_ROOT (2770 $PICKER_USER:$MAESTRO_USER)"

    if [[ -d "$JAM_BLUEBERRY_REPO/.git" ]]; then
        prepare_group_access_tree "$JAM_BLUEBERRY_REPO/.git" "$MAESTRO_USER" "Blueberry git metadata"
        run_as "$BUILD_USER" <<EOF
set -euo pipefail
git -C "$JAM_BLUEBERRY_REPO" config core.sharedRepository group
EOF
        ensure_safe_directory "$MAESTRO_USER" "$JAM_BLUEBERRY_REPO"
        ensure_safe_directory "$PICKER_USER" "$JAM_BLUEBERRY_REPO"
        pass "Blueberry git metadata writable by $MAESTRO_USER group"
    else
        warn "Blueberry git metadata not found at $JAM_BLUEBERRY_REPO/.git; skipping git permission repair"
    fi

    if [[ -d "$JAM_CANONICAL_TEMPYR_WORKTREE" ]]; then
        prepare_group_access_tree "$JAM_CANONICAL_TEMPYR_WORKTREE" "$MAESTRO_USER" "canonical Tempyr worktree"
        ensure_safe_directory "$MAESTRO_USER" "$JAM_CANONICAL_TEMPYR_WORKTREE"
        pass "Canonical Tempyr worktree writable by $MAESTRO_USER group"
    else
        warn "Canonical Tempyr worktree missing at $JAM_CANONICAL_TEMPYR_WORKTREE"
    fi

    ensure_safe_directory "$PICKER_USER" "$JAM_WORKTREE_ROOT/*"
    install_github_app_git_helper
}

detect_uv_bin() {
    if [[ -n "$UV_BIN" && -x "$UV_BIN" ]]; then
        return
    fi
    if command -v uv >/dev/null 2>&1; then
        UV_BIN="$(command -v uv)"
        return
    fi
    if [[ -x "/home/$BUILD_USER/.local/bin/uv" ]]; then
        UV_BIN="/home/$BUILD_USER/.local/bin/uv"
        return
    fi
    UV_BIN=""
}

# ---------------------------------------------------------------------------
# nats-server install
# ---------------------------------------------------------------------------

install_nats() {
    local installed_version=""
    if [[ -x "$INSTALL_DIR/nats-server" ]]; then
        installed_version="$(nats_installed_version || true)"
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

    local nats_arch
    case "$ARCH" in
        linux_amd64) nats_arch="linux-amd64" ;;
        linux_arm64) nats_arch="linux-arm64" ;;
        *) nats_arch="${ARCH//_/-}" ;;
    esac
    local archive="nats-server-${NATS_VERSION}-${nats_arch}.tar.gz"
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
        installed_version="$(process_compose_installed_version || true)"
    fi

    if [[ "$installed_version" == "${PROCESS_COMPOSE_VERSION#v}" ]]; then
        pass "process-compose $PROCESS_COMPOSE_VERSION already installed"
        return
    fi

    if [[ -n "$installed_version" ]]; then
        info "process-compose $installed_version installed; upgrading to $PROCESS_COMPOSE_VERSION"
    else
        info "Installing process-compose $PROCESS_COMPOSE_VERSION"
    fi

    # process-compose ships as a single binary inside a tar.gz archive.
    # Naming convention as of v1.x: process-compose_linux_amd64.tar.gz
    local pc_arch
    case "$ARCH" in
        linux_amd64) pc_arch="linux_amd64" ;;
        linux_arm64) pc_arch="linux_arm64" ;;
        *) pc_arch="$ARCH" ;;
    esac
    local archive="process-compose_${pc_arch}.tar.gz"
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
# First-party runtime binaries
# ---------------------------------------------------------------------------

build_first_party_binaries() {
    info "Building enabled first-party runtime binaries as $BUILD_USER"
    local packages=()
    for package in "${FIRST_PARTY_PACKAGES[@]}"; do
        packages+=("-p" "$package")
    done

    run_as "$BUILD_USER" <<EOF
set -euo pipefail
cd "$REPO_ROOT"
cargo build --release ${packages[*]}
EOF
}

install_first_party_binaries() {
    build_first_party_binaries
    run_cmd mkdir -p "$INSTALL_DIR"

    local bin src dest
    for bin in "${FIRST_PARTY_BINS[@]}"; do
        src="$REPO_ROOT/target/release/$bin"
        dest="$INSTALL_DIR/$bin"
        if [[ $DRY_RUN -eq 0 && ! -x "$src" ]]; then
            die "Built binary missing or non-executable: $src" \
"    Re-run sudo ./scripts/install-substrate.sh after fixing the Rust build."
        fi
        run_cmd install -m 755 "$src" "$dest"
        pass "$bin installed at $dest"
    done
}

# ---------------------------------------------------------------------------
# UI static bundle
# ---------------------------------------------------------------------------

build_ui_bundle() {
    info "Building SolidJS UI bundle as $BUILD_USER"
    run_as "$BUILD_USER" <<EOF
set -euo pipefail
cd "$UI_SRC_DIR"
npm ci
npm run build
EOF
}

install_ui_bundle() {
    build_ui_bundle
    if [[ $DRY_RUN -eq 0 && ! -f "$UI_SRC_DIR/dist/index.html" ]]; then
        die "Built UI bundle missing: $UI_SRC_DIR/dist/index.html" \
"    Re-run sudo ./scripts/install-substrate.sh after fixing the UI build."
    fi

    local ui_root
    ui_root="$(dirname "$UI_DIST_DIR")"

    if [[ $DRY_RUN -eq 1 ]]; then
        run_cmd install -d -m 755 "$ui_root"
        run_cmd rm -rf "$UI_DIST_DIR"
        run_cmd mkdir -p "$UI_DIST_DIR"
        run_cmd cp -a "$UI_SRC_DIR/dist/." "$UI_DIST_DIR/"
        return
    fi

    install -d -m 755 "$ui_root"
    rm -rf "$UI_DIST_DIR"
    install -d -m 755 "$UI_DIST_DIR"
    cp -a "$UI_SRC_DIR/dist/." "$UI_DIST_DIR/"
    if id maestro >/dev/null 2>&1; then
        chown maestro:maestro "$ui_root"
        chown -R maestro:maestro "$UI_DIST_DIR"
    fi
    find "$UI_DIST_DIR" -type d -exec chmod 755 {} +
    find "$UI_DIST_DIR" -type f -exec chmod 644 {} +
    pass "UI static bundle installed at $UI_DIST_DIR"
}

# ---------------------------------------------------------------------------
# Maestro Python app
# ---------------------------------------------------------------------------

install_maestro_app() {
    info "Installing Maestro Python app at $MAESTRO_INSTALL_DIR"
    if [[ $DRY_RUN -eq 1 ]]; then
        run_cmd rm -rf "$MAESTRO_INSTALL_DIR"
        run_cmd install -d -m 755 "$MAESTRO_INSTALL_DIR"
        run_cmd cp -a "$MAESTRO_SRC_DIR/pyproject.toml" "$MAESTRO_SRC_DIR/uv.lock" "$MAESTRO_SRC_DIR/src" "$MAESTRO_INSTALL_DIR/"
        run_cmd chown -R maestro:maestro "$MAESTRO_INSTALL_DIR"
        if [[ -n "$UV_BIN" ]]; then
            run_cmd sudo -u maestro -H "$UV_BIN" venv "$MAESTRO_INSTALL_DIR/.venv"
            run_cmd sudo -u maestro -H "$UV_BIN" pip install --python "$MAESTRO_INSTALL_DIR/.venv/bin/python" "$MAESTRO_INSTALL_DIR"
        else
            run_cmd sudo -u maestro -H python3 -m venv "$MAESTRO_INSTALL_DIR/.venv"
            run_cmd sudo -u maestro -H "$MAESTRO_INSTALL_DIR/.venv/bin/python" -m pip install "$MAESTRO_INSTALL_DIR"
        fi
        return
    fi

    if ! id maestro >/dev/null 2>&1; then
        die "maestro user does not exist" \
"    Run sudo ./scripts/bootstrap-users.sh first."
    fi
    if [[ ! -f "$MAESTRO_SRC_DIR/pyproject.toml" || ! -d "$MAESTRO_SRC_DIR/src" ]]; then
        die "Maestro source tree missing at $MAESTRO_SRC_DIR" \
"    Run from the Jamboree monorepo checkout."
    fi

    rm -rf "$MAESTRO_INSTALL_DIR"
    install -d -o maestro -g maestro -m 755 "$MAESTRO_INSTALL_DIR"
    cp -a "$MAESTRO_SRC_DIR/pyproject.toml" "$MAESTRO_SRC_DIR/uv.lock" "$MAESTRO_SRC_DIR/src" "$MAESTRO_INSTALL_DIR/"
    chown -R maestro:maestro "$MAESTRO_INSTALL_DIR"

    detect_uv_bin
    if [[ -n "$UV_BIN" ]]; then
        sudo -u maestro -H "$UV_BIN" venv "$MAESTRO_INSTALL_DIR/.venv"
        sudo -u maestro -H "$UV_BIN" pip install --python "$MAESTRO_INSTALL_DIR/.venv/bin/python" "$MAESTRO_INSTALL_DIR"
    else
        sudo -u maestro -H python3 -m venv "$MAESTRO_INSTALL_DIR/.venv"
        sudo -u maestro -H "$MAESTRO_INSTALL_DIR/.venv/bin/python" -m pip install --upgrade pip
        sudo -u maestro -H "$MAESTRO_INSTALL_DIR/.venv/bin/python" -m pip install "$MAESTRO_INSTALL_DIR"
    fi
    pass "Maestro Python app installed at $MAESTRO_INSTALL_DIR"
}

# ---------------------------------------------------------------------------
# Verification
# ---------------------------------------------------------------------------

executable_exists() {
    local bin="$1"
    if [[ -x "$INSTALL_DIR/$bin" ]]; then
        pass "$INSTALL_DIR/$bin executable"
        return 0
    fi
    fail "$INSTALL_DIR/$bin missing or non-executable"
    return 1
}

nats_installed_version() {
    "$INSTALL_DIR/nats-server" --version 2>&1 \
      | awk '{print $NF; exit}' \
      | sed 's/^v//'
}

process_compose_installed_version() {
    "$INSTALL_DIR/process-compose" version 2>&1 \
      | awk '/^Version:/ {print $2; exit}' \
      | sed 's/^v//'
}

verify_version() {
    local label="$1"
    local actual="$2"
    local expected="$3"
    if [[ "$actual" == "$expected" ]]; then
        pass "$label $actual matches pinned version"
        return 0
    fi
    fail "$label version $actual installed, expected $expected"
    return 1
}

verify_install() {
    header "Verification"
    local failed=0

    if executable_exists nats-server; then
        local nats_version
        nats_version="$(nats_installed_version || true)"
        if [[ -z "$nats_version" ]]; then
            fail "nats-server version output is empty"
            failed=1
        else
            verify_version nats-server "$nats_version" "${NATS_VERSION#v}" || failed=1
        fi
    else
        failed=1
    fi

    if executable_exists process-compose; then
        local process_compose_version
        process_compose_version="$(process_compose_installed_version || true)"
        if [[ -z "$process_compose_version" ]]; then
            fail "process-compose version output is empty"
            failed=1
        else
            verify_version process-compose "$process_compose_version" "${PROCESS_COMPOSE_VERSION#v}" || failed=1
        fi
    else
        failed=1
    fi

    for bin in "${FIRST_PARTY_BINS[@]}"; do
        executable_exists "$bin" || failed=1
    done

    if [[ -x "$MAESTRO_INSTALL_DIR/.venv/bin/python" ]]; then
        if sudo -n -u maestro -H "$MAESTRO_INSTALL_DIR/.venv/bin/python" -m jam_maestro --help >/dev/null 2>&1; then
            pass "Maestro Python app runnable from $MAESTRO_INSTALL_DIR"
        else
            fail "Maestro Python app installed but not runnable"
            failed=1
        fi
    else
        fail "$MAESTRO_INSTALL_DIR/.venv/bin/python missing or non-executable"
        failed=1
    fi

    if [[ -f "$UI_DIST_DIR/index.html" ]]; then
        pass "$UI_DIST_DIR/index.html present"
    else
        fail "$UI_DIST_DIR/index.html missing"
        failed=1
    fi

    local ui_root
    ui_root="$(dirname "$UI_DIST_DIR")"
    if [[ -d "$ui_root" ]]; then
        pass "$ui_root present"
        if id maestro >/dev/null 2>&1; then
            if sudo -n -u maestro -H test -w "$ui_root" >/dev/null 2>&1; then
                pass "$ui_root writable by maestro for UI session tokens"
            else
                fail "$ui_root not writable by maestro for UI session tokens"
                failed=1
            fi
        fi
    else
        fail "$ui_root missing"
        failed=1
    fi

    if [[ -d "$JAM_WORKTREE_ROOT" ]]; then
        pass "$JAM_WORKTREE_ROOT present"
        if sudo -n -u maestro -H test -w "$JAM_WORKTREE_ROOT" >/dev/null 2>&1; then
            pass "$JAM_WORKTREE_ROOT writable by maestro for worktree creation"
        else
            fail "$JAM_WORKTREE_ROOT not writable by maestro"
            failed=1
        fi
        if sudo -n -u picker -H test -w "$JAM_WORKTREE_ROOT" >/dev/null 2>&1; then
            pass "$JAM_WORKTREE_ROOT writable by picker for Picker sessions"
        else
            fail "$JAM_WORKTREE_ROOT not writable by picker"
            failed=1
        fi
    else
        fail "$JAM_WORKTREE_ROOT missing"
        failed=1
    fi

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
    check_build_user
    check_build_tools

    if [[ $VERIFY_ONLY -eq 1 ]]; then
        verify_install
        exit $?
    fi

    header "Prepare runtime access"
    prepare_runtime_access

    header "Install nats-server"
    install_nats

    header "Install process-compose"
    install_process_compose

    header "Install first-party runtime binaries"
    install_first_party_binaries

    header "Install UI static bundle"
    install_ui_bundle

    header "Install Maestro Python app"
    install_maestro_app

    header "Verify canonical Tempyr worktree"
    ensure_canonical_tempyr_worktree_registered

    if [[ $DRY_RUN -eq 1 ]]; then
        header "Dry run complete."
        exit 0
    fi

    if ! verify_install; then
        die "Substrate verification failed after install." \
"    Inspect the failed checks above, fix the install issue, then rerun:
        sudo ./scripts/install-substrate.sh"
    fi

    header "Substrate ready."
    cat <<EOF

Next steps:
  1. Verify the layout:  $INSTALL_DIR/nats-server --version
                          $INSTALL_DIR/process-compose version
                          $INSTALL_DIR/jam doctor
  2. Start the substrate (after Phase 0 binaries land):
         sudo $INSTALL_DIR/process-compose -U -u /home/maestro/.jam/process-compose.sock up -f /home/caleb/jamboree/process-compose.yaml -D -t=false
EOF
}

main "$@"
