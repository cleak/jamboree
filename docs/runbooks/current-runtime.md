# Current Runtime State

Last audited: 2026-05-10.

This runbook describes the Jamboree instance currently running on this
workstation. It is intentionally operational: use it before assuming that the
architecture spec, graph backlog, or old smoke-test notes reflect the live
deployment exactly.

## Source And Runtime Paths

- Source checkout: `/home/caleb/jamboree`
- Blueberry source repo: `/home/caleb/blueberry`
- Blueberry Tempyr worktree: `/home/caleb/blueberry-jam`
- Runtime home: `/home/maestro/.jam`
- Picker worktrees: `/home/picker/workers`
- Process-compose socket: `/home/maestro/.jam/process-compose.sock`

Inspect live services:

```bash
sudo -n -u maestro -H /opt/jam/bin/process-compose \
  -U -u /home/maestro/.jam/process-compose.sock \
  list -o wide
```

The active process-compose config is currently an overlay at
`/home/maestro/.jam/process-compose.jamboree-prmeta.yaml`. It was generated
from the repo-root `process-compose.yaml` and points selected services at
patched runtime binaries under `/home/maestro/.jam/bin`.

## Active Service Set

Running substrate services at audit time:

- `nats`
- `jam-nats-bridge`
- `jam-svc-message`
- `jam-svc-observe`
- `jam-svc-repo`
- `jam-svc-session`
- `jam-svc-supervise`
- `jam-svc-worktree`
- `maestro`
- `pr-status-poller`
- `task-lifecycle-handler`
- `ui-server`

Disabled at audit time:

- `clock-watcher`
- `harness-version-watcher`
- `jam-ntfy-bridge`
- `jam-svc-evolve`
- `jam-svc-knowledge`
- `jam-svc-research`
- `jam-svc-search`
- `journal-reconciler`
- `patch-agent`
- `skill-suspicion-reconciler`
- `stall-detector`
- `tempyr-pr-reconciler`
- `tempyr-write-reconciler`
- `trunk-fetcher`

The disabled services are still represented in `process-compose.yaml`, but
should not be assumed live by a new dev or agent.

## Patched Runtime Binaries

The running overlay currently uses these patched binaries:

- `/home/maestro/.jam/bin/jam-svc-session-warn`
- `/home/maestro/.jam/bin/jam-svc-repo-prmeta`
- `/home/maestro/.jam/bin/jam-ui-server-prmeta`
- `/home/maestro/.jam/bin/jam-task-lifecycle-taskfailed`

The UI bundle is deployed at:

- `/home/maestro/.jam/ui/dist`

The Maestro Python package was also patched in place under:

- `/opt/jam/maestro/.venv/lib/python3.12/site-packages/jam_maestro`
- `/opt/jam/maestro/src/jam_maestro`

Cold-start caveat: the source changes have not all been installed back into
the canonical root-owned `/opt/jam/bin/*` production paths. If the machine is
restarted from the original root launch path, reinstall the release binaries or
regenerate the active process-compose overlay before assuming the latest fixes
are present.

Root install commands for persistence:

```bash
sudo install -m 0755 /home/caleb/jamboree/target/release/jam-svc-session /opt/jam/bin/jam-svc-session
sudo install -m 0755 /home/caleb/jamboree/target/release/jam-svc-repo /opt/jam/bin/jam-svc-repo
sudo install -m 0755 /home/caleb/jamboree/target/release/jam-ui-server /opt/jam/bin/jam-ui-server
sudo install -m 0755 /home/caleb/jamboree/target/release/jam-task-lifecycle /opt/jam/bin/jam-task-lifecycle
```

## Harness Drift Policy

The live `jam-svc-session` defaults to warn-and-continue for concrete harness
version or checksum drift:

```yaml
JAM_HARNESS_LOCKFILE_POLICY=warn
```

Supported values:

- `warn`: log the drift, include it in telemetry, and continue spawning.
- `strict`: reject the spawn on `harness-version-drift` or
  `harness-checksum-drift`.
