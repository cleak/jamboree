---
id: task-runbooks-failure-modes
type: task
status: done
created: 2026-05-04T04:00:51.433205461Z
updated: 2026-05-07T00:38:24Z
---
Phase 9 (§12). Document runbooks for: NATS data loss, canonical worktree corruption, harness version drift, all-quota-exhausted, prolonged provider outage.

Implementation note (2026-05-06): added `docs/runbooks/failure-modes.md` covering all five named Phase 9 failure modes. Each runbook includes symptoms, immediate action, recovery, verification, and escalation guidance, using current command surfaces such as `jam pause-dispatch`, `jam trace replay`, `jam tempyr canonical-worktree recreate`, `jam health ping`, `jam quota show`, `jam doctor`, and `tempyr validate`. `docs/layout.md` now lists `docs/runbooks/` as the operator recovery-procedure location.

Follow-up note (2026-05-06): the NATS data-loss runbook now uses the dedicated
`scripts/smoke-substrate-journal.sh --maestro-runtime` and `--existing` checks
instead of a task spawn as the journal recovery proof.

Operator handoff note (2026-05-06): `docs/onboard-blueberry.md` now points to
`scripts/smoke-local-acceptance.sh --core` as the deterministic local
acceptance sweep before the externally blocked production checks. `--all`
includes heavier sandbox, atomic-swap, Hermes vendor, cgroup, and patch-agent
smokes.

Reverification note (2026-05-06): both
`scripts/smoke-local-acceptance.sh --core` and `--heavy-only` passed, covering
all current deterministic smoke scripts while still excluding root production
install, real provider quota, real GitHub App, and phone/Tailscale acceptance.

Summary artifact note (2026-05-06): `scripts/smoke-local-acceptance.sh` now
retains a machine-readable `summary.json` and per-smoke `summary.jsonl` next to
the individual logs. A core rerun passed and produced
`/tmp/jam-local-acceptance.5Irw7M/summary.json`; a heavy-only rerun passed and
produced `/tmp/jam-local-acceptance.qczeRk/summary.json`.

External audit note (2026-05-06): `scripts/audit-external-acceptance.sh` now
checks the production-only gates that local smokes deliberately exclude:
`/opt/jam/bin` substrate binaries, production NATS plus `--existing` journal
smoke, GitHub App pass keys, Tailscale presence, and `jam doctor`. The current
run exits nonzero with the known blockers: missing `/opt/jam/bin`, missing
GitHub App installation ID, no production NATS, and deployment/manual
acceptance still outstanding.

External audit summary note (2026-05-06): the audit now writes retained
machine-readable evidence. The current run produced
`/tmp/jam-external-acceptance.SiziMK/summary.json` with `status=failed`,
`failures=11`, and `manual_followups=4`.

Manual evidence note (2026-05-06): the external audit now checks named
deployment evidence files under `/home/maestro/.jam/acceptance/`:
`tailscale-phone-ui.json`, `ntfy-phone-delivery.json`,
`provider-quota-window.json`, `github-app-live-pr.json`, and
`seven-day-stability.json`. The current run produced
`/tmp/jam-external-acceptance.FVxNrV/summary.json` with `status=failed`,
`failures=11`, and `manual_followups=6`.

Strict evidence note (2026-05-06): acceptance evidence files parse as JSON
objects with `verified_at`, `verifier`, and object-valued `evidence`, then
apply acceptance-specific semantics: Tailscale UI needs a URL plus positive
`auth_check` and `websocket`; ntfy needs a trace ID, receipt timestamp, and
`high`/`critical` urgency; provider/quota needs a trace ID, non-empty harness
list, and before/after quota maps; GitHub App live PR needs a positive
installation ID, PR ref, and true push/comment API booleans; and seven-day soak
requires >=7 days, >=50 completed tasks, and <5 downtime minutes. The latest
real audit produced `/tmp/jam-external-acceptance.c2ZIh1/summary.json` with
`status=failed`, `failures=11`, and `manual_followups=6`; a temporary readable
override probe validated the positive schema path and was removed afterward.

