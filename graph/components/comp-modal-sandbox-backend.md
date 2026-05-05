---
id: comp-modal-sandbox-backend
type: component
status: planned
created: 2026-05-04T03:39:26.353412354Z
updated: 2026-05-04T04:44:45.109660995Z
edges:
- target: comp-sandbox-backend-trait
  type: depends_on
- target: feat-sandboxing-profile-x-backend
  type: used_by
- target: principle-native-fs-only
  type: constrained_by
- target: principle-sandbox-blast-radius-not-behavior
  type: constrained_by
---
Modal serverless function. Elastic; pay-per-second (§6.2). Use case: hardened × modal for elastic burst capacity.

Worktree-only: hard. Ephemeral container; same shape as docker (§6.12).

`full-stop` for Modal: API call to terminate the function (§5.7 `full-stop` *Implementation*).