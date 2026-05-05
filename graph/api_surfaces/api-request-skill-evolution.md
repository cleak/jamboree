---
id: api-request-skill-evolution
type: api_surface
status: draft
created: 2026-05-04T03:53:21.802124226Z
updated: 2026-05-04T04:58:46.767391998Z
edges:
- target: comp-jam-svc-evolve
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`request-skill-evolution(skill-name, eval-source?)` → triggers the Hermes evolution pipeline manually (§5.8, §4.4.7, §17.1).

Other triggers: periodic (default weekly), `skill.under-suspicion` events.

Output: candidate skill diff at `~/.jam/skills-evolution-candidates/<skill-name>.diff`. Human reviews via `git commit` on skills repo.