Evidence smoke note (2026-05-07): the positive schema path is now repeatable as
`scripts/smoke-external-audit-evidence.sh`. It creates temporary readable fake
evidence, runs `scripts/audit-external-acceptance.sh` with
`ACCEPTANCE_EVIDENCE_DIR` pointed at that directory, asserts all five evidence
rows pass in `summary.jsonl`, and removes the fake evidence afterward. This is
part of `scripts/smoke-local-acceptance.sh --core` but remains distinct from
real provider, phone, GitHub App, and production substrate acceptance.

Evidence smoke verification (2026-05-07): the focused smoke passed against
audit summary `/tmp/jam-external-acceptance.BcAXp3/summary.json`, and the core
local acceptance suite passed with the new smoke included at
`/tmp/jam-local-acceptance.5udTy1/summary.json`.

Current external blocker audit (2026-05-07): a fresh
`scripts/audit-external-acceptance.sh` run retained
`/tmp/jam-external-acceptance.9DD3E8/summary.json` with `status=failed`,
`failures=11`, and `manual_followups=6`. The audit summary now embeds
`failed_checks` and `manual_followup_checks` arrays in addition to the full
`summary.jsonl`, so the retained artifact is self-contained for the
interactive/root and manual acceptance handoff. The required failures are still
the production substrate install, production NATS/journal, missing GitHub App
installation ID, and `jam doctor`; the manual follow-ups are still Tailscale,
phone ntfy delivery, real provider/quota evidence, GitHub live PR evidence, and
the seven-day soak evidence.

External acceptance runbook note (2026-05-07): added
`docs/runbooks/external-acceptance.md` so the retained audit summary maps to
operator actions and concrete evidence files for the interactive/root install,
production NATS, GitHub App installation ID, Tailscale phone UI, ntfy phone
delivery, real provider/quota window, live PR push/comment, and seven-day soak.

Production doctor note (2026-05-07): `scripts/audit-external-acceptance.sh`
now prefers `/opt/jam/bin/jam doctor` and labels `target/debug/jam` as a
development fallback before the root install exists. This prevents the
production acceptance audit from accidentally relying on the repo build after
the installed CLI should be present.

Production doctor verification (2026-05-07): the audit summary at
`/tmp/jam-external-acceptance.ijjjHw/summary.json` now retains the doctor
failure detail as `target/debug/jam doctor (development fallback because
/opt/jam/bin/jam is not installed) still failing`, with the expected
`failures=11` and `manual_followups=6`.

Core suite reverification (2026-05-07): after the production-doctor audit
fallback change, `scripts/smoke-local-acceptance.sh --core` passed again and
retained `/tmp/jam-local-acceptance.4IfFTp/summary.json`.

Remediation summary note (2026-05-07): `scripts/audit-external-acceptance.sh`
now writes grouped `remediations` into `summary.json`, mapping each failed or
manual check class to the root/provider/phone/GitHub/soak action, expected
evidence file, and runbook anchor. This keeps `/tmp/jam-external-acceptance.*`
handoff artifacts actionable without terminal scrollback.

Remediation summary verification (2026-05-07): the real audit summary
`/tmp/jam-external-acceptance.echURV/summary.json` retained `status=failed`,
`failures=11`, `manual_followups=6`, and remediation groups for production
install, production substrate start, GitHub App installation ID, Tailscale
phone UI, ntfy phone delivery, real provider/quota window, GitHub live PR,
seven-day soak, and final doctor rerun.

Completion audit note (2026-05-07): added
`docs/runbooks/completion-audit.md` to map the active objective requirements to
current artifacts. The audit records local Phase 0/1 and deterministic smoke
evidence as complete locally, but keeps the objective incomplete because 16
graph tasks remain blocked and the external acceptance audit still reports 11
failures plus 6 manual follow-ups.

Completion audit refresh (2026-05-07): the latest live probe still reports no
backlog tasks, no in-progress tasks, and the same 16 blocked tasks. The
completion audit now points at `/tmp/jam-external-acceptance.g9YYgl/summary.json`,
which retains `status=failed`, `failures=11`, `manual_followups=6`, and 9
remediation groups.

Evidence validation reason note (2026-05-07): the external audit now preserves
the concrete evidence validation reason in `manual_followup_checks`, such as a
missing file, invalid JSON location, missing evidence key, invalid timestamp,
false verification boolean, or failed seven-day soak threshold.

