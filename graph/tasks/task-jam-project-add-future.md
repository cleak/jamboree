---
id: task-jam-project-add-future
type: task
status: cut
created: 2026-05-04T05:38:27.589059972Z
updated: 2026-05-06T20:43:30Z
edges:
- target: dec-single-project-per-instance
  type: blocked_by
- target: feat-jam-cli
  type: child_of
- target: task-document-blueberry-manual-onboarding
  type: blocked_by
---
**v2-or-later.** Implement `jam project add <name>` CLI that automates the manual steps from `task-document-blueberry-manual-onboarding`.

Per `dec-manual-project-onboarding-v1`. Manual setup is fine while N=1; revisit when a second project comes into scope.

Not blocking any v1 phase; not on the §12 phase plan. Tracked here so we don't forget.

Cut note (2026-05-06): `dec-single-project-per-instance` is stronger than the older "v2-or-later" idea. Jamboree remains Blueberry-only per instance; when a second project comes into scope, spin up a second Jamboree instance with its own substrate instead of adding `jam project add <name>` to this instance.
