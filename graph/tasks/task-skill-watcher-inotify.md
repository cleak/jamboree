---
id: task-skill-watcher-inotify
type: task
status: done
created: 2026-05-04T04:01:30.388581426Z
updated: 2026-05-06T07:41:21.156448439Z
---
inotify watcher on `~caleb/code/jam-skills/` (under multi-user model) emits `skills.changed{file_path}`. Maestro skill cache invalidates affected scope.

Per `comp-jam-svc-knowledge`, §21.4, §21.5.

Acceptance: edit a skill file → next `read-skills(scope)` call returns updated content. No restart.

Constraint: `fs.inotify.max_user_watches >= 524288` per `constraint-inotify-watches-524k`.

Implementation note (2026-05-06): `crates/jam-svc-knowledge` now includes the first knowledge-service slice: a recursive Linux inotify watcher over the configured skills folders and individual skill files. It reads the same `skills.toml` shape as the Maestro loader (`folders` + `files`), refuses to start when `fs.inotify.max_user_watches < 524288`, and emits traced `journal.skills.changed` events with payload `{file_path, ts}`. The generated event type is `skills.changed`.

Live smoke (2026-05-06): temporary NATS root `/tmp/jam-skill-watcher-smoke-541LBa` watched `/tmp/jam-skill-watcher-smoke-541LBa/skills`, edited `projects/blueberry/hot-paths.md`, and wrote one `journal.skills.jsonl` entry: `skills.changed`, trace `01KQY3QHPCPGH57MT4AKQRYCHQ`, actor `jam-svc-knowledge`, file path `/tmp/jam-skill-watcher-smoke-541LBa/skills/projects/blueberry/hot-paths.md`. The same smoke called `FileSkillLoader.load(SkillScope(project="blueberry"))` after the edit and read the updated `second` content without restarting.
