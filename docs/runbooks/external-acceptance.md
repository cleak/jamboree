# External Acceptance Handoff

**Status:** Phase 9 external acceptance runbook
**Updated:** 2026-05-07

Use this after `scripts/smoke-local-acceptance.sh --core` and `--heavy-only`
are green. The local smokes prove deterministic behavior; this runbook covers
the root-owned install, production substrate, paid/provider checks, phone
delivery, GitHub App installation, and seven-day soak that cannot be proven by
the rootless suite.

For the current objective-level completion snapshot, see
`docs/runbooks/completion-audit.md`.

The audit of record is:

```bash
scripts/audit-external-acceptance.sh
```

It retains `summary.json` and `summary.jsonl` under
`/tmp/jam-external-acceptance.*`. `summary.json` embeds `failed_checks` and
`manual_followup_checks`; use those arrays as the handoff checklist. It also
embeds grouped `remediations` with commands, evidence files, and doc anchors
derived from failed and manual checks. Evidence validation failures include the
specific missing field or semantic rule that failed. The audit uses
`/opt/jam/bin/jam doctor` after the production CLI is installed.

## Current Blocker Shape

The current blocker classes are:

- production binaries and UI bundle missing from `/opt/jam/bin` and
  `/home/maestro/.jam/ui/dist`
- production NATS unreachable at `nats://127.0.0.1:4222`
- missing `jam/pickers/github-app-installation-id`
- Tailscale host/phone UI verification missing
- ntfy phone delivery evidence missing
- real provider/quota window evidence missing
- GitHub App live PR push/comment evidence missing
- seven-day stability evidence missing

Commands requiring root must be run from Caleb's interactive shell or an
existing root shell. The noninteractive agent shell cannot satisfy a sudo
password prompt.

## Install And Start Production Substrate

From `/home/caleb/jamboree`:

```bash
sudo ./scripts/install-substrate.sh

sudo /opt/jam/bin/process-compose \
  -U -u /home/maestro/.jam/process-compose.sock \
  up \
  -f /home/caleb/jamboree/process-compose.yaml \
  -D -t=false
```

On this workstation, if root sudo is not available but `caleb -> maestro`
NOPASSWD sudo is working, the currently enabled all-`maestro` service set can
also be launched without root:

```bash
sudo -u maestro -H /opt/jam/bin/process-compose \
  -U -u /home/maestro/.jam/process-compose.sock \
  up \
  -f /home/caleb/jamboree/process-compose.yaml \
  -D -t=false
```

Verify:

```bash
/opt/jam/bin/nats-server --version
/opt/jam/bin/process-compose version
scripts/smoke-substrate-journal.sh --existing
/opt/jam/bin/jam doctor
```

Expected: the substrate install checks pass, production NATS is reachable, the
`--existing` journal smoke writes `journal.test`, and `jam doctor` no longer
reports the substrate failures.

## Seed GitHub App Installation

Install the Jamboree GitHub App on Blueberry, then seed the installation ID:

```bash
sudo -u maestro -H pass insert jam/pickers/github-app-installation-id
/opt/jam/bin/jam doctor
```

Expected: `github-app-key-valid` exchanges the App key for an installation
token. Then verify a token-backed push and PR comment API call against a
Blueberry test PR before writing `github-app-live-pr.json`.

## Write Manual Evidence

Create the acceptance evidence directory:

```bash
sudo -u maestro -H mkdir -p /home/maestro/.jam/acceptance
```

Every file must be readable by `maestro`, be valid JSON, and include
`verified_at`, `verifier`, and an object-valued `evidence` field. Replace all
sample values with real acceptance evidence; do not copy these examples as
proof.

### Tailscale Phone UI

File: `/home/maestro/.jam/acceptance/tailscale-phone-ui.json`

```json
{
  "verified_at": "2026-05-07T00:00:00Z",
  "verifier": "human:caleb",
  "evidence": {
    "ui_url": "http://100.64.0.10:8787/",
    "auth_check": true,
    "websocket": true
  }
}
```

Verification source: `docs/runbooks/mobile-tailscale-ui.md`.

### ntfy Phone Delivery

File: `/home/maestro/.jam/acceptance/ntfy-phone-delivery.json`

```json
{
  "verified_at": "2026-05-07T00:00:00Z",
  "verifier": "human:caleb",
  "evidence": {
    "trace_id": "01REALTRACEID000000000000000",
    "phone_received_at": "2026-05-07T00:00:00Z",
    "urgency": "critical"
  }
}
```

Expected: a traced `notify-human` request reaches both the phone and the UI
notification drawer.

### Real Provider And Quota Window

File: `/home/maestro/.jam/acceptance/provider-quota-window.json`

```json
{
  "verified_at": "2026-05-07T00:00:00Z",
  "verifier": "human:caleb",
  "evidence": {
    "trace_id": "01REALPROVIDERTRACE0000000000",
    "harnesses": ["codex-cli", "claude-code", "opencode-deepseek"],
    "quota_before": {
      "codex-cli": {"local_messages": "available"},
      "claude-code": {"rate_limit": "available"},
      "opencode-deepseek": {"api_budget_remaining_usd": 10.0}
    },
    "quota_after": {
      "codex-cli": {"local_messages": "exhausted-or-lower"},
      "claude-code": {"rate_limit": "observed"},
      "opencode-deepseek": {"api_budget_remaining_usd": 9.5}
    }
  }
}
```

Expected: real provider work runs during an approved quota window and the
Maestro records quota state changes through the normal journal path.

### GitHub App Live PR

File: `/home/maestro/.jam/acceptance/github-app-live-pr.json`

```json
{
  "verified_at": "2026-05-07T00:00:00Z",
  "verifier": "human:caleb",
  "evidence": {
    "installation_id": 123456,
    "pr_ref": "caleb/blueberry#42",
    "push_verified": true,
    "comment_api_verified": true
  }
}
```

Expected: the GitHub App installation token, not a personal token, performs the
push and PR comment operations.

### Seven-Day Stability

File: `/home/maestro/.jam/acceptance/seven-day-stability.json`

```json
{
  "verified_at": "2026-05-14T00:00:00Z",
  "verifier": "human:caleb",
  "evidence": {
    "started_at": "2026-05-07T00:00:00Z",
    "finished_at": "2026-05-14T00:00:00Z",
    "completed_tasks": 50,
    "downtime_minutes": 0
  }
}
```

Expected: at least seven days elapsed, at least 50 tasks completed, and less
than five minutes total downtime.

## Final Acceptance

Rerun:

```bash
scripts/audit-external-acceptance.sh
/opt/jam/bin/jam doctor
tempyr validate
```

Acceptance is complete only when the audit reports zero failures and zero manual
follow-ups, `jam doctor` is clean, and the evidence files point to real
production/provider/phone/GitHub/soak results.
