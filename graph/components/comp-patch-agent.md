---
id: comp-patch-agent
type: component
status: planned
created: 2026-05-04T03:31:47.051820592Z
updated: 2026-05-04T05:05:51.825671235Z
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