- `off`: skip concrete version/checksum comparison.

Missing lockfiles, malformed lockfiles, missing harness entries, and unsupported
lockfile policy values still fail loudly.

Current Blueberry Codex pin:

```toml
[harnesses.codex-cli]
version = "0.129.0"
checksum-sha256 = "baefc109b871e73a7bab298ee19b8bf73c8b647c4f8649a9794fc5db01db17b9"
last-validated = "2026-05-09T07:13:02Z"
```

Historical note: the task ending in `03er4c` failed before this policy changed.
It has since been backfilled with `journal.task.failed` and appears as failed
in Tempyr with `failure-reason: harness-version-drift`.

## Task Failure Visibility

Failures before Picker spawn are now durable task events. The Maestro publishes:

```text
journal.task.failed
```

The event payload includes:

- `task_id`
- `reason`
- `detail`
- `failed_at`
- `source_event_type`

`task-lifecycle-handler` consumes `task.failed`, marks the Tempyr task
`status: failed`, and records the failure fields in the task frontmatter. The UI
also renders `task.failed` in the task timeline.

This fixes the earlier gap where a Maestro spawn error could appear in process
logs but not in the canonical task status.

## PR Behavior

Picker-created PRs are non-draft by default:

```yaml
JAM_SESSION_OPEN_PR_DRAFT=false
```

The session service instructs Pickers to write:

- `.jam/pr-title.txt`
- `.jam/pr-body.md`

The repo service deterministically applies the `[jam]` title prefix and rejects
titles that are just IDs, branch names, or log spew. The task UI links to the PR
once `pr.opened` or `pr.status-changed` has been observed.

Known recent evidence:

- `2026-05-09-pick-one-small-backlog-item-and-fix-it-or-implem-9d2qxy`
  completed and opened `cleak/blueberry#393`.
- The PR title was `[jam] Require AgentDebugState for controller debug startup`.

## UI Access

The UI server is currently bound for LAN/WSL forwarding:

```yaml
JAM_UI_BIND=0.0.0.0:8787
JAM_UI_ALLOW_BIND_ADDRS=0.0.0.0,127.0.0.1,10.0.0.0/8,172.16.0.0/12,192.168.0.0/16,100.64.0.0/10
```

Create a UI token:

```bash
sudo -n -u maestro -H /opt/jam/bin/jam ui token --user-id human:caleb
```

For remote LAN access from Windows-hosted WSL, Windows still needs a local
portproxy/firewall rule. Keep the firewall rule scoped to the LAN subnet, not
the whole internet.

## Live Run Logs

Picker stdout/stderr is stored as JSONL under:

```text
/home/maestro/.jam/session-logs/<session_id>.jsonl
```

The UI task detail page shows these records in the Run Log panel. The backing
HTTP endpoint is:

```text
GET /api/sessions/{session_id}/output
```

The UI also subscribes to:

```text
picker.<session_id>.output
```

This is the current answer to "Task started" being too opaque during long runs.

## Useful Verification

Rust and UI checks recently run successfully:

```bash
cargo fmt --check
cargo test -p jam-svc-session harness_lockfile
cargo test -p jam-svc-session warn_policy_allows_harness_drift
cargo check -p jam-svc-session
cargo test -p jam-task-lifecycle task_failed_marks_task_failed_before_picker_spawn
cargo check -p jam-cli -p jam-task-lifecycle
npm run build
PYTHONPATH=maestro maestro/.venv/bin/python -m pytest -q maestro/tests/unit/test_session.py
```

Run `tempyr validate` after manual graph edits.

## Known Gaps

- Some graph nodes and spec sections still describe intended future behavior,
  not the exact live deployment.
- The runtime uses patched overlay binaries under `/home/maestro/.jam/bin`; root
  `/opt/jam/bin` persistence still needs a final install.
- `harness-version-watcher` is implemented but disabled in the current process
  set.
- Several reconcilers and optional services remain disabled.
- Historical smoke-test notes may mention draft PRs; current default is
  non-draft PRs.
