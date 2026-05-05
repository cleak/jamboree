---
id: dec-skills-direct-read-with-config
type: decision
status: decided
created: 2026-05-04T15:58:05.488491726Z
updated: 2026-05-04T15:59:01.895660712Z
edges:
- target: comp-skills-source-config
  type: decision_for
- target: feat-self-improvement
  type: depended_on_by
- target: oq-runtime-skills-path
  type: answers
---
**Skills are read directly from configured locations — no symlinks, no sync step.** A config file declares both folders (recursively scanned) and individual file paths to load.

Resolves `oq-runtime-skills-path`.

**Config: `/home/maestro/.jam/config/skills.toml`** (per-instance):

```toml
[skills]
# Folders to search recursively for *.md skill files.
# Files matched are treated as skills using their YAML front-matter (`scope:`).
folders = [
    "/home/caleb/jamboree/skills/",
]

# Individual files to load (full paths).
# Useful for pulling in CLAUDE.md / AGENTS.md from project repos as skills.
files = [
    "/home/caleb/blueberry/CLAUDE.md",
    "/home/caleb/blueberry/AGENTS.md",
]
```

**Maestro behavior:**
- On every `read-skills(scope)` call, walk all `folders` and read all `files` from the config.
- Match scope hierarchically against each skill's `scope:` front-matter (or filename when no scope is set).
- Hot-edit works because the Maestro re-reads on each call (or when invalidated by `skills.changed` inotify events).
- Permission model: maestro user must have read access to all configured paths. For monorepo source: mode 2770 caleb:maestro on `skills/` so maestro can also write `record-learning` outputs.

**Why direct-read:**
- Symlinks couple two paths invisibly; debugging "why isn't this skill loading" is harder.
- Sync steps break hot-edit and add a failure mode.
- Config makes the runtime explicit and configurable per-instance.

**Why folders + individual files:**
- Folders for the natural Jamboree skills tree (`skills/Maestro.md`, `skills/projects/blueberry/*.md`, etc.).
- Individual files for project-side knowledge (Blueberry's `CLAUDE.md`, `AGENTS.md`) that's already curated and shouldn't be duplicated. Pickers can also pull in specific Blueberry docs as scoped skills.

**Where the config itself lives:** `/home/maestro/.jam/config/skills.toml`. Created by `bootstrap-users.sh` (or `jam setup`) with sane defaults pointing at `/home/caleb/jamboree/skills/`.