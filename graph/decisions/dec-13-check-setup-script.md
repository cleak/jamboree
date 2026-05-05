---
id: dec-13-check-setup-script
type: decision
status: decided
created: 2026-05-04T03:46:33.980515100Z
updated: 2026-05-04T05:03:22.149254594Z
edges:
- target: comp-jam-setup
  type: decision_for
---
**`jam setup` is a 13-check script that refuses to install on bad environment** (§11.4). Plus 11 multi-user additions per security-setup §10 = 24 total.

Why: failures must surface immediately (`principle-failure-surfaces-immediately`). Every check has a specific error and a specific remediation hint. No silent degradation at install time.

Same checks run by `jam doctor` at any time. Patch agent invokes after every patch. CI invokes as part of integration test suite.

After setup succeeds, `setup-result.json` is written to NATS KV (`setup-result` bucket); patch agent reads on first boot to know the verified-good baseline.