---
id: principle-structure-in-tools-not-policy
type: constraint
status: active
created: 2026-05-04T03:23:48.006221658Z
updated: 2026-05-04T04:12:22.152009624Z
edges:
- target: feat-maestro-tool-surface
  type: constrains
---
**§2.3 Structure lives in tools, not policy.**

The Maestro's behavior is shaped by which tools exist and what they do, not by hardcoded policy in code. Want to disallow a behavior? Don't put a tool there. Want to enforce an invariant? Build it into the tool's contract.

Concrete: there is no `merge-pr` tool. The Maestro cannot merge PRs. Period. If the Maestro decides a PR is ready, it calls `request-human-merge`. The invariant ("merge requires human") lives in the tool shape.

*Why:* policy-checks-on-the-Maestro are easily bypassed by a creative agent. Tool-shape invariants are mechanically impossible to bypass — there's nothing to call. The "no tool, no possibility" rule.