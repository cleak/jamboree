---
id: task-jam-svc-evolve-coordinator
type: task
status: blocked
created: 2026-05-04T03:59:41.047314943Z
updated: 2026-05-06T16:39:56.485115632Z
edges:
- target: feat-skill-evolution-pipeline
  type: child_of
---
Phase 5 (§12). `jam-svc-evolve` coordinating pipeline runs.

Per `comp-jam-svc-evolve`.

Implementation note (2026-05-06): the first coordinator slice is implemented in `crates/jam-svc-evolve`. The service handles traced NATS request/reply on `tool.evolve.request-skill-evolution`, resolves Jamboree skill scopes/files under the configured skills dir, and invokes `evolution/jamboree_evolve_skill.py` through `uv run --with-editable evolution/hermes-agent-self-evolution`. `scripts/smoke-evolve-coordinator.sh` passed with live NATS and `JAM_EVOLVE_DRY_RUN=true`, proving the route and subprocess boundary without a model call.

Blocked note (2026-05-06): full acceptance remains blocked on a real DSPy/GEPA optimization run because no compatible LLM credential is configured locally. To finish this task, seed the optimizer/eval model credential, verify `task-vendor-hermes-evolution` with a real candidate diff, then rerun `tool.evolve.request-skill-evolution` with dry-run disabled and verify it writes a candidate under `~/.jam/skills-evolution-candidates/`.
