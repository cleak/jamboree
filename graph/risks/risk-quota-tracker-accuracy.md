---
id: risk-quota-tracker-accuracy
type: risk
status: identified
created: 2026-05-04T03:46:53.960242264Z
updated: 2026-05-04T03:46:53.960242655Z
---
**§13.3 Quota tracker accuracy.** Subscription-window quota counting depends on parsing harness logs and observed limit-hit events. Could drift from actual upstream state.

Mitigation: conservative-by-default (under-estimate remaining quota); periodic re-sync via observed limit responses; manual re-sync via `jam quota recalibrate`.