---
id: dec-pass-and-gpg-for-secrets
type: decision
status: decided
created: 2026-05-04T03:46:18.293831908Z
updated: 2026-05-04T05:02:16.999505041Z
edges:
- target: comp-pass-secret-backend
  type: decision_for
- target: feat-tech-stack-hardening
  type: depended_on_by
---
**`pass` (encrypted with GPG) as primary secret backend; file fallback** (§11.3).

Why: `pass` is the standard Unix password manager — encrypted on disk via GPG. Battle-tested. CLI integration is straightforward.

`~/.jam/config/secrets.toml` (chmod 600) is the file fallback when `pass` is unavailable.

Conventional naming: `jam/<scope>/<key>` (full list §11.3.1).

Under multi-user model: orchestrator's `pass` belongs to `maestro` user; caleb's personal pass stays separate (security-setup §5.4). A compromise of one doesn't expose the other.