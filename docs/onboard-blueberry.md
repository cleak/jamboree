# Blueberry Manual Onboarding

This is the v1 path for bringing a fresh machine to the point where this
Jamboree instance can orchestrate Blueberry work. It follows
`dec-manual-project-onboarding-v1`, `dec-single-project-per-instance`, and
`dec-blueberry-jam-path`.

Target layout:

```text
/home/caleb/blueberry/       # pristine Blueberry checkout, human-owned
/home/caleb/blueberry-jam/   # canonical tempyr-live worktree, shared caleb:maestro
/home/maestro/.jam/          # runtime substrate state
/home/picker/workers/        # per-task Picker worktrees
```

Blueberry uses `graph/` for Tempyr data. The orchestrator writes only
`/home/caleb/blueberry-jam/graph/tasks/`.

## Prerequisites

- Linux native filesystem only, not `/mnt/c` or another Windows mount.
- `/home/caleb/blueberry/` is already cloned and has `origin/master`.
- This repository is at `/home/caleb/jamboree/`.
- You can use `sudo` for bootstrap and install steps.

## 1. Bootstrap Users And Tools

Run from `/home/caleb/jamboree/`:

```bash
sudo ./scripts/bootstrap-users.sh --user caleb
sudo ./scripts/install-cli-tools.sh
sudo ./scripts/seed-maestro-secrets.sh
```

Then initialize `pass`/GPG for `maestro` if the seed script reports it is not
ready. The manual GPG step is documented in `docs/security-setup.md` section 5.

Expected users:

```text
caleb    UID 1000
maestro  UID 2000
picker   UID 2001
```

## 2. Install Substrate Binaries

```bash
sudo ./scripts/install-substrate.sh
```

Verify:

```bash
/opt/jam/bin/nats-server --version
/opt/jam/bin/process-compose version
```

`install-substrate.sh` also builds and installs the first-party binaries needed
by the currently enabled `process-compose.yaml` entries: `jam`,
`jam-nats-bridge`, `jam-svc-message`, `jam-svc-supervise`, and `jam-ui-server`.
It also builds the SolidJS UI and installs the static bundle under
`/home/maestro/.jam/ui/dist`, which `jam-ui-server` refuses to start without.
Keep future services disabled until their binaries and runtime config are
deployed, otherwise `jam doctor` fails before the supervisor start.

## 3. Create Project Config

Create the config directory:

```bash
sudo -u maestro -i mkdir -p /home/maestro/.jam/config/projects
```

Create `/home/maestro/.jam/config/projects/blueberry.toml`:

```toml
name = "blueberry"
repo-path = "/home/caleb/blueberry"
trunk-branch = "master"
fetch-staleness-secs = 60

canonical-worktree = "/home/caleb/blueberry-jam"
canonical-branch = "tempyr-live"
graph-relpath = "graph"
task-state-relpath = "graph/tasks"
task-state-commit-policy = "never"

worktree-root = "/home/picker/workers"
max-concurrent-pickers = 3

[harnesses.codex-cli]
enabled = true
sandbox-backend = "local"
sandbox-profile = "default"

[mcp-servers]
tempyr = { url = "stdio:tempyr --mcp", enabled = true }
context7 = { url = "https://mcp.context7.com/mcp/v1", enabled = true }
github-mcp = { url = "https://api.githubcopilot.com/mcp/", enabled = true, auth = "github-pat" }
warpgrep = { url = "stdio:warpgrep", enabled = false }
tavily-mcp = { url = "https://mcp.tavily.com/v1", enabled = false }

# Optional Composio Connect sidecar config:
# ~/.jam/config/mcp-composio.toml
#
# endpoint = "https://connect.composio.dev/mcp"
# secret-key = "mcp/composio"
# enabled-toolkits = ["linear", "slack", "notion"]
#
# When present, Maestro expands this into toolkit-specific MCP registry entries
# such as composio-linear and composio-slack.

# Optional quota metadata. Uncomment/fill with current provider pricing and
# budget caps when enabling API-tier Pickers.
#
# [quota.windows."codex-cli/local-messages"]
# reset-cadence-secs = 18000
# next-reset-at = "2026-05-06T15:00:00Z"
#
# [quota.api-budgets."opencode-deepseek/api-budget"]
# provider = "deepseek"
# model = "deepseek-v4-pro"
# monthly-cap-usd = 20.0
# spent-this-month-usd = 0.0
# current-input-rate-per-1m = 0.0
# current-output-rate-per-1m = 0.0
# rate-limit-state = "available"
#
# [[quota.price-events]]
# harness = "opencode-deepseek"
# window-kind = "api-budget"
# name = "provider-price-change"
# provider = "deepseek"
# model = "deepseek-v4-pro"
# ends-at = "2026-05-31T15:59:00Z"
# input-rate-per-1m = 0.0
# output-rate-per-1m = 0.0
```

