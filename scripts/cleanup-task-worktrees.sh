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
#   sudo ./scripts/cleanup-task-worktrees.sh --pattern '<repo-path>' '<pattern>'
#
# Examples:
#   sudo ./scripts/cleanup-task-worktrees.sh /home/caleb/jamboree t-260516-cqz1zpjk t-260516-bf5apzv5
#   sudo ./scripts/cleanup-task-worktrees.sh --pattern /home/caleb/jamboree 't-260516-*'
#
# Refuses to delete worktrees for branches currently checked out elsewhere.

set -euo pipefail

WORKTREE_ROOT_DEFAULT="/home/picker/workers"
WORKTREE_ROOT="${JAM_WORKTREE_ROOT:-$WORKTREE_ROOT_DEFAULT}"

die() {
    printf 'cleanup-task-worktrees: %s\n' "$1" >&2
    exit "${2:-1}"
}

require_root() {
    if [[ $EUID -ne 0 ]]; then
        die "must run as root (or under sudo) to remove root-owned worktree state" 64
    fi
}

cleanup_one() {
    local repo="$1"
    local task_id="$2"
    local branch="task/${task_id}"
    local picker_path="${WORKTREE_ROOT}/${task_id}"
    local admin_dir="${repo}/.git/worktrees/${task_id}"
    local ref_file="${repo}/.git/refs/heads/${branch}"

    printf '\x1b[1mcleanup\x1b[0m %s\n' "$task_id"

    # Refuse if the branch is currently HEAD of some other working tree.
    if [[ -f "$ref_file" ]]; then
        local in_use
        in_use=$(git -C "$repo" worktree list --porcelain 2>/dev/null \
            | awk -v b="refs/heads/$branch" '$1 == "branch" && $2 == b {flag=1} END {print flag+0}')
        if [[ "$in_use" == "1" ]] && [[ ! -d "$admin_dir" ]]; then
            # Branch is HEAD of a worktree other than this task's — skip.
            printf '  \x1b[33m!\x1b[0m skipping %s: branch %s is in use by another worktree\n' \
                "$task_id" "$branch"
            return 0
        fi
    fi

    # 1. Filesystem: picker's checkout. Owned by root or picker; sudo rm.
    if [[ -d "$picker_path" ]]; then
        rm -rf "$picker_path"
        printf '  \x1b[32m-\x1b[0m removed %s\n' "$picker_path"
    fi

    # 2. Filesystem: caleb's .git/worktrees admin dir. Root-owned post-debug.
    if [[ -d "$admin_dir" ]]; then
        rm -rf "$admin_dir"
        printf '  \x1b[32m-\x1b[0m removed %s\n' "$admin_dir"
    fi

    # 3. Filesystem: ref file. Tidies up branch metadata; safer than
    #    `git update-ref -d` because that takes ref-lock the user may not
    #    have permission for.
    if [[ -f "$ref_file" ]]; then
        rm -f "$ref_file"
        printf '  \x1b[32m-\x1b[0m removed %s\n' "$ref_file"
    fi
}

usage() {
    sed -n '2,17p' "$0" >&2
    exit 64
}

main() {
    require_root
    [[ $# -ge 2 ]] || usage

    case "$1" in
        --all)
            local repo="$2"
            [[ -d "$repo/.git" ]] || die "not a git repo: $repo" 65
            local ids
            ids=$(git -C "$repo" for-each-ref --format='%(refname:lstrip=2)' refs/heads/task 2>/dev/null \
                | sed 's@^@@' || true)
            if [[ -z "$ids" ]]; then
                printf 'no task/* branches in %s\n' "$repo"
                return 0
            fi
            while read -r branch; do
                local task="${branch#task/}"
                cleanup_one "$repo" "$task"
            done <<<"$ids"
            git -C "$repo" worktree prune || true
            git -C "$repo" gc --auto || true
            ;;
        --pattern)
            local repo="$2"
            local pattern="$3"
            [[ -d "$repo/.git" ]] || die "not a git repo: $repo" 65
            local matched=0
            for branch in $(git -C "$repo" for-each-ref --format='%(refname:lstrip=3)' "refs/heads/task/${pattern}" 2>/dev/null); do
                cleanup_one "$repo" "$branch"
                matched=$((matched + 1))
            done
            if [[ $matched -eq 0 ]]; then
                printf 'no task/%s branches matched in %s\n' "$pattern" "$repo"
            fi
            git -C "$repo" worktree prune || true
            ;;
        *)
            local repo="$1"
            shift
            [[ -d "$repo/.git" ]] || die "not a git repo: $repo" 65
            for task in "$@"; do
                cleanup_one "$repo" "$task"
            done
            git -C "$repo" worktree prune || true
            ;;
    esac

    printf '\n\x1b[32mdone\x1b[0m\n'
}

main "$@"
