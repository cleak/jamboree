#!/usr/bin/env bash
# cleanup-task-worktrees.sh - remove abandoned per-task picker worktrees.
#
# Picker worktrees live at /home/picker/workers/<task-id>/ and have admin
# state under /home/caleb/<repo>/.git/worktrees/<task-id>/. When pickers run
# as root (legacy process-compose launch) those admin dirs are root-owned,
# so `git worktree remove` and `git branch -D` fail for caleb. Need sudo rm.
#
# Usage:
#   sudo ./scripts/cleanup-task-worktrees.sh <repo-path> <task-id> [<task-id> ...]
#   sudo ./scripts/cleanup-task-worktrees.sh --all <repo-path>      # everything matching task/*
#   sudo ./scripts/cleanup-task-worktrees.sh --pattern <repo-path> '<pattern>'
#
# Examples:
#   sudo ./scripts/cleanup-task-worktrees.sh /home/caleb/jamboree t-260516-cqz1zpjk t-260516-bf5apzv5
#   sudo ./scripts/cleanup-task-worktrees.sh --pattern /home/caleb/jamboree 't-260516-*'
#
# Refuses to delete worktrees for branches currently checked out elsewhere.
# Uses the bootstrap-users.sh-style helpers (pass/fail/info/warn/die) for
# consistency with the rest of the operational tooling, including the
# "Fix:" remediation block style from spec §2.12.

set -euo pipefail

WORKTREE_ROOT_DEFAULT="/home/picker/workers"
WORKTREE_ROOT="${JAM_WORKTREE_ROOT:-$WORKTREE_ROOT_DEFAULT}"

# ─── Output helpers (match bootstrap-users.sh and `jam doctor`) ────────

