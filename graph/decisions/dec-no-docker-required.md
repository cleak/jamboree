---
id: dec-no-docker-required
type: decision
status: decided
created: 2026-05-04T03:46:48.236519036Z
updated: 2026-05-04T05:04:55.388126418Z
edges:
- target: comp-multi-user-filesystem-layout
  type: decision_for
---
**Multi-user model deliberately avoids Docker dependency** (security-setup §0). Convenience-first design means the orchestrator works with just users + sudoers + GPG/pass.

Docker (or Modal/SSH) backend is **additive** — turn it on per-task for hardening when needed (§6.2). Don't require it for the baseline.

Why: Docker adds operational complexity (daemon management, image storage, network setup). For a single-developer machine where the threat model is prompt-injection-driven exfiltration, user-level isolation is sufficient.