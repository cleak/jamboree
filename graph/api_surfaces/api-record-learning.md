---
id: api-record-learning
type: api_surface
status: stable
created: 2026-05-04T03:53:16.964844742Z
updated: 2026-05-06T21:26:08Z
edges:
- target: feat-maestro-tool-surface
  type: exposed_by
---
`record-learning(scope, evidence, guidance, counterexample, confidence, originated-from-trace?)` (§5.8, §7.1, §22.7).

Writes a structured skill note (markdown front-matter + body) AND emits a Tempyr `decision` (or `finding` if no decision was made) tagged with the relevant skill scope and the new skill's path.

Required front-matter: `scope`, `confidence`, `evidence`, `guidance`, `originated-from-trace`. Optional: `counterexample`.

Captured as `dec-record-learning-emits-dual` decision.

Implementation note (2026-05-06): current callable name is `record-learning` in `MaestroToolRegistry`, routed as local meta subject `meta.record-learning` with `RecordLearningRequest` validation.
