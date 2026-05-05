---
id: feat-implementation-walkthrough-reference
type: feature
status: active
created: 2026-05-04T03:28:27.271348617Z
updated: 2026-05-04T05:07:07.672895812Z
owner: caleb
edges:
- target: jamboree-v5
  type: child_of
- target: note-build-order-phases
  type: relates_to
- target: the-manager
  type: serves
---
§24 of the spec is a **worked end-to-end implementation walkthrough**: a task's path from spawn through merge with the code paths involved at each step. Reference document for implementers.

Scenario walked: `jam task spawn 'Refactor canyon generator to use spline-based seam protocols'` at 08:15 → Maestro wake → `world-snapshot` → spawn-picker (codex-cli) → Picker reasoning + Tempyr journal entries → PR opened → CodeRabbit comments → Maestro wake on review → reply with rationale → human merge next day → Maestro records learning → tempyr-pr-reconciler emits update candidates.

The walkthrough touches: trace propagation across §23 boundaries, multi-trigger correlation (§24.5 on traces A/B/C), failure recovery branches (§24.8: stalled Picker, prompt-injection in comment, tool-service crash, canonical-worktree corruption).

Implementation order recap (§24.9):
1. Directory layout (§11.1) → workspace.
2. `jam-events`, `jam-trace`, `jam-secrets` first.
3. Codegen pipeline.
4. NATS + journal writer.
5. Setup script + jam doctor.
6. One tool service (`jam-svc-observe`).
7. Maestro MVP.
8. Spawn-Picker (Codex CLI).
9. Tempyr canonical worktree + journal.
10. Trace-replay tool.
11. Iterate per §12 phase plan.