Evidence validation reason verification (2026-05-07): the latest real audit
summary `/tmp/jam-external-acceptance.7B5CYM/summary.json` retained
`status=failed`, `failures=11`, `manual_followups=6`, and now reports every
missing manual evidence file as `file is missing or not a regular file` inside
`manual_followup_checks`.

Remediation filter note (2026-05-07): audit `remediations` now derive only from
failed and manual checks, not passed evidence rows. The external-audit evidence
smoke asserts that valid temporary ntfy/provider/GitHub-live/seven-day evidence
does not produce those remediation IDs. Verification summary
`/tmp/jam-external-acceptance.Uv8WOY/summary.json` retained only five
remediation groups after valid fake evidence, while the real audit summary
`/tmp/jam-external-acceptance.7B5CYM/summary.json` retained all nine current
external remediation groups.

Core suite remediation-filter verification (2026-05-07): after the remediation
filter change, `scripts/smoke-local-acceptance.sh --core` passed again and
retained `/tmp/jam-local-acceptance.8pf9kw/summary.json`, including the
stricter `external-audit-evidence` smoke.

Invalid evidence smoke note (2026-05-07): `scripts/smoke-external-audit-evidence.sh`
now runs both valid and intentionally invalid temporary evidence. The invalid
summary `/tmp/jam-external-acceptance.49FZnY/summary.json` verifies reasoned
manual follow-up details for invalid JSON, invalid ntfy urgency, empty provider
harness list, non-positive GitHub App installation ID, and seven-day soak
duration under threshold.

Repeatable completion audit note (2026-05-07): added
`scripts/audit-completion-state.sh`, a rootless completion-audit wrapper that
captures Tempyr backlog/in-progress/blocked counts, `tempyr validate`, the
latest local acceptance summary, and a fresh external acceptance audit into
`/tmp/jam-completion-audit.*/summary.json`. It exits nonzero while the
objective remains incomplete.

Repeatable completion audit verification (2026-05-07):
`scripts/audit-completion-state.sh` retained
`/tmp/jam-completion-audit.yo1IYa/summary.json` with `status=incomplete`,
`objective_complete=false`, graph counts `backlog=0`, `in_progress=0`,
`blocked=16`, local acceptance `passed`, and external acceptance
`status=failed`, `failures=11`, `manual_followups=6`.

Named task audit note (2026-05-07): `scripts/audit-completion-state.sh` now
also records explicit statuses for the required Phase 0/1 nodes named by the
active objective: LiteLLM backend, UI shell, NATS/JSONL substrate,
Maestro session loop, Codex Picker session service, and observe/world-snapshot.

Named task audit verification (2026-05-07): completion audit summary
`/tmp/jam-completion-audit.Jb0jnC/summary.json` retained
`required_tasks_all_done=true` for those six named tasks while still reporting
`objective_complete=false`, `blocked=16`, and external acceptance
`failures=11`, `manual_followups=6`.

Local smoke audit note (2026-05-07): `scripts/audit-completion-state.sh` now
parses the retained local acceptance `summary.jsonl` and records the required
core smoke statuses by name, including `external-audit-evidence`, instead of
trusting only the suite-level `status=passed`.

Local smoke audit verification (2026-05-07): completion audit summary
`/tmp/jam-completion-audit.fCwylM/summary.json` retained
`local_required_smokes_all_passed=true` for install-substrate,
maestro-runtime-journal, message-modes, research-service, search-service,
external-audit-evidence, and evolve-coordinator, while still reporting
`objective_complete=false` because external acceptance remains failed.

Fresh local completion audit note (2026-05-07): `scripts/audit-completion-state.sh`
now accepts `--run-local-core`, runs `scripts/smoke-local-acceptance.sh --core`
inside the completion audit, and records `local_acceptance_summary_source` plus
the core-smoke log path in the retained JSON summary.

Fresh local completion audit verification (2026-05-07):
`scripts/audit-completion-state.sh --run-local-core` retained
`/tmp/jam-completion-audit.R7qYfX/summary.json` with
`local_acceptance_summary_source=fresh-core-run`,
`local_required_smokes_all_passed=true`, `blocked=16`, and external
`failures=11`, `manual_followups=6`.
