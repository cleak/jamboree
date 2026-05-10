---
id: comp-patch-agent
type: component
status: active
created: 2026-05-04T03:31:47.051820592Z
updated: 2026-05-06T16:06:41Z
edges:
- target: comp-atomic-swap-procedure
  type: depends_on
- target: comp-jam-setup
  type: depends_on
- target: comp-rollback-flow
  type: depends_on
- target: comp-routing-manifest
  type: depends_on
- target: dec-patch-agent-deterministic-then-llm
  type: has_decision
- target: feat-failure-handling
  type: used_by
- target: feat-hot-patching
  type: used_by
- target: feat-substrate-services
  type: used_by
- target: insight-deterministic-then-llm-pattern
  type: relates_to
- target: principle-failure-surfaces-immediately
  type: constrained_by
---
Separate Rust crate `crates/jam-patch-agent/` (§4.4.8, §20.5). Pinned dependencies: `tokio`, `serde`, `tracing`, `nats`, `octocrab` (for ntfy proxying), one LLM client (default Claude Haiku 4.5 or GPT-5.5-mini for cost).

Activates on `patch.applied` events. Procedure (§20.5):

A. **Deterministic health checks** (cheap, near-zero LLM cost):
1. `tool.<service>.ping` responds within 5s.
2. Smoke test: known-safe method returns valid shape.
3. `jam doctor` passes.
4. No `*.failed` events in past 60s for the patched service.

B. **If A fails:** mechanical rollback → re-run health checks → if healthy emit `patch.rolled-back-successfully` + ntfy FYI; if still unhealthy escalate to C.

C. **LLM diagnosis** (incurs cost): focused session, $0.50 budget cap, single-turn, fed recent journal events + health check failures + manifest before/after + `jam doctor` output. Asked to suggest from menu: `[restart-service, rollback-to-version, ntfy-with-incident-dump]`.

D. **If C fails or budget exceeded:** write incident dump to `~/.jam/incidents/<id>/` → ntfy critical → pause-dispatch → patch agent exits.

Patches serialized via supervisor's NATS KV `patch-lock` (TTL 5min). Patch-on-patch is queued.

Patch agent's own `patch-agent.md` skill file (§9) explains the escalation strategy and trace-replay procedure.

Implementation status (2026-05-06): `crates/jam-patch-agent` is active in the Rust workspace and `process-compose.yaml` already declares the long-running `jam-patch-agent` process. The agent subscribes to `patch.applied`, runs deterministic checks (`ping`, observe `list-blockers` smoke shape, `jam doctor`, and recent service-scoped `patch.failed` events from the `patch` JetStream stream), emits `patch.confirmed` for healthy patches, retries mechanical rollback while `jam patch apply` is still releasing `patch-lock/current`, relaunches the rolled-back manifest route when the old process has drained, emits `patch.rolled-back-successfully` + low-urgency `notify.human` on recovery, and writes `~/.jam/incidents/<id>/` plus `patch.failed`, critical `notify.human`, and dispatch pause state when recovery fails.

LLM diagnosis is wired through the focused `JAM_PATCH_AGENT_LLM_CMD` single-turn command hook with a default `$0.50` budget cap recorded in the incident transcript. The prompt includes the patch payload, health reports, rollback command report, and last 1000 journal events. Successful menu suggestions trigger exactly one recovery action (`restart-service` or `rollback-to-version`) followed by health checks; failed, unconfigured, or incident-directed diagnoses write the incident dump and pause dispatch. The live smoke `scripts/smoke-patch-agent-recovery.sh` verifies both acceptance paths: a deliberately broken observe patch is rolled back and confirmed healthy, and a rollback-unhealthy patch runs `/bin/false` as the LLM hook, records the failed attempt in `llm-diagnosis.json`, publishes `patch.failed` / `notify.human`, and exits non-zero.
