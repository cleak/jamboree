from __future__ import annotations

import asyncio
from typing import TYPE_CHECKING

from jam_maestro.skills import FileSkillLoader, ReadSkillsRequest, SkillScope, read_skills

if TYPE_CHECKING:
    from pathlib import Path


def test_file_skill_loader_loads_always_project_and_task_type(tmp_path: Path) -> None:
    root = tmp_path / "skills"
    (root / "projects" / "blueberry").mkdir(parents=True)
    (root / "task-types").mkdir(parents=True)
    (root / "Maestro.md").write_text("# Maestro\n", encoding="utf-8")
    (root / "global.md").write_text("# Global\n", encoding="utf-8")
    (root / "projects" / "blueberry" / "overview.md").write_text("# Blueberry\n", encoding="utf-8")
    (root / "task-types" / "light-edit.md").write_text("# Light edit\n", encoding="utf-8")
    (root / "task-types" / "ecs-refactor.md").write_text("# ECS\n", encoding="utf-8")

    loader = FileSkillLoader(skills_config=tmp_path / "missing.toml", default_root=root)
    loaded = asyncio.run(loader.load(SkillScope(project="blueberry", task_class="light-edit")))

    paths = {skill.path for skill in loaded}
    assert str(root / "Maestro.md") in paths
    assert str(root / "global.md") in paths
    assert str(root / "projects" / "blueberry" / "overview.md") in paths
    assert str(root / "task-types" / "light-edit.md") in paths
    assert str(root / "task-types" / "ecs-refactor.md") not in paths


def test_file_skill_loader_rereads_hot_edited_skill(tmp_path: Path) -> None:
    root = tmp_path / "skills"
    root.mkdir()
    skill = root / "Maestro.md"
    skill.write_text("# Maestro\nfirst\n", encoding="utf-8")

    loader = FileSkillLoader(skills_config=tmp_path / "missing.toml", default_root=root)

    first = asyncio.run(loader.load(SkillScope(project="blueberry")))
    skill.write_text("# Maestro\nsecond\n", encoding="utf-8")
    second = asyncio.run(loader.load(SkillScope(project="blueberry")))

    assert first[0].content == "# Maestro\nfirst\n"
    assert second[0].content == "# Maestro\nsecond\n"


def test_read_skills_meta_tool_accepts_hierarchical_scope(tmp_path: Path) -> None:
    root = tmp_path / "skills"
    (root / "task-types").mkdir(parents=True)
    (root / "Maestro.md").write_text("# Maestro\n", encoding="utf-8")
    (root / "task-types" / "light-edit.md").write_text("# Light edit\n", encoding="utf-8")
    (root / "task-types" / "ecs-refactor.md").write_text("# ECS\n", encoding="utf-8")

    loader = FileSkillLoader(skills_config=tmp_path / "missing.toml", default_root=root)

    loaded = asyncio.run(
        read_skills(
            ReadSkillsRequest(scope="blueberry/task-types/light-edit"),
            loader=loader,
        )
    )

    paths = {skill.path for skill in loaded}
    assert str(root / "Maestro.md") in paths
    assert str(root / "task-types" / "light-edit.md") in paths
    assert str(root / "task-types" / "ecs-refactor.md") not in paths
