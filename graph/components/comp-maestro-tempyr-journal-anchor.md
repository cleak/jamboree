---
id: comp-maestro-tempyr-journal-anchor
type: component
status: active
created: 2026-05-04T03:31:30.488684421Z
updated: 2026-05-06T21:18:00Z
edges:
- target: comp-maestro-session-loop
  type: depended_on_by
- target: feat-maestro-orchestration-loop
  type: used_by
---
Maestro anchors its Tempyr journal at the **canonical Tempyr worktree** with **per-wake unique agent identifier** (§4.1.6, §22.2):

- `worktree`: `~/code/<project>-tempyr-live/`.
- `agent`: `maestro:<maestro-session-id>` (e.g. `maestro:maestro-2026-05-02-08-15-22`).

Each Maestro wake opens a fresh Tempyr session because the agent identifier is unique per wake. Session closes when the wake ends — via an `outcome` entry with `final = true` or via `tempyr journal finalize` invoked by the cleanup path. After finalize, `tempyr journal flush` runs in background to publish session as a git ref.

Decisions land as `decision` entries (Tempyr's `chosen`/`rationale`/`reversible` required, `detail` ≥ 50 chars). Findings land as `finding`. Failed tool-call approaches land as `dead_end` with implicating skill tagged.

Why anchor here: Maestro doesn't naturally have a worktree; canonical worktree is the obvious anchor (orchestrator owns it; Tempyr's task graph nodes live there; persists across reboots).

Implementation note (2026-05-06): `MaestroSessionLoop` opens and finalizes
Tempyr journal sessions through `CliTempyrJournal`, using a per-wake agent id
and recording decisions / outcomes tagged with the active trace. Unit and NATS
smoke coverage verify decisions are journaled during task wakes.
