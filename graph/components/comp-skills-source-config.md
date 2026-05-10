---
id: comp-skills-source-config
type: component
status: active
created: 2026-05-04T15:58:23.994151421Z
updated: 2026-05-04T15:59:01.895660399Z
edges:
- target: dec-skills-direct-read-with-config
  type: has_decision
- target: feat-self-improvement
  type: used_by
---
**`/home/maestro/.jam/config/skills.toml`** — declares where the Maestro reads skills from. Per `dec-skills-direct-read-with-config`.

Schema:
```toml
[skills]
folders = ["/path/", ...]    # recursive scan for *.md files
files = ["/path/file.md", ...]  # individual files
```

The `read-skills(scope)` tool reads this config on every call and walks all configured paths. Skill files are matched against scope via their YAML `scope:` front-matter (or path if absent).

Hot-edit: any edit to a configured file or any new file in a configured folder is picked up on the next `read-skills` call. inotify watcher (`comp-jam-svc-knowledge`) fires `skills.changed{file_path}` to invalidate Maestro's per-session cache.

Created by `bootstrap-users.sh` (or `jam setup`) with default contents:
```toml
[skills]
folders = ["/home/caleb/jamboree/skills/"]
files = ["/home/caleb/blueberry/CLAUDE.md", "/home/caleb/blueberry/AGENTS.md"]
```

Operator can extend by editing the file directly. No restart needed (`maestro` re-reads on next session).

Implementation note (2026-05-06): both Python `FileSkillLoader` and Rust `jam-svc-knowledge` resolve the config as `$JAM_SKILLS_CONFIG` when set, otherwise `$JAM_HOME/config/skills.toml`; `JAM_HOME` itself now follows the shared current-user rule from security-setup §7.1.
