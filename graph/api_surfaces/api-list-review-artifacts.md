---
id: api-list-review-artifacts
type: api_surface
status: stable
created: 2026-05-04T03:51:53.264181575Z
updated: 2026-05-06T21:26:08Z
edges:
- target: comp-jam-svc-observe
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`list-review-artifacts(pr-ref, status-filter?)` → `Vec<ReviewArtifact>` (§5.1, §4.2.4).

`ReviewArtifact.body` is `Untrusted<String>` — untrusted-content discipline (§11.2.4).

Implementation note (2026-05-06): `jam-svc-observe` now serves
`tool.observe.list-review-artifacts` from journal-derived
`pr.review-received` summary events. It supports `pr-ref` / `status-filter`
request aliases and returns summary artifacts with `body_trust: untrusted`.
Full comment bodies remain owned by `read-pr-comments`.

Live smoke (2026-05-06): temporary NATS plus `jam-svc-observe` with a temp
`JAM_JOURNAL_ROOT` returned the expected `review-summary:cleak/blueberry#42:*`
record for `tool.observe.list-review-artifacts`, including
`artifact_count=4` and `body_trust=untrusted`.

Maestro route note (2026-05-06): `list-review-artifacts` is exposed through
`MaestroToolRegistry` with generated typed `pr_ref` / `status_filter`
validation.
