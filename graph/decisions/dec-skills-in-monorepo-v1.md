---
id: dec-skills-in-monorepo-v1
type: decision
status: decided
created: 2026-05-04T05:53:40.435463793Z
updated: 2026-05-04T05:55:23.998030960Z
edges:
- target: feat-multi-user-security-model
  type: depended_on_by
- target: feat-self-improvement
  type: depended_on_by
---
**For v1: skills live inside the Jamboree monorepo at `/home/caleb/jamboree/skills/`.** No separate `~caleb/code/jam-skills/` repo.

Rationale: targeting one project (Blueberry per `dec-single-project-per-instance`); a separate repo is overhead without payoff. Learn the general shapes first; decompose later if a multi-project deploy emerges.

Implications:
- All references to `~/.jam/skills/` (runtime) and `~caleb/code/jam-skills/` (source) collapse to `/home/caleb/jamboree/skills/`.
- Skills directory in monorepo is mode 2770 with group `maestro` so the orchestrator can write `record-learning` outputs and skill evolution candidates (TBD whether candidates land elsewhere).
- The `feat-multi-user-security-model` shared-dir pattern still applies — just at this path.
- **Open question:** runtime path resolution — does the orchestrator read directly from `/home/caleb/jamboree/skills/`, or is there a syncing step? See `oq-runtime-skills-path`.

Reversibility: when/if multiple projects emerge, extract via `git filter-repo --subdirectory-filter skills/`. No cost to changing now.