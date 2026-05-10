---
id: api-read-skills
type: api_surface
status: stable
created: 2026-05-04T03:52:40.863486122Z
updated: 2026-05-06T21:31:44Z
edges:
- target: comp-jam-svc-knowledge
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`read-skills(scope?)` → loads relevant skill files into context, returns front-matter + body (§5.5, §4.1.3 mitigation A).

Relevance-scoped skill loading per §4.1.3 — scope hierarchical (e.g., `blueberry/coderabbit-review/canyon-area`); matches Maestro.md, global.md, projects/*, task-types/*, harnesses/*, reviewers/*.

Maybe 8-15 skills loaded per call, not 50+.

Implementation note (2026-05-06): the local Maestro meta-tool route is `read-skills` → `meta.read-skills` with `ReadSkillsRequest` validation. It uses `FileSkillLoader`, re-reads the configured skills paths on each call, and supports the current Blueberry project plus `task-types/<task-class>` scope shape.