pass() { printf '  \x1b[32m✓\x1b[0m %s\n' "$1"; }
info() { printf '  \x1b[36m∼\x1b[0m %s\n' "$1"; }
warn() { printf '  \x1b[33m!\x1b[0m %s\n' "$1" >&2; }
fail() {
    printf '  \x1b[31m✗\x1b[0m %s\n' "$1" >&2
    if [[ $# -ge 2 ]] && [[ -n "${2:-}" ]]; then
        printf '\n    Fix:\n'
        printf '%s\n' "$2" | sed 's/^/    /' >&2
    fi
}
die() {
    fail "$1" "${2:-}"
    exit "${3:-1}"
}

require_root() {
    if [[ $EUID -ne 0 ]]; then
        die "must run as root (or under sudo) to remove root-owned worktree state" \
"sudo $0 $*" 64
    fi
}

# ─── Task-id validation ─────────────────────────────────────────────────

# Allow only path-safe slug characters. Defense in depth: cleanup_one
# constructs paths from this string and passes them to `rm -rf`, so a
# task_id like `../etc` or `;rm -rf /` must be rejected up front.
# Allowed: lowercase + digits + `.`, `_`, `-`. No slashes, no spaces, no
# parent-directory segments, no shell metacharacters.
validate_task_id() {
    local id="$1"
    if [[ -z "$id" ]]; then
        die "task_id must not be empty" 65
    fi
    if [[ "$id" =~ / ]] || [[ "$id" =~ \.\. ]] || [[ "$id" =~ \  ]]; then
        die "task_id contains forbidden characters: '$id'" \
"Task ids must be slug-like (letters, digits, '.', '_', '-' only). The
caller probably has a bug — verify the source of the id before re-running." 65
    fi
    if [[ ! "$id" =~ ^[A-Za-z0-9._-]+$ ]]; then
        die "task_id '$id' contains characters outside [A-Za-z0-9._-]" \
"Task ids must be slug-like. If this id came from a graph node, it's a
graph data bug — fix the node before re-running." 65
    fi
}

# ─── Per-task cleanup ───────────────────────────────────────────────────

# Always-run branch-in-use guard: probes the porcelain output of
# `git -C <repo> worktree list` for the task's branch regardless of
# whether refs/heads/<branch> is loose or packed. This catches the case
# where a checked-out branch with a packed ref would otherwise slip past
# the guard. The "skip" argument is the picker's filesystem worktree path
# — `git worktree list --porcelain` emits filesystem paths, not
# `.git/worktrees/<name>/` admin dirs, so we have to compare apples to
# apples to allow cleaning the task's own stale worktree.
branch_in_use_elsewhere() {
    local repo="$1"
    local branch="$2"
    local skip_worktree="$3"
    git -C "$repo" worktree list --porcelain 2>/dev/null \
        | awk -v want="refs/heads/$branch" -v skip="$skip_worktree" '
            /^worktree / { wt = $2 }
            /^branch /   { br = $2; if (br == want && wt != skip) found=1 }
            END          { exit (found ? 0 : 1) }
        '
}

cleanup_one() {
    local repo="$1"
    local task_id="$2"
    validate_task_id "$task_id"

    local branch="task/${task_id}"
    local picker_path="${WORKTREE_ROOT}/${task_id}"
    local admin_dir="${repo}/.git/worktrees/${task_id}"
    local ref_file="${repo}/.git/refs/heads/${branch}"

    info "cleanup $task_id"

    # Pass picker_path as "skip" — the porcelain output is filesystem
    # paths, so we have to match against the worktree's filesystem path
    # (not its admin dir) for "the task's own worktree" to compare equal.
    if branch_in_use_elsewhere "$repo" "$branch" "$picker_path"; then
        warn "skipping $task_id: branch $branch is in use by another worktree"
        return 0
    fi

    if [[ -d "$picker_path" ]]; then
        rm -rf -- "$picker_path"
        pass "removed $picker_path"
    fi

    if [[ -d "$admin_dir" ]]; then
        rm -rf -- "$admin_dir"
        pass "removed $admin_dir"
    fi

    if [[ -f "$ref_file" ]]; then
        rm -f -- "$ref_file"
        pass "removed $ref_file"
    fi

    # Remote-tracking refs for the task branch on every configured remote.
    # The PR-open path in `jam-svc-repo` pushes to `origin` and GitHub's
    # auto-merge typically deletes the head branch on merge, but git keeps
    # the local mirror around indefinitely (see `git fetch --prune` for the
    # equivalent automatic cleanup). When pickers ran as root those mirror
    # files end up root-owned, so this script — which we already invoke
    # under sudo — is the right place to scrub them. Skip silently when no
    # mirror file exists; the script stays idempotent.
    local remote remote_ref
    while read -r remote; do
        [[ -z "$remote" ]] && continue
        remote_ref="${repo}/.git/refs/remotes/${remote}/${branch}"
        if [[ -f "$remote_ref" ]]; then
            rm -f -- "$remote_ref"
            pass "removed $remote_ref"
        fi
    done < <(git -C "$repo" remote 2>/dev/null || true)
}

usage() {
    sed -n '2,17p' "$0" >&2
    exit 64
}

main() {
    require_root "$@"
    [[ $# -ge 2 ]] || usage

    case "$1" in
        --all)
            local repo="${2:-}"
            [[ -n "$repo" ]] || die "--all requires a repo path" "usage: see top of script" 64
            [[ -d "$repo/.git" ]] || die "not a git repo: $repo" "Run from a repository root." 65
            local ids
            ids=$(git -C "$repo" for-each-ref --format='%(refname:lstrip=3)' refs/heads/task 2>/dev/null || true)
            if [[ -z "$ids" ]]; then
                info "no task/* branches in $repo"
                return 0
            fi
            while read -r task; do
                [[ -n "$task" ]] && cleanup_one "$repo" "$task"
            done <<<"$ids"
            git -C "$repo" worktree prune || true
            git -C "$repo" gc --auto || true
            ;;
        --pattern)
            local repo="${2:-}"
            local pattern="${3:-}"
            [[ -n "$repo" ]] || die "--pattern requires a repo path" \
"usage: sudo $0 --pattern <repo-path> '<pattern>'" 64
            [[ -n "$pattern" ]] || die "--pattern requires a pattern" \
"usage: sudo $0 --pattern <repo-path> '<pattern>'" 64
            [[ -d "$repo/.git" ]] || die "not a git repo: $repo" "Run from a repository root." 65
            local matched=0
            while read -r task; do
                [[ -z "$task" ]] && continue
                cleanup_one "$repo" "$task"
                matched=$((matched + 1))
            done < <(git -C "$repo" for-each-ref \
                --format='%(refname:lstrip=3)' \
                "refs/heads/task/${pattern}" 2>/dev/null || true)
            if [[ "$matched" -eq 0 ]]; then
                info "no task/$pattern branches matched in $repo"
            fi
            git -C "$repo" worktree prune || true
            ;;
        --*)
            die "unknown flag: $1" "Run with no args to see usage." 64
            ;;
        *)
            local repo="$1"
            shift
            [[ -d "$repo/.git" ]] || die "not a git repo: $repo" "Run from a repository root." 65
            for task in "$@"; do
                cleanup_one "$repo" "$task"
            done
            git -C "$repo" worktree prune || true
            ;;
    esac

    printf '\n\x1b[32mdone\x1b[0m\n'
}

main "$@"
