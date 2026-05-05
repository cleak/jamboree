---
id: api-record-learning
type: api_surface
status: draft
created: 2026-05-04T03:53:16.964844742Z
updated: 2026-05-04T04:33:00.463349007Z
edges:
- target: feat-maestro-tool-surface
  type: exposed_by
---
`record-learning(scope, evidence, guidance, counterexample, confidence, originated-from-trace?)` (§5.8, §7.1, §22.7).

Writes a structured skill note (markdown front-matter + body) AND emits a Tempyr `decision` (or `finding` if no decision was made) tagged with the relevant skill scope and the new skill's path.

Required front-matter: `scope`, `confidence`, `evidence`, `guidance`, `originated-from-trace`. Optional: `counterexample`.

Captured as `dec-record-learning-emits-dual` decision.