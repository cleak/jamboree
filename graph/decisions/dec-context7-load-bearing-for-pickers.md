---
id: dec-context7-load-bearing-for-pickers
type: decision
status: decided
created: 2026-05-04T03:46:16.778158777Z
updated: 2026-05-04T05:02:07.507980758Z
edges:
- target: comp-context7-mcp
  type: decision_for
- target: feat-mcp-integration
  type: depended_on_by
---
**Context7 always-on for any Picker doing code work** (§4.9). Load-bearing for fast-moving-library workloads.

Why: Bevy ships breaking changes per minor release; LLM training data always lags. Context7 has version-pinned doc indexes (`bevy_ecs/0.16.1`, Tokio `1.49.0`) that solve the "model writes 0.13 patterns into a 0.16 codebase" failure mode.

The Maestro itself does not need it — delegate research-with-docs subtasks to a Picker rather than spending Maestro tool-call budget on doc lookups.