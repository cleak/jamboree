# Jamboree Failure-Mode Runbooks

**Status:** Initial Phase 9 runbook
**Updated:** 2026-05-06

These runbooks cover the named Phase 9 failures from `task-runbooks-failure-modes` (§12):

- NATS data loss
- Canonical Tempyr worktree corruption
- Harness version drift
- All quota exhausted
- Prolonged provider outage

Principles used throughout: fail loudly (`principle-failure-surfaces-immediately`), trace first (`principle-tracing-chains-end-to-end`), no silent state repair, and Blueberry-only (`dec-single-project-per-instance`).

## Common Triage

Run these before choosing a specific recovery path:

```bash
jam doctor
tempyr validate
git -C /home/caleb/jamboree status --short
```

Check recent journal and trace context:

```bash
find /home/maestro/.jam/journal -type f -name '*.jsonl' -mtime -1 | sort
jam trace replay <trace-id>
```

If dispatch is still running and the issue can produce bad work, pause new spawns:

```bash
jam pause-dispatch --reason "operator triage: <short reason>"
```

Resume only after the verification step for the specific runbook passes:

```bash
jam resume-dispatch
```

## NATS Data Loss

### Symptoms

- `jam-nats-bridge` cannot resume a durable consumer.
- `jam doctor` reports missing JetStream streams or KV buckets.
- `world-snapshot` loses recent task/session facts that are present in JSONL.
- `process-compose` shows NATS restarted with an empty store directory.

### Immediate Action

Pause dispatch:

```bash
jam pause-dispatch --reason "NATS data loss suspected"
```

Snapshot the damaged state before changing it:

```bash
sudo tar -C /home/maestro/.jam -czf /home/maestro/.jam/incidents/nats-data-$(date -u +%Y%m%dT%H%M%SZ).tgz nats-data journal config
```

### Recovery

Recreate streams and KV buckets by starting the bridge. For an isolated
verification that does not touch production state, run
`scripts/smoke-substrate-journal.sh`. To verify the runtime journal path and
`maestro` permissions without `/opt/jam/bin`, run
`scripts/smoke-substrate-journal.sh --maestro-runtime`.

```bash
sudo /opt/jam/bin/process-compose \
  -U -u /home/maestro/.jam/process-compose.sock \
  up \
  -f /home/caleb/jamboree/process-compose.yaml \
  nats jam-nats-bridge \
  -t=false
```

If NATS KV `routing-manifest/current` is missing, reapply the last known staged service versions from `/home/maestro/.jam/bin/` using `jam patch apply`. If no safe version is known, leave dispatch paused and escalate to the Manager.

Rebuild derived stores from durable JSONL:

```bash
rm -f /home/maestro/.jam/session-store.db
/opt/jam/bin/jam-journal-reconciler --rebuild
jam tempyr canonical-worktree recreate
```

### Verification

```bash
jam doctor
tempyr validate
jam task list
```

Publish a traced test event and verify it lands in JSONL:

```bash
scripts/smoke-substrate-journal.sh --existing
```

Confirm `journal.test` appears under `/home/maestro/.jam/journal/<date>/`.

## Canonical Tempyr Worktree Corruption

### Symptoms

- `tempyr validate` fails only in the canonical worktree.
- `jam task list` and graph task nodes disagree.
- Files under `~/code/blueberry-tempyr-live/` are missing, unparseable, or owned by the wrong user/group.

### Immediate Action

Pause dispatch if task state is unreliable:

```bash
jam pause-dispatch --reason "canonical Tempyr worktree corruption"
```

Preserve the corrupted worktree for inspection:

```bash
tar -C /home/caleb/code -czf /home/caleb/jamboree/.tempyr/corrupt-worktree-$(date -u +%Y%m%dT%H%M%SZ).tgz blueberry-tempyr-live
```

### Recovery

Use the replay path rather than manual renames:

```bash
jam tempyr canonical-worktree recreate
tempyr validate
```

Fix permissions if needed:

```bash
sudo chown -R caleb:maestro /home/caleb/code/blueberry-tempyr-live
sudo find /home/caleb/code/blueberry-tempyr-live -type d -exec chmod 2770 {} +
sudo find /home/caleb/code/blueberry-tempyr-live -type f -exec chmod 660 {} +
```

### Verification

```bash
tempyr validate
jam task list
jam task show <known-task-id>
```

Use `jam trace replay <trace-id>` on a recent task and verify the reconstructed task state matches the Tempyr node.

