---
id: task-skill-watcher-inotify
type: task
status: backlog
created: 2026-05-04T04:01:30.388581426Z
updated: 2026-05-04T04:01:30.388582045Z
---
inotify watcher on `~caleb/code/jam-skills/` (under multi-user model) emits `skills.changed{file_path}`. Maestro skill cache invalidates affected scope.

Per `comp-jam-svc-knowledge`, §21.4, §21.5.

Acceptance: edit a skill file → next `read-skills(scope)` call returns updated content. No restart.

Constraint: `fs.inotify.max_user_watches >= 524288` per `constraint-inotify-watches-524k`.