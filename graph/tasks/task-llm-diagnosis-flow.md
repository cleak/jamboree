---
id: task-llm-diagnosis-flow
type: task
status: done
created: 2026-05-04T04:00:30.150315048Z
updated: 2026-05-06T16:06:41Z
edges:
- target: feat-hot-patching
  type: child_of
---
Phase 7 (§12). LLM diagnosis flow with $0.50 budget cap, single-turn.

Per `dec-patch-agent-deterministic-then-llm`, §20.5 step C.

Implementation note (2026-05-06): accepted. `jam-patch-agent` runs a focused
single-turn diagnosis through `JAM_PATCH_AGENT_LLM_CMD`, records the default
`$0.50` budget cap in `llm-diagnosis.json`, feeds the patch payload,
post-apply/post-rollback health reports, rollback command report, and last
1000 journal events, parses the required menu
`restart-service|rollback-to-version|ntfy-with-incident-dump`, applies at most
one successful suggested recovery action, and reruns health checks. If the
action restores health it emits the same recovery/confirmation events plus a
low-urgency `notify.human`; otherwise the incident dump + critical notify +
dispatch-pause path remains the terminal behavior.

Verification (2026-05-06): `cargo fmt --all -- --check`,
`cargo clippy -p jam-patch-agent --all-targets -- -D warnings`,
`cargo test -p jam-patch-agent`, and `scripts/smoke-patch-agent-recovery.sh`.
