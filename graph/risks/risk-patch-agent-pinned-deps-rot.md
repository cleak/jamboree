---
id: risk-patch-agent-pinned-deps-rot
type: risk
status: identified
created: 2026-05-04T03:47:17.992195066Z
updated: 2026-05-04T03:47:17.992195478Z
---
**§13.17 Patch agent pinned-deps rot (NEW v5).** The patch agent's intentionally-pinned dependencies will fall behind security advisories.

Mitigation: `cargo audit` runs in CI on the patch-agent crate specifically; updates are batched manually with deliberate review; the patch agent's tiny dependency surface keeps the rot small.