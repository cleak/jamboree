"""Skill loading for Maestro wake sessions."""

from __future__ import annotations

import os
import tomllib
from pathlib import Path
from typing import TYPE_CHECKING, Protocol, cast

from pydantic import Field

from jam_maestro.models import StrictBaseModel
from jam_maestro.paths import jam_home

if TYPE_CHECKING:
    from collections.abc import Iterable, Mapping


class SkillScope(StrictBaseModel):
    """Scope used for a wake's skill read."""

    project: str = "blueberry"
    task_class: str | None = None


class ReadSkillsRequest(StrictBaseModel):
    """Inputs for the local `read-skills` Maestro meta-tool."""

    scope: str | None = Field(default=None, min_length=1)
    project: str | None = Field(default=None, min_length=1)
    task_class: str | None = Field(default=None, min_length=1)


class LoadedSkill(StrictBaseModel):
    """One skill file loaded into a Maestro session."""

    path: str
    content: str = Field(min_length=1)


class SkillLoader(Protocol):
    """Loads skills relevant to a wake scope."""

    async def load(self, scope: SkillScope) -> list[LoadedSkill]:
        """Return ordered skill files for `scope`."""
        ...


class NullSkillLoader:
    """Test-only loader used when a unit test does not care about skills."""

    async def load(self, scope: SkillScope) -> list[LoadedSkill]:
        _ = scope
        return []


class FileSkillLoader:
    """Load skills directly from configured markdown paths."""

    def __init__(
        self,
        *,
        skills_config: Path | None = None,
        default_root: Path | None = None,
    ) -> None:
        self._skills_config = skills_config or _default_skills_config()
        self._default_root = default_root or Path("/home/caleb/jamboree/skills")

    async def load(self, scope: SkillScope) -> list[LoadedSkill]:
        folders, files = self._configured_paths()
        selected = _select_paths(folders, files, scope)
        return [
            LoadedSkill(path=str(path), content=path.read_text(encoding="utf-8"))
            for path in selected
        ]

    def _configured_paths(self) -> tuple[list[Path], list[Path]]:
        if not self._skills_config.exists():
            return [self._default_root], []

        with self._skills_config.open("rb") as handle:
            raw = cast("dict[str, object]", tomllib.load(handle))
        root_obj = raw.get("skills")
        if not isinstance(root_obj, dict):
            return [self._default_root], []
        root = cast("Mapping[str, object]", root_obj)

        folders = _paths_from_config(root.get("folders"))
        files = _paths_from_config(root.get("files"))
        return folders or [self._default_root], files


async def read_skills(
    request: ReadSkillsRequest,
    *,
    loader: SkillLoader | None = None,
) -> list[LoadedSkill]:
    """Load scope-matched skills for the `read-skills` meta-tool."""
    active_loader = loader or FileSkillLoader()
    return await active_loader.load(_skill_scope_from_request(request))


def _skill_scope_from_request(request: ReadSkillsRequest) -> SkillScope:
    project = request.project or "blueberry"
    task_class = request.task_class
    if request.scope:
        parts = [part for part in request.scope.split("/") if part]
        if parts:
            project = request.project or parts[0]
        if task_class is None:
            task_class = _task_class_from_scope(parts)
    return SkillScope(project=project, task_class=task_class)


def _task_class_from_scope(parts: list[str]) -> str | None:
    if "task-types" in parts:
        index = parts.index("task-types")
        if index + 1 < len(parts):
            return parts[index + 1]
    return None


def _default_skills_config() -> Path:
    explicit = os.environ.get("JAM_SKILLS_CONFIG")
    return Path(explicit) if explicit else jam_home() / "config" / "skills.toml"


def _paths_from_config(value: object) -> list[Path]:
    if not isinstance(value, list):
        return []
    items = cast("list[object]", value)
    return [Path(item) for item in items if isinstance(item, str)]


def _select_paths(folders: Iterable[Path], files: Iterable[Path], scope: SkillScope) -> list[Path]:
    candidates: list[Path] = []
    for folder in folders:
        if folder.is_dir():
            candidates.extend(sorted(folder.rglob("*.md")))
    candidates.extend(path for path in files if path.is_file())

    selected: list[Path] = []
    seen: set[Path] = set()
    for path in candidates:
        if not path.is_file():
            continue
        if not _matches_scope(path, scope):
            continue
        resolved = path.resolve()
        if resolved in seen:
            continue
        seen.add(resolved)
        selected.append(path)
    return selected


def _matches_scope(path: Path, scope: SkillScope) -> bool:
    parts = path.parts
    name = path.name
    if name in {"Maestro.md", "global.md"}:
        return True

    if _contains_segments(parts, "projects", scope.project):
        return True

    if scope.task_class and _contains_segments(parts, "task-types", f"{scope.task_class}.md"):
        return True

    # Project-side CLAUDE.md / AGENTS.md files are configured as individual
    # files. They are scoped to the current single-project instance.
    return name in {"CLAUDE.md", "AGENTS.md"} and scope.project == "blueberry"


def _contains_segments(parts: tuple[str, ...], first: str, second: str) -> bool:
    return any(part == first and parts[idx + 1] == second for idx, part in enumerate(parts[:-1]))