## Harness Version Drift

### Symptoms

- Picker spawn may warn before first output, or fail before first output when
  `JAM_HARNESS_LOCKFILE_POLICY=strict`.
- A harness binary reports a version different from the project lockfile.
- `harness.version-changed` appears in the journal.
- New work differs without code or skill changes.

### Immediate Action

Pause new spawns if the drift can affect generated code:

```bash
jam pause-dispatch --reason "harness version drift"
```

Inspect installed versions as each runtime user:

```bash
sudo -u maestro -i codex --version
sudo -u picker -i codex --version
sudo -u maestro -i claude --version
sudo -u picker -i claude --version
```

Check the current lockfile:

```bash
cat /home/maestro/.jam/config/projects/blueberry-harnesses.lock
```

### Recovery

If the upgrade is intentional, update the lockfile through the harness-version
workflow and record the trace in Tempyr. If the upgrade is accidental, reinstall
the pinned version for the affected user and rerun the version check.

Current policy values:

- `warn`: log the drift and continue spawning. This is the current live default.
- `strict`: reject spawns on concrete version/checksum drift.
- `off`: skip concrete version/checksum comparison.

Missing or malformed lockfiles still fail loudly in every policy.

For Codex/Claude installs managed by the CLI installer:

```bash
sudo ./scripts/install-cli-tools.sh --verify-only
sudo ./scripts/install-cli-tools.sh
```

### Verification

```bash
sudo -u picker -i codex --version
sudo -u picker -i claude --version
jam doctor
```

Spawn one dry-run Picker and verify `picker.spawned` records the expected harness version in the journal before resuming dispatch.

## All Quota Exhausted

### Symptoms

- `quota.exhausted` or `quota.exhausted-soon` events for every configured harness.
- Maestro cannot select a safe harness for new work.
- `world-snapshot.harness_quotas` shows no usable provider.

### Immediate Action

Pause dispatch:

```bash
jam pause-dispatch --reason "all quota exhausted"
```

Inspect quota state:

```bash
jam quota show
```

Check recent quota events:

```bash
grep -R '"quota.' /home/maestro/.jam/journal/$(date -u +%F) || true
```

### Recovery

Choose one:

- Wait for the shortest reset window and leave dispatch paused.
- Manually fund or raise the budget for the relevant provider.
- Reduce scope by splitting tasks into smaller Manager-approved requests.
- Temporarily restrict work to non-LLM deterministic reconcilers.

Do not route around the quota tracker manually; that violates `principle-failure-surfaces-immediately`.

### Verification

```bash
jam quota show
jam doctor
```

Resume only after at least one harness has enough quota for the next task class:

```bash
jam resume-dispatch
```

## Prolonged Provider Outage

### Symptoms

- Repeated backend errors from LiteLLM or harness CLIs.
- Provider status is degraded for more than one reset/retry interval.
- Multiple tasks fail before meaningful Picker output.
- Retry attempts produce the same upstream error kind.

### Immediate Action

Pause dispatch if all viable providers are affected:

```bash
jam pause-dispatch --reason "provider outage"
```

Capture evidence:

```bash
jam trace replay <failed-trace-id>
jam quota show
```

Record a Tempyr decision with the outage window, affected provider, and chosen recovery path.

### Recovery

If another configured provider is healthy and the task class allows it, update dispatch policy or task skill guidance so the Maestro can route there. If no provider is healthy, leave dispatch paused and continue deterministic services only.

For authentication-specific failures, re-run provider login under the affected runtime user:

```bash
sudo -u maestro -i codex login --device-auth
sudo -u picker -i codex login --device-auth
sudo -u maestro -i claude
sudo -u picker -i claude
```

### Verification

Run a minimal backend or harness smoke:

```bash
uv run --directory /home/caleb/jamboree/maestro python -m jam_maestro prompt "Reply with: pong"
sudo -u picker -i codex --version
```

Resume dispatch after one low-risk task completes or fails for a non-provider reason:

```bash
jam resume-dispatch
```

## Escalation Template

Use this structure for ntfy or Manager handoff:

```text
incident: <short id>
failure: <NATS data loss | Tempyr corruption | harness drift | all quota exhausted | provider outage>
first_seen_utc: <timestamp>
paused_dispatch: <yes/no>
root_trace: <trace id if known>
evidence:
- <journal file or command output>
- <doctor/tempyr result>
current_state: <what is still broken>
next_safe_action: <one command or human decision needed>
```