Keep this file readable by `maestro`:

```bash
sudo chown maestro:maestro /home/maestro/.jam/config/projects/blueberry.toml
sudo chmod 600 /home/maestro/.jam/config/projects/blueberry.toml
```

## 4. Create Harness Lockfile

Get the installed Codex CLI version and checksum for each runtime user after
`install-cli-tools.sh`:

```bash
sudo -u caleb -i codex --version
sudo -u maestro -i codex --version
sudo -u picker -i codex --version
sudo -u picker -i sh -lc 'command -v codex | xargs sha256sum'
```

Create `/home/maestro/.jam/config/projects/blueberry-harnesses.lock`:

```toml
[harnesses.codex-cli]
version = "REPLACE_WITH_CODEX_VERSION"
checksum-sha256 = "REPLACE_WITH_PICKER_CODEX_SHA256"
last-validated = "2026-05-06T00:00:00Z"
validation-tests-passed = ["spawn-dry-run"]

[harnesses.claude-code]
version = "deferred"
checksum-sha256 = "deferred"
last-validated = "deferred"

[harnesses.opencode-deepseek]
version = "deferred"
checksum-sha256 = "deferred"
last-validated = "deferred"
```

Lock down ownership:

```bash
sudo chown maestro:maestro /home/maestro/.jam/config/projects/blueberry-harnesses.lock
sudo chmod 600 /home/maestro/.jam/config/projects/blueberry-harnesses.lock
```

## 5. Create Canonical Tempyr Worktree

The canonical worktree is `/home/caleb/blueberry-jam`, not
`~/code/blueberry-tempyr-live`.

Preferred path after the config files exist:

```bash
jam setup
```

Direct create/recreate path:

```bash
JAM_PROJECT_REPO=/home/caleb/blueberry \
JAM_CANONICAL_TEMPYR_WORKTREE=/home/caleb/blueberry-jam \
JAM_TEMPYR_BASE_REF=origin/master \
JAM_GRAPH_RELPATH=graph \
jam tempyr canonical-worktree recreate
```

Manual fallback:

```bash
git -C /home/caleb/blueberry worktree add /home/caleb/blueberry-jam tempyr-live
sudo chown -R caleb:maestro /home/caleb/blueberry-jam
sudo find /home/caleb/blueberry-jam -type d -exec chmod 2770 {} \;
sudo find /home/caleb/blueberry-jam -type f -exec chmod 660 {} \;
mkdir -p /home/caleb/blueberry-jam/graph/tasks
```

Recovery path for corruption:

```bash
JAM_PROJECT_REPO=/home/caleb/blueberry \
JAM_CANONICAL_TEMPYR_WORKTREE=/home/caleb/blueberry-jam \
JAM_TEMPYR_BASE_REF=origin/master \
JAM_GRAPH_RELPATH=graph \
jam tempyr canonical-worktree recreate
```

This removes the worktree with `git worktree remove --force`, recreates it from
`tempyr-live`, clears derived `graph/tasks/`, and replays task lifecycle journal
events.

## 6. Initialize Skills Repo

`bootstrap-users.sh` creates the shared skills location. Verify:

```bash
ls -la /home/caleb/jamboree/skills/Maestro.md
ls -la /home/caleb/jamboree/skills/global.md
```

If using a separate shared skills checkout later, it must be group-readable by
`maestro` and follow `dec-skills-in-monorepo-v1`.

## 7. Start Substrate

```bash
sudo /opt/jam/bin/process-compose \
  -U -u /home/maestro/.jam/process-compose.sock \
  up \
  -f /home/caleb/jamboree/process-compose.yaml \
  -D -t=false
```

For a foreground smoke with only NATS and the journal bridge, use a separate
terminal and stop it with `Ctrl-C` after verification.

## 8. Verify

Run:

```bash
jam doctor
```

Minimum expected checks for onboarding:

```bash
test -d /home/caleb/blueberry
test -e /home/caleb/blueberry-jam/.git
test -d /home/caleb/blueberry-jam/graph/tasks
sudo -u maestro test -r /home/caleb/blueberry-jam/graph/tasks
sudo -u picker test -d /home/picker/workers
```

Local deterministic acceptance sweep:

```bash
scripts/smoke-local-acceptance.sh --core
```

This runs the rootless substrate installer verifier, the maestro-runtime
NATS-to-JSONL smoke on an isolated port, and deterministic message, research,
search, external-audit evidence-schema, and evolve-coordinator service smokes.
It deliberately excludes the root `/opt/jam/bin` install, production
`--existing` substrate check, real provider quota burns, real GitHub App flows,
and phone/Tailscale checks. The evidence-schema smoke uses temporary fake
evidence only to prove the audit parser's positive path; it is not production
acceptance evidence. The script retains logs under
`/tmp/jam-local-acceptance.*` and writes `summary.json` plus per-smoke
`summary.jsonl` entries in that directory.

