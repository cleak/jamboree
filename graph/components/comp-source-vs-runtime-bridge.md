---
id: comp-source-vs-runtime-bridge
type: component
status: active
created: 2026-05-04T03:40:08.163084996Z
updated: 2026-05-06T21:22:00Z
edges:
- target: comp-monorepo-tree
  type: depends_on
- target: comp-multi-user-filesystem-layout
  type: depends_on
- target: feat-monorepo-layout
  type: used_by
---
Two distinct concepts (layout.md §Source vs. runtime):

| | Lives at | Owned by | Purpose |
|---|---|---|---|
| **Source** | `/home/caleb/jamboree/` | `caleb` | Editable, version-controlled, the only thing humans modify |
| **Runtime** | `/home/maestro/.jam/` | `maestro` | What the Maestro process actually executes against |

Bridge: `jam patch apply` (§21.6) — built artifacts staged from source, validated, atomically swapped into runtime. Until that flow exists (Phase 3+), runtime layout is documented in security-setup §7.

Source-of-truth lives at `~caleb/jamboree/` regardless of where it runs. Build artifacts cross the user boundary via `jam patch apply` (§21.6).

Implementation note (2026-05-06): the first runtime bridge path is active in
`jam patch apply` / `jam patch rollback`. Candidate service binaries are staged
under `JAM_HOME/staging`, installed into the runtime bin dir, health-gated, and
published through the routing manifest without moving source-of-truth out of the
Caleb-owned checkout. Production install to `/opt/jam/bin` remains a root-shell
operator step.
