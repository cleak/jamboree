---
id: dec-ulid-for-trace-ids
type: decision
status: decided
created: 2026-05-04T03:46:21.896867556Z
updated: 2026-05-04T05:01:31.457230021Z
edges:
- target: comp-jam-trace-crate
  type: decision_for
- target: feat-trace-propagation
  type: depended_on_by
---
**ULID for trace IDs** (§23.2). 26-char Base32 string. Time-sortable. Globally unique. Universal pattern: `^[0-9A-HJKMNP-TV-Z]{26}$`.

Why ULID over UUID v4: time-sortable means traces sort in roughly chronological order without requiring a separate timestamp index. Base32 is easier to copy-paste than hex. Length is acceptable in NATS headers and JSON payloads.