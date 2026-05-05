---
id: risk-schema-codegen-drift
type: risk
status: identified
created: 2026-05-04T03:47:16.149157979Z
updated: 2026-05-04T03:47:16.149158505Z
---
**§13.16 Schema codegen drift (NEW v5).** `events.toml` could fall out of sync with consumers.

Mitigation: pre-commit hook regenerates and verifies; CI re-checks; consumers fail loudly on unknown event types or missing required fields rather than silently mis-parsing.