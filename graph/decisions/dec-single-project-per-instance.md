---
id: dec-single-project-per-instance
type: decision
status: decided
created: 2026-05-04T05:37:59.727702259Z
updated: 2026-05-04T05:38:55.340228014Z
edges:
- target: feat-jam-cli
  type: depended_on_by
- target: feat-multi-user-security-model
  type: depended_on_by
- target: task-jam-project-add-future
  type: blocks
---
**Jamboree runs one project per instance.** Multi-project deployments = multiple Jamboree instances, each with its own substrate (NATS, journal, session-store, canonical worktree, skills repo).

For v1: **Blueberry only**. Anything other than Blueberry is explicitly out of scope. Do not generalize for multi-project.

Implications:
- `~/.jam/config/projects/<project>.toml` directory still exists (per spec §11.1) but it always contains exactly one project file (`blueberry.toml`) for now.
- Maestro tool calls / world-snapshots can assume the project context.
- NATS subjects don't need project-scoping.
- Cross-project quota juggling is a non-concern.
- Skills repo (`~caleb/code/jam-skills/`) is Blueberry-flavored.

When/if a second project is targeted, the answer is "spin up a second Jamboree instance with its own users (`maestro2`/`picker2`)" — not "extend the orchestrator."