Isolated NATS to JSONL smoke:

```bash
scripts/smoke-substrate-journal.sh
```

Maestro runtime NATS to JSONL smoke without `/opt/jam/bin` install:

```bash
scripts/smoke-substrate-journal.sh --maestro-runtime
```

Rootless substrate installer verification smoke:

```bash
scripts/smoke-install-substrate.sh
```

Production NATS to JSONL smoke after the substrate is running:

```bash
TRACE_ID=01ARZ3NDEKTSV4RRFFQ69G5FAV \
  scripts/smoke-substrate-journal.sh --existing
```

External acceptance audit:

```bash
scripts/audit-external-acceptance.sh
scripts/smoke-external-audit-evidence.sh
```

This rootless audit checks the production `/opt/jam/bin` install, production
NATS reachability plus `--existing` journal smoke, GitHub App pass keys,
Tailscale presence, and `jam doctor`. It exits nonzero until the interactive
root install, GitHub App installation ID, production substrate, and manual
deployment checks are finished. It retains `summary.json` and per-check
`summary.jsonl` under `/tmp/jam-external-acceptance.*`; `summary.json` also
embeds `failed_checks` and `manual_followup_checks` so the retained artifact is
usable without terminal scrollback. It also embeds grouped `remediations` with
commands, evidence files, and doc anchors derived from failed and manual
checks. Evidence validation failures include the specific missing field or
semantic rule that failed. For `jam doctor`, the audit uses the installed
`/opt/jam/bin/jam` when present and labels the `target/debug/jam` fallback
explicitly before the production install exists.

The evidence smoke creates temporary readable evidence files, runs the same
audit with `ACCEPTANCE_EVIDENCE_DIR` pointed at them, and asserts the five
manual evidence rows pass. It still allows production/root/provider rows to
fail and removes the fake evidence afterward.

For the root install, production substrate, GitHub App installation, phone,
provider, and seven-day soak handoff, use
`docs/runbooks/external-acceptance.md`.

Manual deployment evidence lives under `/home/maestro/.jam/acceptance/` as
valid JSON files with `verified_at`, `verifier`, and an object-valued
`evidence` field:

```bash
sudo -u maestro -H mkdir -p /home/maestro/.jam/acceptance
```

```text
tailscale-phone-ui.json       # evidence keys: ui_url, auth_check, websocket
ntfy-phone-delivery.json      # evidence keys: trace_id, phone_received_at, urgency
provider-quota-window.json    # evidence keys: trace_id, harnesses, quota_before, quota_after
github-app-live-pr.json       # evidence keys: installation_id, pr_ref, push_verified, comment_api_verified
seven-day-stability.json      # evidence keys: started_at, finished_at, completed_tasks, downtime_minutes
```

The audit also checks semantics: `auth_check`, `websocket`, `push_verified`,
and `comment_api_verified` must be positive; ntfy urgency must be `high` or
`critical`; `harnesses` must be non-empty; and the soak evidence must cover at
least 7 days, at least 50 completed tasks, and less than 5 minutes downtime.

Minimal shape:

```json
{
  "verified_at": "2026-05-06T00:00:00Z",
  "verifier": "human:caleb",
  "evidence": {
    "summary": "what was verified",
    "links_or_paths": []
  }
}
```

The installer smoke stages the pinned `nats-server` / `process-compose`
binaries, release-built first-party runtime binaries, and the built UI bundle
into temporary runtime dirs, then runs `install-substrate.sh --verify-only`
without touching `/opt/jam/bin` or `/home/maestro/.jam`.

The isolated smoke verifies `journal` and `KV_routing-manifest` streams,
publishes `journal.test`, and checks `journal.test.jsonl` in a temporary
`JAM_HOME`. The maestro-runtime smoke starts cached `nats-server` plus
`target/debug/jam-nats-bridge` as `maestro`, writes only the journal entry to
`/home/maestro/.jam`, and cleans up its temporary NATS store. The production
smoke performs the same publish against an already-running substrate and
verifies the line in `/home/maestro/.jam/journal/<date>/journal.test.jsonl`.

Use the heavier deterministic sweep when changing sandbox, patch, or evolution
plumbing. Use `--heavy-only` after a green core run when you do not need to
repeat the core smokes:

```bash
scripts/smoke-local-acceptance.sh --all
scripts/smoke-local-acceptance.sh --heavy-only
```

## Notes

- Do not add a second project to this instance. Per
  `dec-single-project-per-instance`, use a second Jamboree instance for a second
  project.
- Do not let the orchestrator write to `/home/caleb/blueberry/`.
- If `sudo -n -u maestro ...` fails, fix `/etc/sudoers.d/jam-users` before
  testing real Picker spawns.
- A fresh onboarding should fit in 30 minutes when package downloads are already
  available or the network is healthy.
